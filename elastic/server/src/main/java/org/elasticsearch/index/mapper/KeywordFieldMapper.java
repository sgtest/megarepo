/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.analysis.tokenattributes.CharTermAttribute;
import org.apache.lucene.document.Field;
import org.apache.lucene.document.FieldType;
import org.apache.lucene.document.SortedSetDocValuesField;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.plain.SortedSetOrdinalsIndexFieldData;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.similarity.SimilarityProvider;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptCompiler;
import org.elasticsearch.script.StringFieldScript;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.lookup.FieldValues;
import org.elasticsearch.search.lookup.SearchLookup;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.Supplier;

/**
 * A field mapper for keywords. This mapper accepts strings and indexes them as-is.
 */
public final class KeywordFieldMapper extends FieldMapper {

    public static final String CONTENT_TYPE = "keyword";

    public static class Defaults {
        public static final FieldType FIELD_TYPE = new FieldType();

        static {
            FIELD_TYPE.setTokenized(false);
            FIELD_TYPE.setOmitNorms(true);
            FIELD_TYPE.setIndexOptions(IndexOptions.DOCS);
            FIELD_TYPE.freeze();
        }
    }

    public static class KeywordField extends Field {

        public KeywordField(String field, BytesRef term, FieldType ft) {
            super(field, term, ft);
        }

    }

    private static KeywordFieldMapper toType(FieldMapper in) {
        return (KeywordFieldMapper) in;
    }

    public static class Builder extends FieldMapper.Builder {

        private final Parameter<Boolean> indexed = Parameter.indexParam(m -> toType(m).indexed, true);
        private final Parameter<Boolean> hasDocValues = Parameter.docValuesParam(m -> toType(m).hasDocValues, true);
        private final Parameter<Boolean> stored = Parameter.storeParam(m -> toType(m).fieldType.stored(), false);

        private final Parameter<String> nullValue
            = Parameter.stringParam("null_value", false, m -> toType(m).nullValue, null).acceptsNull();

        private final Parameter<Boolean> eagerGlobalOrdinals
            = Parameter.boolParam("eager_global_ordinals", true, m -> toType(m).eagerGlobalOrdinals, false);
        private final Parameter<Integer> ignoreAbove
            = Parameter.intParam("ignore_above", true, m -> toType(m).ignoreAbove, Integer.MAX_VALUE);

        private final Parameter<String> indexOptions
            = Parameter.restrictedStringParam("index_options", false, m -> toType(m).indexOptions, "docs", "freqs");
        private final Parameter<Boolean> hasNorms = TextParams.norms(false, m -> toType(m).fieldType.omitNorms() == false);
        private final Parameter<SimilarityProvider> similarity = TextParams.similarity(m -> toType(m).similarity);

        private final Parameter<String> normalizer
            = Parameter.stringParam("normalizer", false, m -> toType(m).normalizerName, null).acceptsNull();

        private final Parameter<Boolean> splitQueriesOnWhitespace
            = Parameter.boolParam("split_queries_on_whitespace", true, m -> toType(m).splitQueriesOnWhitespace, false);

        private final Parameter<Map<String, String>> meta = Parameter.metaParam();

        private final Parameter<Script> script = Parameter.scriptParam(m -> toType(m).script);
        private final Parameter<String> onScriptError = Parameter.onScriptErrorParam(m -> toType(m).onScriptError, script);

        private final IndexAnalyzers indexAnalyzers;
        private final ScriptCompiler scriptCompiler;

        public Builder(String name, IndexAnalyzers indexAnalyzers, ScriptCompiler scriptCompiler) {
            super(name);
            this.indexAnalyzers = indexAnalyzers;
            this.scriptCompiler = Objects.requireNonNull(scriptCompiler);
            this.script.precludesParameters(nullValue);
            addScriptValidation(script, indexed, hasDocValues);
        }

        public Builder(String name) {
            this(name, null, ScriptCompiler.NONE);
        }

        public Builder ignoreAbove(int ignoreAbove) {
            this.ignoreAbove.setValue(ignoreAbove);
            return this;
        }

        Builder normalizer(String normalizerName) {
            this.normalizer.setValue(normalizerName);
            return this;
        }

