/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.fetch;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.TotalHits;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.index.fieldvisitor.LeafStoredFieldLoader;
import org.elasticsearch.index.fieldvisitor.StoredFieldLoader;
import org.elasticsearch.index.mapper.SourceLoader;
import org.elasticsearch.search.LeafNestedDocuments;
import org.elasticsearch.search.NestedDocuments;
import org.elasticsearch.search.SearchContextSourcePrinter;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.search.SearchShardTarget;
import org.elasticsearch.search.fetch.FetchSubPhase.HitContext;
import org.elasticsearch.search.fetch.subphase.InnerHitsContext;
import org.elasticsearch.search.fetch.subphase.InnerHitsPhase;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.search.lookup.Source;
import org.elasticsearch.search.lookup.SourceLookup;
import org.elasticsearch.search.profile.ProfileResult;
import org.elasticsearch.search.profile.Profilers;
import org.elasticsearch.tasks.TaskCancelledException;
import org.elasticsearch.xcontent.XContentType;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

/**
 * Fetch phase of a search request, used to fetch the actual top matching documents to be returned to the client, identified
 * after reducing all of the matches returned by the query phase
 */
public class FetchPhase {
    private static final Logger LOGGER = LogManager.getLogger(FetchPhase.class);

    private final FetchSubPhase[] fetchSubPhases;

    public FetchPhase(List<FetchSubPhase> fetchSubPhases) {
        this.fetchSubPhases = fetchSubPhases.toArray(new FetchSubPhase[fetchSubPhases.size() + 1]);
        this.fetchSubPhases[fetchSubPhases.size()] = new InnerHitsPhase(this);
    }

    public void execute(SearchContext context) {
        if (LOGGER.isTraceEnabled()) {
            LOGGER.trace("{}", new SearchContextSourcePrinter(context));
        }

        if (context.isCancelled()) {
            throw new TaskCancelledException("cancelled");
        }

        if (context.docIdsToLoad() == null || context.docIdsToLoad().length == 0) {
            // no individual hits to process, so we shortcut
            SearchHits hits = new SearchHits(new SearchHit[0], context.queryResult().getTotalHits(), context.queryResult().getMaxScore());
            context.fetchResult().shardResult(hits, null);
            return;
        }

        Profiler profiler = context.getProfilers() == null ? Profiler.NOOP : Profilers.startProfilingFetchPhase();
        SearchHits hits = null;
        try {
            hits = buildSearchHits(context, profiler);
        } finally {
            // Always finish profiling
            ProfileResult profileResult = profiler.finish();
            // Only set the shardResults if building search hits was successful
            if (hits != null) {
                context.fetchResult().shardResult(hits, profileResult);
            }
        }
    }

    private SearchHits buildSearchHits(SearchContext context, Profiler profiler) {

        FetchContext fetchContext = new FetchContext(context);
        SourceLoader sourceLoader = context.newSourceLoader();

        List<FetchSubPhaseProcessor> processors = getProcessors(context.shardTarget(), fetchContext, profiler);

        StoredFieldsSpec storedFieldsSpec = StoredFieldsSpec.NO_REQUIREMENTS;
        for (FetchSubPhaseProcessor proc : processors) {
            storedFieldsSpec = storedFieldsSpec.merge(proc.storedFieldsSpec());
        }
        storedFieldsSpec = storedFieldsSpec.merge(new StoredFieldsSpec(false, false, sourceLoader.requiredStoredFields()));

        StoredFieldLoader storedFieldLoader = profiler.storedFields(buildStoredFieldsLoader(storedFieldsSpec));
        boolean requiresSource = storedFieldsSpec.requiresSource();

        NestedDocuments nestedDocuments = context.getSearchExecutionContext().getNestedDocuments();

        FetchPhaseDocsIterator docsIterator = new FetchPhaseDocsIterator() {

            LeafReaderContext ctx;
            LeafNestedDocuments leafNestedDocuments;
            LeafStoredFieldLoader leafStoredFieldLoader;
            SourceLoader.Leaf leafSourceLoader;

            @Override
            protected void setNextReader(LeafReaderContext ctx, int[] docsInLeaf) throws IOException {
                profiler.startNextReader();
                this.ctx = ctx;
                this.leafNestedDocuments = nestedDocuments.getLeafNestedDocuments(ctx);
                this.leafStoredFieldLoader = storedFieldLoader.getLoader(ctx, docsInLeaf);
                this.leafSourceLoader = sourceLoader.leaf(ctx.reader(), docsInLeaf);
                for (FetchSubPhaseProcessor processor : processors) {
                    processor.setNextReader(ctx);
                }
                profiler.stopNextReader();
            }

            @Override
            protected SearchHit nextDoc(int doc) throws IOException {
                if (context.isCancelled()) {
                    throw new TaskCancelledException("cancelled");
                }
                HitContext hit = prepareHitContext(
                    context,
                    requiresSource,
                    profiler,
                    leafNestedDocuments,
                    leafStoredFieldLoader,
                    doc,
                    ctx,
                    leafSourceLoader
                );
                for (FetchSubPhaseProcessor processor : processors) {
                    processor.process(hit);
                }
                return hit.hit();
            }
        };

        SearchHit[] hits = docsIterator.iterate(context.shardTarget(), context.searcher().getIndexReader(), context.docIdsToLoad());

        if (context.isCancelled()) {
            throw new TaskCancelledException("cancelled");
        }

        TotalHits totalHits = context.queryResult().getTotalHits();
        return new SearchHits(hits, totalHits, context.queryResult().getMaxScore());
    }

