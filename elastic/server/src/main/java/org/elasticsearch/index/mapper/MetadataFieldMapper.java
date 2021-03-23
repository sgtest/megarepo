/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.elasticsearch.common.Explicit;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.index.analysis.NamedAnalyzer;

import java.io.IOException;
import java.util.Map;
import java.util.function.Function;


/**
 * A mapper for a builtin field containing metadata about a document.
 */
public abstract class MetadataFieldMapper extends FieldMapper {

    public interface TypeParser extends Mapper.TypeParser {

        @Override
        MetadataFieldMapper.Builder parse(String name, Map<String, Object> node,
                                               ParserContext parserContext) throws MapperParsingException;

        /**
         * Get the default {@link MetadataFieldMapper} to use, if nothing had to be parsed.
         *
         * @param parserContext context that may be useful to build the field like analyzers
         */
        MetadataFieldMapper getDefault(ParserContext parserContext);
    }

    /**
     * Declares an updateable boolean parameter for a metadata field
     *
     * We need to distinguish between explicit configuration and default value for metadata
     * fields, because mapping updates will carry over the previous metadata values if a
     * metadata field is not explicitly declared in the update.  A standard boolean
     * parameter explicitly configured with a default value will not be serialized (as
     * we do not serialize default parameters for mapping updates), and as such will be
     * ignored by the update merge.  Instead, we use an {@link Explicit} object that
     * will serialize its value if it has been configured, no matter what the value is.
     */
    public static Parameter<Explicit<Boolean>> updateableBoolParam(String name, Function<FieldMapper, Explicit<Boolean>> initializer,
                                                                   boolean defaultValue) {
        Explicit<Boolean> defaultExplicit = new Explicit<>(defaultValue, false);
        return new Parameter<>(name, true, () -> defaultExplicit,
            (n, c, o) -> new Explicit<>(XContentMapValues.nodeBooleanValue(o), true), initializer)
            .setSerializer((b, n, v) -> b.field(n, v.value()), v -> Boolean.toString(v.value()));
    }

    /**
     * A type parser for an unconfigurable metadata field.
     */
    public static class FixedTypeParser implements TypeParser {

        final Function<ParserContext, MetadataFieldMapper> mapperParser;

        public FixedTypeParser(Function<ParserContext, MetadataFieldMapper> mapperParser) {
            this.mapperParser = mapperParser;
        }

        @Override
        public Builder parse(String name, Map<String, Object> node, ParserContext parserContext) throws MapperParsingException {
            throw new MapperParsingException(name + " is not configurable");
        }

        @Override
        public MetadataFieldMapper getDefault(ParserContext parserContext) {
            return mapperParser.apply(parserContext);
        }
    }

    public static class ConfigurableTypeParser implements TypeParser {

        final Function<ParserContext, MetadataFieldMapper> defaultMapperParser;
        final Function<ParserContext, Builder> builderFunction;

        public ConfigurableTypeParser(Function<ParserContext, MetadataFieldMapper> defaultMapperParser,
                                      Function<ParserContext, Builder> builderFunction) {
            this.defaultMapperParser = defaultMapperParser;
            this.builderFunction = builderFunction;
        }

        @Override
        public Builder parse(String name, Map<String, Object> node, ParserContext parserContext) throws MapperParsingException {
            Builder builder = builderFunction.apply(parserContext);
            builder.parse(name, parserContext, node);
            return builder;
        }

        @Override
        public MetadataFieldMapper getDefault(ParserContext parserContext) {
            return defaultMapperParser.apply(parserContext);
        }
    }

    public abstract static class Builder extends FieldMapper.Builder {

        protected Builder(String name) {
            super(name);
        }

        boolean isConfigured() {
            for (Parameter<?> param : getParameters()) {
                if (param.isConfigured()) {
                    return true;
                }
            }
            return false;
        }

        @Override
        public final MetadataFieldMapper build(ContentPath path) {
            return build();
        }

        public abstract MetadataFieldMapper build();
    }

    protected MetadataFieldMapper(MappedFieldType mappedFieldType) {
        super(mappedFieldType.name(), mappedFieldType, MultiFields.empty(), CopyTo.empty());
    }

    protected MetadataFieldMapper(MappedFieldType mappedFieldType, NamedAnalyzer indexAnalyzer) {
        super(mappedFieldType.name(), mappedFieldType, indexAnalyzer, MultiFields.empty(), CopyTo.empty());
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return null;    // by default, things can't be configured so we have no builder
    }

    @Override
    public final XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        MetadataFieldMapper.Builder mergeBuilder = (MetadataFieldMapper.Builder) getMergeBuilder();
        if (mergeBuilder == null || mergeBuilder.isConfigured() == false) {
            return builder;
        }
        builder.startObject(simpleName());
        getMergeBuilder().toXContent(builder, params);
        return builder.endObject();
    }

    @Override
    protected void parseCreateField(ParseContext context) throws IOException {
        throw new MapperParsingException("Field [" + name() + "] is a metadata field and cannot be added inside"
            + " a document. Use the index API request parameters.");
    }

    /**
     * Called before {@link FieldMapper#parse(ParseContext)} on the {@link RootObjectMapper}.
     */
    public void preParse(ParseContext context) throws IOException {
        // do nothing
    }

    /**
     * Called after {@link FieldMapper#parse(ParseContext)} on the {@link RootObjectMapper}.
     */
    public void postParse(ParseContext context) throws IOException {
        // do nothing
    }

}
