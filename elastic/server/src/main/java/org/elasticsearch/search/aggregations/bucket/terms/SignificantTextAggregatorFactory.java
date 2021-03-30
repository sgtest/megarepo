/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations.bucket.terms;

import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.analysis.miscellaneous.DeDuplicatingTokenFilter;
import org.apache.lucene.analysis.miscellaneous.DuplicateByteSequenceSpotter;
import org.apache.lucene.analysis.tokenattributes.CharTermAttribute;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.BytesRefBuilder;
import org.elasticsearch.common.lease.Releasables;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.BytesRefHash;
import org.elasticsearch.common.util.ObjectArray;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.TextSearchInfo;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.Aggregator.SubAggCollectionMode;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.CardinalityUpperBound;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.LeafBucketCollector;
import org.elasticsearch.search.aggregations.LeafBucketCollectorBase;
import org.elasticsearch.search.aggregations.NonCollectingAggregator;
import org.elasticsearch.search.aggregations.bucket.BucketUtils;
import org.elasticsearch.search.aggregations.bucket.terms.IncludeExclude.StringFilter;
import org.elasticsearch.search.aggregations.bucket.terms.MapStringTermsAggregator.CollectConsumer;
import org.elasticsearch.search.aggregations.bucket.terms.TermsAggregator.BucketCountThresholds;
import org.elasticsearch.search.aggregations.bucket.terms.heuristic.SignificanceHeuristic;
import org.elasticsearch.search.aggregations.support.AggregationContext;
import org.elasticsearch.search.lookup.SourceLookup;

import java.io.IOException;
import java.util.Iterator;
import java.util.Map;
import java.util.function.LongConsumer;

public class SignificantTextAggregatorFactory extends AggregatorFactory {
    private static final int MEMORY_GROWTH_REPORTING_INTERVAL_BYTES = 5000;

    private final IncludeExclude includeExclude;
    private final MappedFieldType fieldType;
    private final String[] sourceFieldNames;
    private final QueryBuilder backgroundFilter;
    private final TermsAggregator.BucketCountThresholds bucketCountThresholds;
    private final SignificanceHeuristic significanceHeuristic;
    private final boolean filterDuplicateText;

    public SignificantTextAggregatorFactory(String name,
                                                IncludeExclude includeExclude,
                                                QueryBuilder backgroundFilter,
                                                TermsAggregator.BucketCountThresholds bucketCountThresholds,
                                                SignificanceHeuristic significanceHeuristic,
                                                AggregationContext context,
                                                AggregatorFactory parent,
                                                AggregatorFactories.Builder subFactoriesBuilder,
                                                String fieldName,
                                                String [] sourceFieldNames,
                                                boolean filterDuplicateText,
                                                Map<String, Object> metadata) throws IOException {
        super(name, context, parent, subFactoriesBuilder, metadata);

        this.fieldType = context.getFieldType(fieldName);
        if (fieldType != null) {
            if (supportsAgg(fieldType) == false) {
                throw new IllegalArgumentException("Field [" + fieldType.name() + "] has no analyzer, but SignificantText " +
                    "requires an analyzed field");
            }            
            String indexedFieldName = fieldType.name();
            this.sourceFieldNames = sourceFieldNames == null ? new String[] {indexedFieldName} : sourceFieldNames;
        } else {
            this.sourceFieldNames = new String[0];
        }

        this.includeExclude = includeExclude;
        this.backgroundFilter = backgroundFilter;
        this.filterDuplicateText = filterDuplicateText;
        this.bucketCountThresholds = bucketCountThresholds;
        this.significanceHeuristic = significanceHeuristic;
    }
    
    protected Aggregator createUnmapped(Aggregator parent, Map<String, Object> metadata) throws IOException {
        final InternalAggregation aggregation = new UnmappedSignificantTerms(name, bucketCountThresholds.getRequiredSize(),
                bucketCountThresholds.getMinDocCount(), metadata);
        return new NonCollectingAggregator(name, context, parent, factories, metadata) {
            @Override
            public InternalAggregation buildEmptyAggregation() {
                return aggregation;
            }
        };
    }    

