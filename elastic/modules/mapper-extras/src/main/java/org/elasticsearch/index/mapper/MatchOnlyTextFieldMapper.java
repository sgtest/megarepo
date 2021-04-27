/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.document.Field;
import org.apache.lucene.document.FieldType;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.index.Term;
import org.apache.lucene.queries.intervals.Intervals;
import org.apache.lucene.queries.intervals.IntervalsSource;
import org.apache.lucene.search.ConstantScoreQuery;
import org.apache.lucene.search.FuzzyQuery;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.MultiTermQuery;
import org.apache.lucene.search.PrefixQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.Version;
import org.elasticsearch.common.CheckedIntFunction;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.mapper.TextFieldMapper.TextFieldType;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.query.SourceConfirmedTextQuery;
import org.elasticsearch.index.query.SourceIntervalsSource;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.lookup.SourceLookup;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.Function;
import java.util.function.Supplier;

/**
 * A {@link FieldMapper} for full-text fields that only indexes
 * {@link IndexOptions#DOCS} and runs positional queries by looking at the
 * _source.
 */
public class MatchOnlyTextFieldMapper extends FieldMapper {

    public static final String CONTENT_TYPE = "match_only_text";

    public static class Defaults {
        public static final FieldType FIELD_TYPE = new FieldType();

        static {
            FIELD_TYPE.setTokenized(true);
            FIELD_TYPE.setStored(false);
            FIELD_TYPE.setStoreTermVectors(false);
            FIELD_TYPE.setOmitNorms(true);
            FIELD_TYPE.setIndexOptions(IndexOptions.DOCS);
            FIELD_TYPE.freeze();
        }

    }

    private static Builder builder(FieldMapper in) {
        return ((MatchOnlyTextFieldMapper) in).builder;
    }

    public static class Builder extends FieldMapper.Builder {

        private final Version indexCreatedVersion;

        private final Parameter<Map<String, String>> meta = Parameter.metaParam();

        private final TextParams.Analyzers analyzers;

        public Builder(String name, IndexAnalyzers indexAnalyzers) {
            this(name, Version.CURRENT, indexAnalyzers);
        }

        public Builder(String name, Version indexCreatedVersion, IndexAnalyzers indexAnalyzers) {
            super(name);
            this.indexCreatedVersion = indexCreatedVersion;
            this.analyzers = new TextParams.Analyzers(indexAnalyzers, m -> builder(m).analyzers);
        }

        public Builder addMultiField(FieldMapper.Builder builder) {
            this.multiFieldsBuilder.add(builder);
            return this;
        }

        @Override
        protected List<Parameter<?>> getParameters() {
            return Arrays.asList(meta);
        }

        private MatchOnlyTextFieldType buildFieldType(FieldType fieldType, ContentPath contentPath) {
            NamedAnalyzer searchAnalyzer = analyzers.getSearchAnalyzer();
            NamedAnalyzer searchQuoteAnalyzer = analyzers.getSearchQuoteAnalyzer();
            NamedAnalyzer indexAnalyzer = analyzers.getIndexAnalyzer();
            TextSearchInfo tsi = new TextSearchInfo(fieldType, null, searchAnalyzer, searchQuoteAnalyzer);
            MatchOnlyTextFieldType ft = new MatchOnlyTextFieldType(buildFullName(contentPath), tsi, indexAnalyzer, meta.getValue());
            return ft;
        }

        @Override
        public MatchOnlyTextFieldMapper build(ContentPath contentPath) {
            MatchOnlyTextFieldType tft = buildFieldType(Defaults.FIELD_TYPE, contentPath);
            MultiFields multiFields = multiFieldsBuilder.build(this, contentPath);
            return new MatchOnlyTextFieldMapper(
                name,
                Defaults.FIELD_TYPE,
                tft,
                analyzers.getIndexAnalyzer(),
                multiFields,
                copyTo.build(),
                this
            );
        }
    }

    public static final TypeParser PARSER = new TypeParser((n, c) -> new Builder(n, c.indexVersionCreated(), c.getIndexAnalyzers()));

    public static class MatchOnlyTextFieldType extends StringFieldType {

        private final Analyzer indexAnalyzer;
        private final TextFieldType textFieldType;

        public MatchOnlyTextFieldType(String name, TextSearchInfo tsi, Analyzer indexAnalyzer, Map<String, String> meta) {
            super(name, true, false, false, tsi, meta);
            this.indexAnalyzer = Objects.requireNonNull(indexAnalyzer);
            this.textFieldType = new TextFieldType(name);
        }

        public MatchOnlyTextFieldType(String name, boolean stored, Map<String, String> meta) {
            super(
                name,
                true,
                stored,
                false,
                new TextSearchInfo(Defaults.FIELD_TYPE, null, Lucene.STANDARD_ANALYZER, Lucene.STANDARD_ANALYZER),
                meta
            );
            this.indexAnalyzer = Lucene.STANDARD_ANALYZER;
            this.textFieldType = new TextFieldType(name);
        }