        Builder nullValue(String nullValue) {
            this.nullValue.setValue(nullValue);
            return this;
        }

        public Builder docValues(boolean hasDocValues) {
            this.hasDocValues.setValue(hasDocValues);
            return this;
        }

        private FieldValues<String> scriptValues() {
            if (script.get() == null) {
                return null;
            }
            StringFieldScript.Factory scriptFactory = scriptCompiler.compile(script.get(), StringFieldScript.CONTEXT);
            return scriptFactory == null ? null : (lookup, ctx, doc, consumer) -> scriptFactory
                .newFactory(name, script.get().getParams(), lookup)
                .newInstance(ctx)
                .runForDoc(doc, consumer);
        }

        @Override
        protected List<Parameter<?>> getParameters() {
            return List.of(indexed, hasDocValues, stored, nullValue, eagerGlobalOrdinals, ignoreAbove,
                indexOptions, hasNorms, similarity, normalizer, splitQueriesOnWhitespace,
                script, onScriptError, meta);
        }

        private KeywordFieldType buildFieldType(ContentPath contentPath, FieldType fieldType) {
            NamedAnalyzer normalizer = Lucene.KEYWORD_ANALYZER;
            NamedAnalyzer searchAnalyzer = Lucene.KEYWORD_ANALYZER;
            NamedAnalyzer quoteAnalyzer = Lucene.KEYWORD_ANALYZER;
            String normalizerName = this.normalizer.getValue();
            if (normalizerName != null) {
                assert indexAnalyzers != null;
                normalizer = indexAnalyzers.getNormalizer(normalizerName);
                if (normalizer == null) {
                    throw new MapperParsingException("normalizer [" + normalizerName + "] not found for field [" + name + "]");
                }
                searchAnalyzer = quoteAnalyzer = normalizer;
                if (splitQueriesOnWhitespace.getValue()) {
                    searchAnalyzer = indexAnalyzers.getWhitespaceNormalizer(normalizerName);
                }
            }
            else if (splitQueriesOnWhitespace.getValue()) {
                searchAnalyzer = Lucene.WHITESPACE_ANALYZER;
            }
            return new KeywordFieldType(buildFullName(contentPath), fieldType, normalizer, searchAnalyzer, quoteAnalyzer, this);
        }

        @Override
        public KeywordFieldMapper build(ContentPath contentPath) {
            FieldType fieldtype = new FieldType(Defaults.FIELD_TYPE);
            fieldtype.setOmitNorms(this.hasNorms.getValue() == false);
            fieldtype.setIndexOptions(TextParams.toIndexOptions(this.indexed.getValue(), this.indexOptions.getValue()));
            fieldtype.setStored(this.stored.getValue());
            return new KeywordFieldMapper(name, fieldtype, buildFieldType(contentPath, fieldtype),
                    multiFieldsBuilder.build(this, contentPath), copyTo.build(), this);
        }
    }

    public static final TypeParser PARSER
        = new TypeParser((n, c) -> new Builder(n, c.getIndexAnalyzers(), c.scriptCompiler()));

    public static final class KeywordFieldType extends StringFieldType {

        private final int ignoreAbove;
        private final String nullValue;
        private final NamedAnalyzer normalizer;
        private final boolean eagerGlobalOrdinals;
        private final FieldValues<String> scriptValues;

        public KeywordFieldType(String name, FieldType fieldType,
                                NamedAnalyzer normalizer, NamedAnalyzer searchAnalyzer, NamedAnalyzer quoteAnalyzer,
                                Builder builder) {
            super(name,
                fieldType.indexOptions() != IndexOptions.NONE,
                fieldType.stored(),
                builder.hasDocValues.getValue(),
                new TextSearchInfo(fieldType, builder.similarity.getValue(), searchAnalyzer, quoteAnalyzer),
                builder.meta.getValue());
            this.eagerGlobalOrdinals = builder.eagerGlobalOrdinals.getValue();
            this.normalizer = normalizer;
            this.ignoreAbove = builder.ignoreAbove.getValue();
            this.nullValue = builder.nullValue.getValue();
            this.scriptValues = builder.scriptValues();
        }

