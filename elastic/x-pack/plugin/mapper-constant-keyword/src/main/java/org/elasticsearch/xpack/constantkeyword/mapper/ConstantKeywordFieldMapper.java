/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */


package org.elasticsearch.xpack.constantkeyword.mapper;

import org.apache.lucene.document.FieldType;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.MultiTermQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.UnicodeUtil;
import org.apache.lucene.util.automaton.Automaton;
import org.apache.lucene.util.automaton.CharacterRunAutomaton;
import org.apache.lucene.util.automaton.LevenshteinAutomata;
import org.apache.lucene.util.automaton.RegExp;
import org.elasticsearch.common.geo.ShapeRelation;
import org.elasticsearch.common.lucene.BytesRefs;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.time.DateMathParser;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.plain.ConstantIndexFieldData;
import org.elasticsearch.index.mapper.ConstantFieldType;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.KeywordFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.Mapper;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.index.mapper.ParseContext;
import org.elasticsearch.index.mapper.TypeParsers;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;

import java.io.IOException;
import java.time.ZoneId;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * A {@link FieldMapper} that assigns every document the same value.
 */
public class ConstantKeywordFieldMapper extends FieldMapper {

    public static final String CONTENT_TYPE = "constant_keyword";

    public static class Defaults {
        public static final FieldType FIELD_TYPE = new FieldType();
        static {
            FIELD_TYPE.setIndexOptions(IndexOptions.NONE);
            FIELD_TYPE.freeze();
        }
    }

    public static class Builder extends FieldMapper.Builder<Builder> {

        String value;

        public Builder(String name) {
            super(name, Defaults.FIELD_TYPE);
            builder = this;
        }

        // TODO we should ban setting 'index' on constant keyword

        public Builder setValue(String value) {
            this.value = value;
            return this;
        }

        @Override
        public ConstantKeywordFieldMapper build(BuilderContext context) {
            return new ConstantKeywordFieldMapper(
                    name, fieldType, new ConstantKeywordFieldType(buildFullName(context), value, meta));
        }
    }

    public static class TypeParser implements Mapper.TypeParser {
        @Override
        public Mapper.Builder<?> parse(String name, Map<String, Object> node, ParserContext parserContext) throws MapperParsingException {
            Object value = null;
            if (node.containsKey("value")) {
                value = node.remove("value");
                if (value == null) {
                    throw new MapperParsingException("Property [value] of field [" + name + "] can't be [null].");
                }
                if (value instanceof Number == false && value instanceof CharSequence == false) {
                    throw new MapperParsingException("Property [value] of field [" + name +
                            "] must be a number or a string, but got [" + value + "]");
                }
            }
            ConstantKeywordFieldMapper.Builder builder = new ConstantKeywordFieldMapper.Builder(name);
            if (value != null) {
                builder.setValue(value.toString());
            }
            if (node.containsKey("meta")) {
                builder.meta(TypeParsers.parseMeta(name, node.remove("meta")));
            }
            return builder;
        }
    }

    public static final class ConstantKeywordFieldType extends ConstantFieldType {

        private final String value;

        public ConstantKeywordFieldType(String name, String value, Map<String, String> meta) {
            super(name, meta);
            this.value = value;
        }

        public ConstantKeywordFieldType(String name, String value) {
            this(name, value, Collections.emptyMap());
        }

        /** Return the value that this field wraps. This may be {@code null} if the field is not configured yet. */
        public String value() {
            return value;
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public String familyTypeName() {
            return KeywordFieldMapper.CONTENT_TYPE;
        }

        @Override
        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName) {
            return new ConstantIndexFieldData.Builder(mapperService -> value, CoreValuesSourceType.BYTES);
        }

        @Override
        protected boolean matches(String pattern, QueryShardContext context) {
            if (value == null) {
                return false;
            }
            return Regex.simpleMatch(pattern, value);
        }

        @Override
        public Query existsQuery(QueryShardContext context) {
            return value != null ? new MatchAllDocsQuery() : new MatchNoDocsQuery();
        }

        @Override
        public Query rangeQuery(
                Object lowerTerm, Object upperTerm,
                boolean includeLower, boolean includeUpper,
                ShapeRelation relation, ZoneId timeZone, DateMathParser parser,
                QueryShardContext context) {
            if (this.value == null) {
                return new MatchNoDocsQuery();
            }

            final BytesRef valueAsBytesRef = new BytesRef(value);
            if (lowerTerm != null && BytesRefs.toBytesRef(lowerTerm).compareTo(valueAsBytesRef) >= (includeLower ? 1 : 0)) {
                return new MatchNoDocsQuery();
            }
            if (upperTerm != null && valueAsBytesRef.compareTo(BytesRefs.toBytesRef(upperTerm)) >= (includeUpper ? 1 : 0)) {
                return new MatchNoDocsQuery();
            }
            return new MatchAllDocsQuery();
        }

