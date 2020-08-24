/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.mapper;

import org.elasticsearch.common.time.DateFormatter;
import org.elasticsearch.common.util.LocaleUtils;
import org.elasticsearch.index.mapper.BooleanFieldMapper;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.IpFieldMapper;
import org.elasticsearch.index.mapper.KeywordFieldMapper;
import org.elasticsearch.index.mapper.Mapper;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.NumberFieldMapper.NumberType;
import org.elasticsearch.index.mapper.ParametrizedFieldMapper;
import org.elasticsearch.index.mapper.ParseContext;
import org.elasticsearch.index.mapper.ValueFetcher;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.xpack.runtimefields.BooleanScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.DateScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.DoubleScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.IpScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.LongScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.StringScriptFieldScript;

import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.BiFunction;

public final class RuntimeScriptFieldMapper extends ParametrizedFieldMapper {

    public static final String CONTENT_TYPE = "runtime_script";

    public static final TypeParser PARSER = new TypeParser((name, parserContext) -> new Builder(name, new ScriptCompiler() {
        @Override
        public <FactoryType> FactoryType compile(Script script, ScriptContext<FactoryType> context) {
            return parserContext.scriptService().compile(script, context);
        }
    }));

    private final String runtimeType;
    private final Script script;
    private final ScriptCompiler scriptCompiler;

    protected RuntimeScriptFieldMapper(
        String simpleName,
        AbstractScriptMappedFieldType mappedFieldType,
        MultiFields multiFields,
        CopyTo copyTo,
        String runtimeType,
        Script script,
        ScriptCompiler scriptCompiler
    ) {
        super(simpleName, mappedFieldType, multiFields, copyTo);
        this.runtimeType = runtimeType;
        this.script = script;
        this.scriptCompiler = scriptCompiler;
    }

    @Override
    public ParametrizedFieldMapper.Builder getMergeBuilder() {
        return new RuntimeScriptFieldMapper.Builder(simpleName(), scriptCompiler).init(this);
    }

    @Override
    protected void parseCreateField(ParseContext context) {
        // there is no lucene field
    }