        public KeywordFieldType(String name, boolean isSearchable, boolean hasDocValues, Map<String, String> meta) {
            super(name, isSearchable, false, hasDocValues, TextSearchInfo.SIMPLE_MATCH_ONLY, meta);
            this.normalizer = Lucene.KEYWORD_ANALYZER;
            this.ignoreAbove = Integer.MAX_VALUE;
            this.nullValue = null;
            this.eagerGlobalOrdinals = false;
            this.scriptValues = null;
        }

        public KeywordFieldType(String name) {
            this(name, true, true, Collections.emptyMap());
        }

        public KeywordFieldType(String name, FieldType fieldType) {
            super(name, fieldType.indexOptions() != IndexOptions.NONE,
                false, false,
                new TextSearchInfo(fieldType, null, Lucene.KEYWORD_ANALYZER, Lucene.KEYWORD_ANALYZER),
                Collections.emptyMap());
            this.normalizer = Lucene.KEYWORD_ANALYZER;
            this.ignoreAbove = Integer.MAX_VALUE;
            this.nullValue = null;
            this.eagerGlobalOrdinals = false;
            this.scriptValues = null;
        }

        public KeywordFieldType(String name, NamedAnalyzer analyzer) {
            super(name, true, false, true, new TextSearchInfo(Defaults.FIELD_TYPE, null, analyzer, analyzer), Collections.emptyMap());
            this.normalizer = Lucene.KEYWORD_ANALYZER;
            this.ignoreAbove = Integer.MAX_VALUE;
            this.nullValue = null;
            this.eagerGlobalOrdinals = false;
            this.scriptValues = null;
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public boolean eagerGlobalOrdinals() {
            return eagerGlobalOrdinals;
        }

        NamedAnalyzer normalizer() {
            return normalizer;
        }

        @Override
        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
            failIfNoDocValues();
            return new SortedSetOrdinalsIndexFieldData.Builder(name(), CoreValuesSourceType.KEYWORD);
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            if (format != null) {
                throw new IllegalArgumentException("Field [" + name() + "] of type [" + typeName() + "] doesn't support formats.");
            }
            if (this.scriptValues != null) {
                return FieldValues.valueFetcher(this.scriptValues, context);
            }
            return new SourceValueFetcher(name(), context, nullValue) {
                @Override
                protected String parseSourceValue(Object value) {
                    String keywordValue = value.toString();
                    if (keywordValue.length() > ignoreAbove) {
                        return null;
                    }

                    NamedAnalyzer normalizer = normalizer();
                    if (normalizer == null) {
                        return keywordValue;
                    }

                    return normalizeValue(normalizer, name(), keywordValue);
                }
            };
        }

        @Override
        public Object valueForDisplay(Object value) {
            if (value == null) {
                return null;
            }
            // keywords are internally stored as utf8 bytes
            BytesRef binaryValue = (BytesRef) value;
            return binaryValue.utf8ToString();
        }

        @Override
        protected BytesRef indexedValueForSearch(Object value) {
            if (getTextSearchInfo().getSearchAnalyzer() == Lucene.KEYWORD_ANALYZER) {
                // keyword analyzer with the default attribute source which encodes terms using UTF8
                // in that case we skip normalization, which may be slow if there many terms need to
                // parse (eg. large terms query) since Analyzer.normalize involves things like creating
                // attributes through reflection
                // This if statement will be used whenever a normalizer is NOT configured
                return super.indexedValueForSearch(value);
            }

            if (value == null) {
                return null;
            }
            if (value instanceof BytesRef) {
                value = ((BytesRef) value).utf8ToString();
            }
            return getTextSearchInfo().getSearchAnalyzer().normalize(name(), value.toString());
        }

        @Override
        public CollapseType collapseType() {
            return CollapseType.KEYWORD;
        }

        /** Values that have more chars than the return value of this method will
         *  be skipped at parsing time. */
        public int ignoreAbove() {
            return ignoreAbove;
        }
    }

    private final boolean indexed;
    private final boolean hasDocValues;
    private final String nullValue;
    private final boolean eagerGlobalOrdinals;
    private final int ignoreAbove;
    private final String indexOptions;
    private final FieldType fieldType;
    private final SimilarityProvider similarity;
    private final String normalizerName;
    private final boolean splitQueriesOnWhitespace;
    private final Script script;
    private final FieldValues<String> scriptValues;
    private final ScriptCompiler scriptCompiler;