        @Override
        public Query fuzzyQuery(Object value, Fuzziness fuzziness, int prefixLength, int maxExpansions,
                boolean transpositions, QueryShardContext context) {
            if (this.value == null) {
                return new MatchNoDocsQuery();
            }

            final String termAsString = BytesRefs.toString(value);
            final int maxEdits = fuzziness.asDistance(termAsString);

            final int[] termText = new int[termAsString.codePointCount(0, termAsString.length())];
            for (int cp, i = 0, j = 0; i < termAsString.length(); i += Character.charCount(cp)) {
              termText[j++] = cp = termAsString.codePointAt(i);
            }
            final int termLength = termText.length;

            prefixLength = Math.min(prefixLength, termLength);
            final String suffix = UnicodeUtil.newString(termText, prefixLength, termText.length - prefixLength);
            final LevenshteinAutomata builder = new LevenshteinAutomata(suffix, transpositions);
            final String prefix = UnicodeUtil.newString(termText, 0, prefixLength);
            final Automaton automaton = builder.toAutomaton(maxEdits, prefix);

            final CharacterRunAutomaton runAutomaton = new CharacterRunAutomaton(automaton);
            if (runAutomaton.run(this.value)) {
                return new MatchAllDocsQuery();
            } else {
                return new MatchNoDocsQuery();
            }
        }

        @Override
        public Query regexpQuery(String value, int flags, int maxDeterminizedStates,
                MultiTermQuery.RewriteMethod method, QueryShardContext context) {
            if (this.value == null) {
                return new MatchNoDocsQuery();
            }

            final Automaton automaton = new RegExp(value, flags).toAutomaton(maxDeterminizedStates);
            final CharacterRunAutomaton runAutomaton = new CharacterRunAutomaton(automaton);
            if (runAutomaton.run(this.value)) {
                return new MatchAllDocsQuery();
            } else {
                return new MatchNoDocsQuery();
            }
        }

    }

    ConstantKeywordFieldMapper(String simpleName, FieldType fieldType, MappedFieldType mappedFieldType) {
        super(simpleName, fieldType, mappedFieldType, MultiFields.empty(), CopyTo.empty());
    }

    @Override
    protected ConstantKeywordFieldMapper clone() {
        return (ConstantKeywordFieldMapper) super.clone();
    }

    @Override
    public ConstantKeywordFieldType fieldType() {
        return (ConstantKeywordFieldType) super.fieldType();
    }

    @Override
    protected void parseCreateField(ParseContext context) throws IOException {
        String value;
        if (context.externalValueSet()) {
            value = context.externalValue().toString();
        } else {
            XContentParser parser = context.parser();
            value =  parser.textOrNull();
        }

        if (value == null) {
            throw new IllegalArgumentException("[constant_keyword] field [" + name() + "] doesn't accept [null] values");
        }

        if (fieldType().value == null) {
            ConstantKeywordFieldType newFieldType = new ConstantKeywordFieldType(fieldType().name(), value, fieldType().meta());
            Mapper update = new ConstantKeywordFieldMapper(simpleName(), fieldType, newFieldType);
            context.addDynamicMapper(update);
        } else if (Objects.equals(fieldType().value, value) == false) {
            throw new IllegalArgumentException("[constant_keyword] field [" + name() +
                    "] only accepts values that are equal to the value defined in the mappings [" + fieldType().value() +
                    "], but got [" + value + "]");
        }
    }

    @Override
    protected void mergeOptions(FieldMapper other, List<String> conflicts) {
        ConstantKeywordFieldType newConstantKeywordFT = (ConstantKeywordFieldType) other.fieldType();
        if (this.fieldType().value != null) {
            if (newConstantKeywordFT.value == null) {
                conflicts.add("mapper [" + name() + "] cannot unset [value]");
            } else if (Objects.equals(fieldType().value, newConstantKeywordFT.value) == false) {
                conflicts.add("mapper [" + name() + "] has different [value] from the value that is configured in mappings: ["
                    + fieldType().value + "] vs. [" + newConstantKeywordFT.value + "]");
            }
        }
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    protected void doXContentBody(XContentBuilder builder, boolean includeDefaults, Params params) throws IOException {
        super.doXContentBody(builder, includeDefaults, params);
        if (fieldType().value() != null) {
            builder.field("value", fieldType().value());
        }
    }
}