    @Override
    public ValueFetcher valueFetcher(MapperService mapperService, String format) {
        throw new UnsupportedOperationException();
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    public static class Builder extends ParametrizedFieldMapper.Builder {

        static final Map<String, BiFunction<Builder, BuilderContext, AbstractScriptMappedFieldType>> FIELD_TYPE_RESOLVER = Map.of(
            BooleanFieldMapper.CONTENT_TYPE,
            (builder, context) -> {
                builder.formatAndLocaleNotSupported();
                BooleanScriptFieldScript.Factory factory = builder.scriptCompiler.compile(
                    builder.script.getValue(),
                    BooleanScriptFieldScript.CONTEXT
                );
                return new ScriptBooleanMappedFieldType(
                    builder.buildFullName(context),
                    builder.script.getValue(),
                    factory,
                    builder.meta.getValue()
                );
            },
            DateFieldMapper.CONTENT_TYPE,
            (builder, context) -> {
                DateScriptFieldScript.Factory factory = builder.scriptCompiler.compile(
                    builder.script.getValue(),
                    DateScriptFieldScript.CONTEXT
                );
                String format = builder.format.getValue();
                if (format == null) {
                    format = DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER.pattern();
                }
                Locale locale = builder.locale.getValue();
                if (locale == null) {
                    locale = Locale.ROOT;
                }
                DateFormatter dateTimeFormatter = DateFormatter.forPattern(format).withLocale(locale);
                return new ScriptDateMappedFieldType(
                    builder.buildFullName(context),
                    builder.script.getValue(),
                    factory,
                    dateTimeFormatter,
                    builder.meta.getValue()
                );
            },
            NumberType.DOUBLE.typeName(),
            (builder, context) -> {
                builder.formatAndLocaleNotSupported();
                DoubleScriptFieldScript.Factory factory = builder.scriptCompiler.compile(
                    builder.script.getValue(),
                    DoubleScriptFieldScript.CONTEXT
                );
                return new ScriptDoubleMappedFieldType(
                    builder.buildFullName(context),
                    builder.script.getValue(),
                    factory,
                    builder.meta.getValue()
                );
            },
            IpFieldMapper.CONTENT_TYPE,
            (builder, context) -> {
                builder.formatAndLocaleNotSupported();
                IpScriptFieldScript.Factory factory = builder.scriptCompiler.compile(
                    builder.script.getValue(),
                    IpScriptFieldScript.CONTEXT
                );
                return new ScriptIpMappedFieldType(
                    builder.buildFullName(context),
                    builder.script.getValue(),
                    factory,
                    builder.meta.getValue()
                );
            },
            KeywordFieldMapper.CONTENT_TYPE,
            (builder, context) -> {
                builder.formatAndLocaleNotSupported();
                StringScriptFieldScript.Factory factory = builder.scriptCompiler.compile(
                    builder.script.getValue(),
                    StringScriptFieldScript.CONTEXT
                );
                return new ScriptKeywordMappedFieldType(
                    builder.buildFullName(context),
                    builder.script.getValue(),
                    factory,
                    builder.meta.getValue()
                );
            },
            NumberType.LONG.typeName(),
            (builder, context) -> {
                builder.formatAndLocaleNotSupported();
                LongScriptFieldScript.Factory factory = builder.scriptCompiler.compile(
                    builder.script.getValue(),
                    LongScriptFieldScript.CONTEXT
                );
                return new ScriptLongMappedFieldType(
                    builder.buildFullName(context),
                    builder.script.getValue(),
                    factory,
                    builder.meta.getValue()
                );
            }
        );

        private static RuntimeScriptFieldMapper toType(FieldMapper in) {
            return (RuntimeScriptFieldMapper) in;
        }

        private final Parameter<Map<String, String>> meta = Parameter.metaParam();
        private final Parameter<String> runtimeType = Parameter.stringParam(
            "runtime_type",
            true,
            mapper -> toType(mapper).runtimeType,
            null
        ).setValidator(runtimeType -> {
            if (runtimeType == null) {
                throw new IllegalArgumentException("runtime_type must be specified for " + CONTENT_TYPE + " field [" + name + "]");
            }
        });
        private final Parameter<Script> script = new Parameter<>(
            "script",
            true,
            () -> null,
            Builder::parseScript,
            mapper -> toType(mapper).script
        ).setValidator(script -> {
            if (script == null) {
                throw new IllegalArgumentException("script must be specified for " + CONTENT_TYPE + " field [" + name + "]");
            }
        });
        private final Parameter<String> format = Parameter.stringParam(
            "format",
            true,
            mapper -> ((AbstractScriptMappedFieldType) mapper.fieldType()).format(),
            null
        ).setSerializer((b, n, v) -> {
            if (v != null && false == v.equals(DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER.pattern())) {
                b.field(n, v);
            }
        }, Object::toString).acceptsNull();
        private final Parameter<Locale> locale = new Parameter<>(
            "locale",
            true,
            () -> null,
            (n, c, o) -> o == null ? null : LocaleUtils.parse(o.toString()),
            mapper -> ((AbstractScriptMappedFieldType) mapper.fieldType()).formatLocale()
        ).setSerializer((b, n, v) -> {
            if (v != null && false == v.equals(Locale.ROOT)) {
                b.field(n, v.toString());
            }
        }, Object::toString).acceptsNull();

        private final ScriptCompiler scriptCompiler;

        protected Builder(String name, ScriptCompiler scriptCompiler) {
            super(name);
            this.scriptCompiler = scriptCompiler;
        }

        @Override
        protected List<Parameter<?>> getParameters() {
            return List.of(meta, runtimeType, script, format, locale);
        }

        @Override
        public RuntimeScriptFieldMapper build(BuilderContext context) {
            BiFunction<Builder, BuilderContext, AbstractScriptMappedFieldType> fieldTypeResolver = Builder.FIELD_TYPE_RESOLVER.get(
                runtimeType.getValue()
            );
            if (fieldTypeResolver == null) {
                throw new IllegalArgumentException(
                    "runtime_type [" + runtimeType.getValue() + "] not supported for " + CONTENT_TYPE + " field [" + name + "]"
                );
            }
            MultiFields multiFields = multiFieldsBuilder.build(this, context);
            if (multiFields.iterator().hasNext()) {
                throw new IllegalArgumentException(CONTENT_TYPE + " field does not support [fields]");
            }
            CopyTo copyTo = this.copyTo.build();
            if (copyTo.copyToFields().isEmpty() == false) {
                throw new IllegalArgumentException(CONTENT_TYPE + " field does not support [copy_to]");
            }
            return new RuntimeScriptFieldMapper(
                name,
                fieldTypeResolver.apply(this, context),
                MultiFields.empty(),
                CopyTo.empty(),
                runtimeType.getValue(),
                script.getValue(),
                scriptCompiler
            );
        }

        static Script parseScript(String name, Mapper.TypeParser.ParserContext parserContext, Object scriptObject) {
            Script script = Script.parse(scriptObject);
            if (script.getType() == ScriptType.STORED) {
                throw new IllegalArgumentException(
                    "stored scripts specified but not supported for " + CONTENT_TYPE + " field [" + name + "]"
                );
            }
            return script;
        }

        private void formatAndLocaleNotSupported() {
            if (format.getValue() != null) {
                throw new IllegalArgumentException("format can not be specified for runtime_type [" + runtimeType.getValue() + "]");
            }
            if (locale.getValue() != null) {
                throw new IllegalArgumentException("locale can not be specified for runtime_type [" + runtimeType.getValue() + "]");
            }
        }
    }

    @FunctionalInterface
    private interface ScriptCompiler {
        <FactoryType> FactoryType compile(Script script, ScriptContext<FactoryType> context);
    }
}