    private static boolean supportsAgg(MappedFieldType ft) {
        return ft.getTextSearchInfo() != TextSearchInfo.NONE
            && ft.getTextSearchInfo() != TextSearchInfo.SIMPLE_MATCH_WITHOUT_TERMS;
    }

    @Override
    protected Aggregator createInternal(Aggregator parent, CardinalityUpperBound cardinality, Map<String, Object> metadata)
        throws IOException {
        
        if (fieldType == null) {
            return createUnmapped(parent, metadata);
        }
        
        BucketCountThresholds bucketCountThresholds = new BucketCountThresholds(this.bucketCountThresholds);
        if (bucketCountThresholds.getShardSize() == SignificantTextAggregationBuilder.DEFAULT_BUCKET_COUNT_THRESHOLDS.getShardSize()) {
            // The user has not made a shardSize selection.
            // Use default heuristic to avoid any wrong-ranking caused by
            // distributed counting but request double the usual amount.
            // We typically need more than the number of "top" terms requested
            // by other aggregations as the significance algorithm is in less
            // of a position to down-select at shard-level - some of the things
            // we want to find have only one occurrence on each shard and as
            // such are impossible to differentiate from non-significant terms
            // at that early stage.
            bucketCountThresholds.setShardSize(2 * BucketUtils.suggestShardSideQueueSize(bucketCountThresholds.getRequiredSize()));
        }

//        TODO - need to check with mapping that this is indeed a text field....

        IncludeExclude.StringFilter incExcFilter = includeExclude == null ? null:
            includeExclude.convertToStringFilter(DocValueFormat.RAW);

        MapStringTermsAggregator.CollectorSource collectorSource = new SignificantTextCollectorSource(
            context.lookup().source(),
            context.bigArrays(),
            fieldType,
            context.getIndexAnalyzer(f -> {
                throw new IllegalArgumentException("No analyzer configured for field " + f);
            }),
            sourceFieldNames,
            filterDuplicateText
        );
        SignificanceLookup lookup = new SignificanceLookup(context, fieldType, DocValueFormat.RAW, backgroundFilter);
        return new MapStringTermsAggregator(
            name,
            factories,
            collectorSource,
            a -> a.new SignificantTermsResults(lookup, significanceHeuristic, cardinality),
            null,
            DocValueFormat.RAW,
            bucketCountThresholds,
            incExcFilter,
            context,
            parent,
            SubAggCollectionMode.BREADTH_FIRST,
            false,
            cardinality,
            metadata
        );
    }

    private static class SignificantTextCollectorSource implements MapStringTermsAggregator.CollectorSource {
        private final SourceLookup sourceLookup;
        private final BigArrays bigArrays;
        private final MappedFieldType fieldType;
        private final Analyzer analyzer;
        private final String[] sourceFieldNames;
        private ObjectArray<DuplicateByteSequenceSpotter> dupSequenceSpotters;

        SignificantTextCollectorSource(
            SourceLookup sourceLookup,
            BigArrays bigArrays,
            MappedFieldType fieldType,
            Analyzer analyzer,
            String[] sourceFieldNames,
            boolean filterDuplicateText
        ) {
            this.sourceLookup = sourceLookup;
            this.bigArrays = bigArrays;
            this.fieldType = fieldType;
            this.analyzer = analyzer;
            this.sourceFieldNames = sourceFieldNames;
            dupSequenceSpotters = filterDuplicateText ? bigArrays.newObjectArray(1) : null;
        }

        @Override
        public boolean needsScores() {
            return false;
        }