    private static StoredFieldLoader buildStoredFieldsLoader(StoredFieldsSpec spec) {
        if (spec.noRequirements()) {
            return StoredFieldLoader.empty();
        }
        return StoredFieldLoader.create(spec.requiresSource(), spec.requiredStoredFields());
    }

    List<FetchSubPhaseProcessor> getProcessors(SearchShardTarget target, FetchContext context, Profiler profiler) {
        try {
            List<FetchSubPhaseProcessor> processors = new ArrayList<>();
            for (FetchSubPhase fsp : fetchSubPhases) {
                FetchSubPhaseProcessor processor = fsp.getProcessor(context);
                if (processor != null) {
                    processors.add(profiler.profile(fsp.getClass().getSimpleName(), "", processor));
                }
            }
            return processors;
        } catch (Exception e) {
            throw new FetchPhaseExecutionException(target, "Error building fetch sub-phases", e);
        }
    }

    private static HitContext prepareHitContext(
        SearchContext context,
        boolean requiresSource,
        Profiler profiler,
        LeafNestedDocuments nestedDocuments,
        LeafStoredFieldLoader leafStoredFieldLoader,
        int docId,
        LeafReaderContext subReaderContext,
        SourceLoader.Leaf sourceLoader
    ) throws IOException {
        if (nestedDocuments.advance(docId - subReaderContext.docBase) == null) {
            return prepareNonNestedHitContext(
                context,
                requiresSource,
                profiler,
                leafStoredFieldLoader,
                docId,
                subReaderContext,
                sourceLoader
            );
        } else {
            return prepareNestedHitContext(
                context,
                requiresSource,
                profiler,
                docId,
                nestedDocuments,
                subReaderContext,
                leafStoredFieldLoader
            );
        }
    }

    /**
     * Resets the provided {@link HitContext} with information on the current
     * document. This includes the following:
     *   - Adding an initial {@link SearchHit} instance.
     *   - Loading the document source and setting it on {@link HitContext#source()}. This
     *     allows fetch subphases that use the hit context to access the preloaded source.
     */
    private static HitContext prepareNonNestedHitContext(
        SearchContext context,
        boolean requiresSource,
        Profiler profiler,
        LeafStoredFieldLoader leafStoredFieldLoader,
        int docId,
        LeafReaderContext subReaderContext,
        SourceLoader.Leaf sourceLoader
    ) throws IOException {
        int subDocId = docId - subReaderContext.docBase;

        leafStoredFieldLoader.advanceTo(subDocId);

        if (leafStoredFieldLoader.id() == null) {
            SearchHit hit = new SearchHit(docId, null);
            Source source = Source.lazy(lazyStoredSourceLoader(profiler, subReaderContext, subDocId));
            return new HitContext(hit, subReaderContext, subDocId, Map.of(), source);
        } else {
            SearchHit hit = new SearchHit(docId, leafStoredFieldLoader.id());
            Source source;
            if (requiresSource) {
                try {
                    profiler.startLoadingSource();
                    source = Source.fromBytes(sourceLoader.source(leafStoredFieldLoader, subDocId));
                    SourceLookup scriptSourceLookup = context.getSearchExecutionContext().lookup().source();
                    scriptSourceLookup.setSegmentAndDocument(subReaderContext, subDocId);
                    scriptSourceLookup.setSourceProvider(new SourceLookup.BytesSourceProvider(source.internalSourceRef()));
                } finally {
                    profiler.stopLoadingSource();
                }
            } else {
                source = Source.lazy(lazyStoredSourceLoader(profiler, subReaderContext, subDocId));
            }
            return new HitContext(hit, subReaderContext, subDocId, leafStoredFieldLoader.storedFields(), source);
        }
    }

