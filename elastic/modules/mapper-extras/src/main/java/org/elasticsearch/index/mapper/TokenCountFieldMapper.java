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
import org.apache.lucene.analysis.tokenattributes.PositionIncrementAttribute;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.query.SearchExecutionContext;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.common.xcontent.support.XContentMapValues.nodeIntegerValue;

/**
 * A {@link FieldMapper} that takes a string and writes a count of the tokens in that string
 * to the index.  In most ways the mapper acts just like an {@link NumberFieldMapper}.
 */
public class TokenCountFieldMapper extends FieldMapper {
    public static final String CONTENT_TYPE = "token_count";

    private static TokenCountFieldMapper toType(FieldMapper in) {
        return (TokenCountFieldMapper) in;
    }

    public static class Builder extends FieldMapper.Builder {

        private final Parameter<Boolean> index = Parameter.indexParam(m -> toType(m).index, true);
        private final Parameter<Boolean> hasDocValues = Parameter.docValuesParam(m -> toType(m).hasDocValues, true);
        private final Parameter<Boolean> store = Parameter.storeParam(m -> toType(m).store, false);

        private final Parameter<NamedAnalyzer> analyzer
            = Parameter.analyzerParam("analyzer", true, m -> toType(m).analyzer, () -> null);
        private final Parameter<Integer> nullValue = new Parameter<>(
            "null_value", false, () -> null,
            (n, c, o) -> o == null ? null : nodeIntegerValue(o), m -> toType(m).nullValue).acceptsNull();
        private final Parameter<Boolean> enablePositionIncrements
            = Parameter.boolParam("enable_position_increments", false, m -> toType(m).enablePositionIncrements, true);

        private final Parameter<Map<String, String>> meta = Parameter.metaParam();

        public Builder(String name) {
            super(name);
        }

        @Override
        protected List<Parameter<?>> getParameters() {
            return Arrays.asList(index, hasDocValues, store, analyzer, nullValue, enablePositionIncrements, meta);
        }

        @Override
        public TokenCountFieldMapper build(ContentPath contentPath) {
            if (analyzer.getValue() == null) {
                throw new MapperParsingException("Analyzer must be set for field [" + name + "] but wasn't.");
            }
            MappedFieldType ft = new TokenCountFieldType(
                buildFullName(contentPath),
                index.getValue(),
                store.getValue(),
                hasDocValues.getValue(),
                nullValue.getValue(),
                meta.getValue());
            return new TokenCountFieldMapper(name, ft, multiFieldsBuilder.build(this, contentPath), copyTo.build(), this);
        }
    }

    static class TokenCountFieldType extends NumberFieldMapper.NumberFieldType {

        TokenCountFieldType(String name, boolean isSearchable, boolean isStored,
                            boolean hasDocValues, Number nullValue, Map<String, String> meta) {
            super(name, NumberFieldMapper.NumberType.INTEGER, isSearchable, isStored, hasDocValues, false, nullValue, meta);
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            if (hasDocValues() == false) {
                return (lookup, ignoredFields) -> List.of();
            }
            return new DocValueFetcher(docValueFormat(format, null), context.getForField(this));
        }
    }

    public static TypeParser PARSER = new TypeParser((n, c) -> new Builder(n));

    private final boolean index;
    private final boolean hasDocValues;
    private final boolean store;
    private final NamedAnalyzer analyzer;
    private final boolean enablePositionIncrements;
    private final Integer nullValue;

    protected TokenCountFieldMapper(String simpleName, MappedFieldType defaultFieldType,
                                    MultiFields multiFields, CopyTo copyTo, Builder builder) {
        super(simpleName, defaultFieldType, multiFields, copyTo);
        this.analyzer = builder.analyzer.getValue();
        this.enablePositionIncrements = builder.enablePositionIncrements.getValue();
        this.nullValue = builder.nullValue.getValue();
        this.index = builder.index.getValue();
        this.hasDocValues = builder.hasDocValues.getValue();
        this.store = builder.store.getValue();
    }

    @Override
    protected void parseCreateField(ParseContext context) throws IOException {
        final String value;
        if (context.externalValueSet()) {
            value = context.externalValue().toString();
        } else {
            value = context.parser().textOrNull();
        }

        if (value == null && nullValue == null) {
            return;
        }

        final int tokenCount;
        if (value == null) {
            tokenCount = nullValue;
        } else {
            tokenCount = countPositions(analyzer, name(), value, enablePositionIncrements);
        }

        context.doc().addAll(
            NumberFieldMapper.NumberType.INTEGER.createFields(fieldType().name(), tokenCount, index, hasDocValues, store)
        );
    }

    /**
     * Count position increments in a token stream.  Package private for testing.
     * @param analyzer analyzer to create token stream
     * @param fieldName field name to pass to analyzer
     * @param fieldValue field value to pass to analyzer
     * @param enablePositionIncrements should we count position increments ?
     * @return number of position increments in a token stream
     * @throws IOException if tokenStream throws it
     */
    static int countPositions(Analyzer analyzer, String fieldName, String fieldValue, boolean enablePositionIncrements) throws IOException {
        try (TokenStream tokenStream = analyzer.tokenStream(fieldName, fieldValue)) {
            int count = 0;
            PositionIncrementAttribute position = tokenStream.addAttribute(PositionIncrementAttribute.class);
            tokenStream.reset();
            while (tokenStream.incrementToken()) {
                if (enablePositionIncrements) {
                    count += position.getPositionIncrement();
                } else {
                    count += Math.min(1, position.getPositionIncrement());
                }
            }
            tokenStream.end();
            if (enablePositionIncrements) {
                count += position.getPositionIncrement();
            }
            return count;
        }
    }

    /**
     * Name of analyzer.
     * @return name of analyzer
     */
    public String analyzer() {
        return analyzer.name();
    }

    /**
     * Indicates if position increments are counted.
     * @return <code>true</code> if position increments are counted
     */
    public boolean enablePositionIncrements() {
        return enablePositionIncrements;
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return new Builder(simpleName()).init(this);
    }
}