        @Override
        public LeafBucketCollector getLeafCollector(
            StringFilter includeExclude,
            LeafReaderContext ctx,
            LeafBucketCollector sub,
            LongConsumer addRequestCircuitBreakerBytes,
            CollectConsumer consumer
        ) throws IOException {
            return new LeafBucketCollectorBase(sub, null) {
                private final BytesRefBuilder scratch = new BytesRefBuilder();

                @Override
                public void collect(int doc, long owningBucketOrd) throws IOException {
                    if (dupSequenceSpotters == null) {
                        collectFromSource(doc, owningBucketOrd, null);
                        return;
                    }
                    dupSequenceSpotters = bigArrays.grow(dupSequenceSpotters, owningBucketOrd + 1);
                    DuplicateByteSequenceSpotter spotter = dupSequenceSpotters.get(owningBucketOrd);
                    if (spotter == null) {
                        spotter = new DuplicateByteSequenceSpotter();
                        dupSequenceSpotters.set(owningBucketOrd, spotter);
                    }
                    collectFromSource(doc, owningBucketOrd, spotter);
                    spotter.startNewSequence();
                }

                private void collectFromSource(int doc, long owningBucketOrd, DuplicateByteSequenceSpotter spotter) throws IOException {
                    sourceLookup.setSegmentAndDocument(ctx, doc);
                    BytesRefHash inDocTerms = new BytesRefHash(256, bigArrays);

                    try {
                        for (String sourceField : sourceFieldNames) {
                            Iterator<String> itr = sourceLookup.extractRawValues(sourceField).stream()
                                .map(obj -> {
                                    if (obj == null) {
                                        return null;
                                    }
                                    if (obj instanceof BytesRef) {
                                        return fieldType.valueForDisplay(obj).toString();
                                    }
                                    return obj.toString();
                                })
                                .iterator();
                            while (itr.hasNext()) {
                                TokenStream ts = analyzer.tokenStream(fieldType.name(), itr.next());
                                processTokenStream(doc, owningBucketOrd, ts, inDocTerms, spotter);
                            }
                        }
                    } finally {
                        Releasables.close(inDocTerms);
                    }
                }

                private void processTokenStream(
                    int doc,
                    long owningBucketOrd,
                    TokenStream ts,
                    BytesRefHash inDocTerms,
                    DuplicateByteSequenceSpotter spotter
                ) throws IOException {
                    long lastTrieSize = 0;
                    if (spotter != null) {
                        lastTrieSize = spotter.getEstimatedSizeInBytes();
                        ts = new DeDuplicatingTokenFilter(ts, spotter);
                    }
                    CharTermAttribute termAtt = ts.addAttribute(CharTermAttribute.class);
                    ts.reset();
                    try {
                        while (ts.incrementToken()) {
                            if (spotter != null) {
                                long newTrieSize = spotter.getEstimatedSizeInBytes();
                                long growth = newTrieSize - lastTrieSize;
                                // Only update the circuitbreaker after
                                if (growth > MEMORY_GROWTH_REPORTING_INTERVAL_BYTES) {
                                    addRequestCircuitBreakerBytes.accept(growth);
                                    lastTrieSize = newTrieSize;
                                }
                            }

                            scratch.clear();
                            scratch.copyChars(termAtt);
                            BytesRef bytes = scratch.get();
                            if (includeExclude != null && false == includeExclude.accept(bytes)) {
                                continue;
                            }
                            if (inDocTerms.add(bytes) < 0) {
                                continue;
                            }
                            consumer.accept(sub, doc, owningBucketOrd, bytes);
                        }
                    } finally {
                        ts.close();
                    }
                    if (spotter != null) {
                        long growth = spotter.getEstimatedSizeInBytes() - lastTrieSize;
                        if (growth > 0) {
                            addRequestCircuitBreakerBytes.accept(growth);
                        }
                    }
                }
            };
        }

        @Override
        public void close() {
            Releasables.close(dupSequenceSpotters);
        }
    }
}