        public MatchOnlyTextFieldType(String name) {
            this(
                name,
                new TextSearchInfo(Defaults.FIELD_TYPE, null, Lucene.STANDARD_ANALYZER, Lucene.STANDARD_ANALYZER),
                Lucene.STANDARD_ANALYZER,
                Collections.emptyMap()
            );
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public String familyTypeName() {
            return TextFieldMapper.CONTENT_TYPE;
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            return SourceValueFetcher.toString(name(), context, format);
        }

        private Function<LeafReaderContext,CheckedIntFunction<List<Object>, IOException>> getValueFetcherProvider(
                SearchExecutionContext searchExecutionContext) {
            if (searchExecutionContext.isSourceEnabled() == false) {
                throw new IllegalArgumentException(
                    "Field [" + name() + "] of type [" + CONTENT_TYPE + "] cannot run positional queries since [_source] is disabled."
                );
            }
            SourceLookup sourceLookup = searchExecutionContext.lookup().source();
            ValueFetcher valueFetcher = valueFetcher(searchExecutionContext, null);
            return context -> {
                valueFetcher.setNextReader(context);
                return docID -> {
                    try {
                        sourceLookup.setSegmentAndDocument(context, docID);
                        return valueFetcher.fetchValues(sourceLookup);
                    } catch (IOException e) {
                        throw new UncheckedIOException(e);
                    }
                };
            };
        }

        private Query toQuery(Query query, SearchExecutionContext searchExecutionContext) {
            return new ConstantScoreQuery(
                new SourceConfirmedTextQuery(query, getValueFetcherProvider(searchExecutionContext), indexAnalyzer));
        }

        private IntervalsSource toIntervalsSource(
                IntervalsSource source,
                Query approximation,
                SearchExecutionContext searchExecutionContext) {
            return new SourceIntervalsSource(source, approximation, getValueFetcherProvider(searchExecutionContext), indexAnalyzer);
        }

        @Override
        public Query termQuery(Object value, SearchExecutionContext context) {
            // Disable scoring
            return new ConstantScoreQuery(super.termQuery(value, context));
        }

        @Override
        public Query fuzzyQuery(
            Object value,
            Fuzziness fuzziness,
            int prefixLength,
            int maxExpansions,
            boolean transpositions,
            SearchExecutionContext context
        ) {
            // Disable scoring
            return new ConstantScoreQuery(super.fuzzyQuery(value, fuzziness, prefixLength, maxExpansions, transpositions, context));
        }

        @Override
        public IntervalsSource termIntervals(BytesRef term, SearchExecutionContext context) {
            return toIntervalsSource(Intervals.term(term), new TermQuery(new Term(name(), term)), context);
        }

        @Override
        public IntervalsSource prefixIntervals(BytesRef term, SearchExecutionContext context) {
            return toIntervalsSource(Intervals.prefix(term), new PrefixQuery(new Term(name(), term)), context);
        }

        @Override
        public IntervalsSource fuzzyIntervals(String term, int maxDistance, int prefixLength,
                boolean transpositions, SearchExecutionContext context) {
            FuzzyQuery fuzzyQuery = new FuzzyQuery(new Term(name(), term),
                maxDistance, prefixLength, 128, transpositions);
            fuzzyQuery.setRewriteMethod(MultiTermQuery.CONSTANT_SCORE_REWRITE);
            IntervalsSource fuzzyIntervals = Intervals.multiterm(fuzzyQuery.getAutomata(), term);
            return toIntervalsSource(fuzzyIntervals, fuzzyQuery, context);
        }

        @Override
        public IntervalsSource wildcardIntervals(BytesRef pattern, SearchExecutionContext context) {
            return toIntervalsSource(
                Intervals.wildcard(pattern),
                new MatchAllDocsQuery(), // wildcard queries can be expensive, what should the approximation be?
                context);
        }

        @Override
        public Query phraseQuery(TokenStream stream, int slop, boolean enablePosIncrements, SearchExecutionContext queryShardContext)
            throws IOException {
            final Query query = textFieldType.phraseQuery(stream, slop, enablePosIncrements, queryShardContext);
            return toQuery(query, queryShardContext);
        }

        @Override
        public Query multiPhraseQuery(
            TokenStream stream,
            int slop,
            boolean enablePositionIncrements,
            SearchExecutionContext queryShardContext
        ) throws IOException {
            final Query query = textFieldType.multiPhraseQuery(stream, slop, enablePositionIncrements, queryShardContext);
            return toQuery(query, queryShardContext);
        }

        @Override
        public Query phrasePrefixQuery(TokenStream stream, int slop, int maxExpansions, SearchExecutionContext queryShardContext)
            throws IOException {
            final Query query = textFieldType.phrasePrefixQuery(stream, slop, maxExpansions, queryShardContext);
            return toQuery(query, queryShardContext);
        }

        @Override
        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
            throw new IllegalArgumentException(CONTENT_TYPE + " fields do not support sorting and aggregations");
        }

    }

    private final Builder builder;
    private final FieldType fieldType;

    private MatchOnlyTextFieldMapper(
        String simpleName,
        FieldType fieldType,
        MatchOnlyTextFieldType mappedFieldType,
        NamedAnalyzer indexAnalyzer,
        MultiFields multiFields,
        CopyTo copyTo,
        Builder builder
    ) {
        super(simpleName, mappedFieldType, indexAnalyzer, multiFields, copyTo);
        assert mappedFieldType.getTextSearchInfo().isTokenized();
        assert mappedFieldType.hasDocValues() == false;
        this.fieldType = fieldType;
        this.builder = builder;
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return new Builder(simpleName(), builder.indexCreatedVersion, builder.analyzers.indexAnalyzers).init(this);
    }

    @Override
    protected void parseCreateField(ParseContext context) throws IOException {
        final String value;
        if (context.externalValueSet()) {
            value = context.externalValue().toString();
        } else {
            value = context.parser().textOrNull();
        }

        if (value == null) {
            return;
        }

        Field field = new Field(fieldType().name(), value, fieldType);
        context.doc().add(field);
        createFieldNamesField(context);
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    public MatchOnlyTextFieldType fieldType() {
        return (MatchOnlyTextFieldType) super.fieldType();
    }

}