    private final IndexAnalyzers indexAnalyzers;

    protected KeywordFieldMapper(String simpleName, FieldType fieldType, KeywordFieldType mappedFieldType,
                                 MultiFields multiFields, CopyTo copyTo, Builder builder) {
        super(simpleName, mappedFieldType, mappedFieldType.normalizer, multiFields, copyTo,
            builder.script.get() != null, builder.onScriptError.getValue());
        assert fieldType.indexOptions().compareTo(IndexOptions.DOCS_AND_FREQS) <= 0;
        this.indexed = builder.indexed.getValue();
        this.hasDocValues = builder.hasDocValues.getValue();
        this.nullValue = builder.nullValue.getValue();
        this.eagerGlobalOrdinals = builder.eagerGlobalOrdinals.getValue();
        this.ignoreAbove = builder.ignoreAbove.getValue();
        this.indexOptions = builder.indexOptions.getValue();
        this.fieldType = fieldType;
        this.similarity = builder.similarity.getValue();
        this.normalizerName = builder.normalizer.getValue();
        this.splitQueriesOnWhitespace = builder.splitQueriesOnWhitespace.getValue();
        this.script = builder.script.get();
        this.scriptValues = builder.scriptValues();
        this.indexAnalyzers = builder.indexAnalyzers;
        this.scriptCompiler = builder.scriptCompiler;
    }

    @Override
    public KeywordFieldType fieldType() {
        return (KeywordFieldType) super.fieldType();
    }

    @Override
    protected void parseCreateField(ParseContext context) throws IOException {
        String value;
        if (context.externalValueSet()) {
            value = context.externalValue().toString();
        } else {
            XContentParser parser = context.parser();
            if (parser.currentToken() == XContentParser.Token.VALUE_NULL) {
                value = nullValue;
            } else {
                value = parser.textOrNull();
            }
        }

        indexValue(context, value);
    }

    @Override
    protected void indexScriptValues(SearchLookup searchLookup, LeafReaderContext readerContext, int doc, ParseContext parseContext) {
        this.scriptValues.valuesForDoc(searchLookup, readerContext, doc, value -> indexValue(parseContext, value));
    }

    private void indexValue(ParseContext context, String value) {

        if (value == null || value.length() > ignoreAbove) {
            return;
        }

        NamedAnalyzer normalizer = fieldType().normalizer();
        if (normalizer != null) {
            value = normalizeValue(normalizer, name(), value);
        }

        // convert to utf8 only once before feeding postings/dv/stored fields
        final BytesRef binaryValue = new BytesRef(value);
        if (fieldType.indexOptions() != IndexOptions.NONE || fieldType.stored())  {
            Field field = new KeywordField(fieldType().name(), binaryValue, fieldType);
            context.doc().add(field);

            if (fieldType().hasDocValues() == false && fieldType.omitNorms()) {
                createFieldNamesField(context);
            }
        }

        if (fieldType().hasDocValues()) {
            context.doc().add(new SortedSetDocValuesField(fieldType().name(), binaryValue));
        }
    }

    private static String normalizeValue(NamedAnalyzer normalizer, String field, String value) {
        try (TokenStream ts = normalizer.tokenStream(field, value)) {
            final CharTermAttribute termAtt = ts.addAttribute(CharTermAttribute.class);
            ts.reset();
            if (ts.incrementToken() == false) {
                throw new IllegalStateException("The normalization token stream is "
                    + "expected to produce exactly 1 token, but got 0 for analyzer "
                    + normalizer + " and input \"" + value + "\"");
            }
            final String newValue = termAtt.toString();
            if (ts.incrementToken()) {
                throw new IllegalStateException("The normalization token stream is "
                    + "expected to produce exactly 1 token, but got 2+ for analyzer "
                    + normalizer + " and input \"" + value + "\"");
            }
            ts.end();
            return newValue;
        }
        catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return new Builder(simpleName(), indexAnalyzers, scriptCompiler).init(this);
    }
}