    private static Supplier<Source> lazyStoredSourceLoader(Profiler profiler, LeafReaderContext ctx, int doc) {
        return () -> {
            StoredFieldLoader rootLoader = profiler.storedFields(StoredFieldLoader.create(true, Collections.emptySet()));
            LeafStoredFieldLoader leafRootLoader = rootLoader.getLoader(ctx, null);
            try {
                leafRootLoader.advanceTo(doc);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
            return Source.fromBytes(leafRootLoader.source());
        };
    }

    /**
     * Resets the provided {@link HitContext} with information on the current
     * nested document. This includes the following:
     *   - Adding an initial {@link SearchHit} instance.
     *   - Loading the document source, filtering it based on the nested document ID, then
     *     setting it on {@link HitContext#source()}. This allows fetch subphases that
     *     use the hit context to access the preloaded source.
     */
    @SuppressWarnings("unchecked")
    private static HitContext prepareNestedHitContext(
        SearchContext context,
        boolean requiresSource,
        Profiler profiler,
        int topDocId,
        LeafNestedDocuments nestedInfo,
        LeafReaderContext subReaderContext,
        LeafStoredFieldLoader childFieldLoader
    ) throws IOException {

        String rootId;
        Map<String, Object> rootSourceAsMap = null;
        XContentType rootSourceContentType = null;

        if (context instanceof InnerHitsContext.InnerHitSubContext innerHitsContext) {
            rootId = innerHitsContext.getRootId();

            if (requiresSource) {
                Source rootLookup = innerHitsContext.getRootLookup();
                rootSourceAsMap = rootLookup.source();
                rootSourceContentType = rootLookup.sourceContentType();
            }
        } else {
            StoredFieldLoader rootLoader = profiler.storedFields(StoredFieldLoader.create(requiresSource, Collections.emptySet()));
            LeafStoredFieldLoader leafRootLoader = rootLoader.getLoader(subReaderContext, null);
            leafRootLoader.advanceTo(nestedInfo.rootDoc());
            rootId = leafRootLoader.id();

            if (requiresSource) {
                if (leafRootLoader.source() != null) {
                    Tuple<XContentType, Map<String, Object>> tuple = XContentHelper.convertToMap(leafRootLoader.source(), false);
                    rootSourceAsMap = tuple.v2();
                    rootSourceContentType = tuple.v1();
                } else {
                    rootSourceAsMap = Collections.emptyMap();
                }
            }
        }

        childFieldLoader.advanceTo(nestedInfo.doc());

        SearchHit.NestedIdentity nestedIdentity = nestedInfo.nestedIdentity();

        SearchHit hit = new SearchHit(topDocId, rootId, nestedIdentity);

        if (rootSourceAsMap != null && rootSourceAsMap.isEmpty() == false) {
            // Isolate the nested json array object that matches with nested hit and wrap it back into the same json
            // structure with the nested json array object being the actual content. The latter is important, so that
            // features like source filtering and highlighting work consistent regardless of whether the field points
            // to a json object array for consistency reasons on how we refer to fields
            Map<String, Object> nestedSourceAsMap = new HashMap<>();
            Map<String, Object> current = nestedSourceAsMap;
            for (SearchHit.NestedIdentity nested = nestedIdentity; nested != null; nested = nested.getChild()) {
                String nestedPath = nested.getField().string();
                current.put(nestedPath, new HashMap<>());
                List<Map<?, ?>> nestedParsedSource = XContentMapValues.extractNestedSources(nestedPath, rootSourceAsMap);
                if (nestedParsedSource == null) {
                    throw new IllegalStateException("Couldn't find nested source for path " + nestedPath);
                }
                rootSourceAsMap = (Map<String, Object>) nestedParsedSource.get(nested.getOffset());
                if (nested.getChild() == null) {
                    current.put(nestedPath, rootSourceAsMap);
                } else {
                    Map<String, Object> next = new HashMap<>();
                    current.put(nestedPath, next);
                    current = next;
                }
            }
            return new HitContext(
                hit,
                subReaderContext,
                nestedInfo.doc(),
                childFieldLoader.storedFields(),
                Source.fromMap(nestedSourceAsMap, rootSourceContentType)
            );
        }
        return new HitContext(hit, subReaderContext, nestedInfo.doc(), childFieldLoader.storedFields(), Source.empty(null));
    }

    interface Profiler {
        ProfileResult finish();

        FetchSubPhaseProcessor profile(String type, String description, FetchSubPhaseProcessor processor);

        StoredFieldLoader storedFields(StoredFieldLoader storedFieldLoader);

        void startLoadingSource();

        void stopLoadingSource();

        void startNextReader();

        void stopNextReader();

        Profiler NOOP = new Profiler() {
            @Override
            public ProfileResult finish() {
                return null;
            }

            @Override
            public StoredFieldLoader storedFields(StoredFieldLoader storedFieldLoader) {
                return storedFieldLoader;
            }

            @Override
            public FetchSubPhaseProcessor profile(String type, String description, FetchSubPhaseProcessor processor) {
                return processor;
            }

            @Override
            public void startLoadingSource() {}

            @Override
            public void stopLoadingSource() {}

            @Override
            public void startNextReader() {}

            @Override
            public void stopNextReader() {}

            @Override
            public String toString() {
                return "noop";
            }
        };
    }
}
