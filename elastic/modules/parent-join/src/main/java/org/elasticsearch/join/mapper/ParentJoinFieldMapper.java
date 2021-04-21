/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.join.mapper;

import org.apache.lucene.document.Field;
import org.apache.lucene.document.FieldType;
import org.apache.lucene.document.SortedDocValuesField;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.plain.SortedSetOrdinalsIndexFieldData;
import org.elasticsearch.index.mapper.ContentPath;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.Mapper;
import org.elasticsearch.index.mapper.MappingLookup;
import org.elasticsearch.index.mapper.ParseContext;
import org.elasticsearch.index.mapper.SourceValueFetcher;
import org.elasticsearch.index.mapper.StringFieldType;
import org.elasticsearch.index.mapper.TextSearchInfo;
import org.elasticsearch.index.mapper.ValueFetcher;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.lookup.SearchLookup;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;
import java.util.stream.Collectors;

/**
 * A {@link FieldMapper} that creates hierarchical joins (parent-join) between documents in the same index.
 * Only one parent-join field can be defined per index.
 */
public final class ParentJoinFieldMapper extends FieldMapper {

    public static final String NAME = "join";
    public static final String CONTENT_TYPE = "join";

    public static class Defaults {
        public static final FieldType FIELD_TYPE = new FieldType();

        static {
            FIELD_TYPE.setTokenized(false);
            FIELD_TYPE.setOmitNorms(true);
            FIELD_TYPE.setIndexOptions(IndexOptions.DOCS);
            FIELD_TYPE.freeze();
        }
    }

    private static void checkIndexCompatibility(IndexSettings settings, String name) {
        if (settings.getIndexMetadata().isRoutingPartitionedIndex()) {
            throw new IllegalStateException("cannot create join field [" + name + "] " +
                "for the partitioned index " + "[" + settings.getIndex().getName() + "]");
        }
    }

    private static void checkObjectOrNested(ContentPath path, String name) {
        if (path.pathAsText(name).contains(".")) {
            throw new IllegalArgumentException("join field [" + path.pathAsText(name) + "] " +
                "cannot be added inside an object or in a multi-field");
        }
    }

    private static ParentJoinFieldMapper toType(FieldMapper in) {
        return (ParentJoinFieldMapper) in;
    }

    public static class Builder extends FieldMapper.Builder {

        final Parameter<Boolean> eagerGlobalOrdinals = Parameter.boolParam("eager_global_ordinals", true,
            m -> toType(m).eagerGlobalOrdinals, true);
        final Parameter<List<Relations>> relations = new Parameter<List<Relations>>("relations", true,
            Collections::emptyList, (n, c, o) -> Relations.parse(o), m -> toType(m).relations)
            .setMergeValidator(ParentJoinFieldMapper::checkRelationsConflicts);

        final Parameter<Map<String, String>> meta = Parameter.metaParam();

        public Builder(String name) {
            super(name);
        }

        public Builder addRelation(String parent, Set<String> children) {
            relations.setValue(Collections.singletonList(new Relations(parent, children)));
            return this;
        }

        @Override
        protected List<Parameter<?>> getParameters() {
            return Arrays.asList(eagerGlobalOrdinals, relations, meta);
        }

        @Override
        public ParentJoinFieldMapper build(ContentPath contentPath) {
            checkObjectOrNested(contentPath, name);
            final Map<String, ParentIdFieldMapper> parentIdFields = new HashMap<>();
            relations.get().stream()
                .map(relation -> new ParentIdFieldMapper(name + "#" + relation.parent, eagerGlobalOrdinals.get()))
                .forEach(mapper -> parentIdFields.put(mapper.name(), mapper));
            Joiner joiner = new Joiner(name(), relations.get());
            return new ParentJoinFieldMapper(name, new JoinFieldType(buildFullName(contentPath), joiner, meta.get()),
                Collections.unmodifiableMap(parentIdFields), eagerGlobalOrdinals.get(), relations.get());
        }
    }

    public static TypeParser PARSER = new TypeParser((n, c) -> {
        checkIndexCompatibility(c.getIndexSettings(), n);
        return new Builder(n);
    });

    public static final class JoinFieldType extends StringFieldType {

        private final Joiner joiner;

        private JoinFieldType(String name, Joiner joiner, Map<String, String> meta) {
            super(name, true, false, true, TextSearchInfo.SIMPLE_MATCH_ONLY, meta);
            this.joiner = joiner;
        }

        Joiner getJoiner() {
            return joiner;
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
            return new SortedSetOrdinalsIndexFieldData.Builder(name(), CoreValuesSourceType.KEYWORD);
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            return SourceValueFetcher.identity(name(), context, format);
        }

        @Override
        public Object valueForDisplay(Object value) {
            if (value == null) {
                return null;
            }
            BytesRef binaryValue = (BytesRef) value;
            return binaryValue.utf8ToString();
        }
    }

    private static boolean checkRelationsConflicts(List<Relations> previous, List<Relations> current, Conflicts conflicts) {
        Joiner pj = new Joiner("f", previous);
        Joiner cj = new Joiner("f", current);
        return pj.canMerge(cj, s -> conflicts.addConflict("relations", s));
    }

    private final Map<String, ParentIdFieldMapper> parentIdFields;
    private final boolean eagerGlobalOrdinals;
    private final List<Relations> relations;

    protected ParentJoinFieldMapper(String simpleName,
                                    MappedFieldType mappedFieldType,
                                    Map<String, ParentIdFieldMapper> parentIdFields,
                                    boolean eagerGlobalOrdinals, List<Relations> relations) {
        super(simpleName, mappedFieldType, Lucene.KEYWORD_ANALYZER, MultiFields.empty(), CopyTo.empty());
        this.parentIdFields = parentIdFields;
        this.eagerGlobalOrdinals = eagerGlobalOrdinals;
        this.relations = relations;
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    public JoinFieldType fieldType() {
        return (JoinFieldType) super.fieldType();
    }

    @Override
    public Iterator<Mapper> iterator() {
        List<Mapper> mappers = new ArrayList<>(parentIdFields.values());
        return mappers.iterator();
    }

    @Override
    protected void parseCreateField(ParseContext context) {
        throw new UnsupportedOperationException("parsing is implemented in parse(), this method should NEVER be called");
    }

    @Override
    public void parse(ParseContext context) throws IOException {
        context.path().add(simpleName());
        XContentParser.Token token = context.parser().currentToken();
        String name = null;
        String parent = null;
        if (token == XContentParser.Token.START_OBJECT) {
            String currentFieldName = null;
            while ((token = context.parser().nextToken()) != XContentParser.Token.END_OBJECT) {
                if (token == XContentParser.Token.FIELD_NAME) {
                    currentFieldName = context.parser().currentName();
                } else if (token == XContentParser.Token.VALUE_STRING) {
                    if ("name".equals(currentFieldName)) {
                        name = context.parser().text();
                    } else if ("parent".equals(currentFieldName)) {
                        parent = context.parser().text();
                    } else {
                        throw new IllegalArgumentException("unknown field name [" + currentFieldName + "] in join field [" + name() + "]");
                    }
                } else if (token == XContentParser.Token.VALUE_NUMBER) {
                    if ("parent".equals(currentFieldName)) {
                        parent = context.parser().numberValue().toString();
                    } else {
                        throw new IllegalArgumentException("unknown field name [" + currentFieldName + "] in join field [" + name() + "]");
                    }
                }
            }
        } else if (token == XContentParser.Token.VALUE_STRING) {
            name = context.parser().text();
            parent = null;
        } else {
            throw new IllegalStateException("[" + name()  + "] expected START_OBJECT or VALUE_STRING but was: " + token);
        }

        if (name == null) {
            throw new IllegalArgumentException("null join name in field [" + name() + "]");
        }

        if (fieldType().joiner.knownRelation(name) == false) {
            throw new IllegalArgumentException("unknown join name [" + name + "] for field [" + name() + "]");
        }
        if (fieldType().joiner.childTypeExists(name)) {
            // Index the document as a child
            if (parent == null) {
                throw new IllegalArgumentException("[parent] is missing for join field [" + name() + "]");
            }
            if (context.sourceToParse().routing() == null) {
                throw new IllegalArgumentException("[routing] is missing for join field [" + name() + "]");
            }
            String fieldName = fieldType().joiner.parentJoinField(name);
            parentIdFields.get(fieldName).indexValue(context, parent);
        }
        if (fieldType().joiner.parentTypeExists(name)) {
            // Index the document as a parent
            String fieldName = fieldType().joiner.childJoinField(name);
            parentIdFields.get(fieldName).indexValue(context, context.sourceToParse().id());
        }

        BytesRef binaryValue = new BytesRef(name);
        Field field = new Field(fieldType().name(), binaryValue, Defaults.FIELD_TYPE);
        context.doc().add(field);
        context.doc().add(new SortedDocValuesField(fieldType().name(), binaryValue));
        context.path().remove();
    }

    @Override
    protected void doXContentBody(XContentBuilder builder, Params params) throws IOException {
        builder.field("type", contentType());
        builder.field("eager_global_ordinals", eagerGlobalOrdinals);
        builder.startObject("relations");
        for (Relations relation : relations) {
            if (relation.children.size() == 1) {
                builder.field(relation.parent, relation.children.iterator().next());
            } else {
                builder.field(relation.parent, relation.children);
            }
        }
        builder.endObject();
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return new Builder(simpleName()).init(this);
    }

    @Override
    protected void doValidate(MappingLookup mappers) {
        List<String> joinFields = getJoinFieldTypes(mappers.fieldTypes()).stream()
            .map(JoinFieldType::name)
            .collect(Collectors.toList());
        if (joinFields.size() > 1) {
            throw new IllegalArgumentException("Only one [parent-join] field can be defined per index, got " + joinFields);
        }
    }

    static JoinFieldType getJoinFieldType(Collection<MappedFieldType> fieldTypes) {
        for (MappedFieldType ft : fieldTypes) {
            if (ft instanceof JoinFieldType) {
                return (JoinFieldType) ft;
            }
        }
        return null;
    }

    private List<JoinFieldType> getJoinFieldTypes(Collection<MappedFieldType> fieldTypes) {
        final List<JoinFieldType> joinFields = new ArrayList<>();
        for (MappedFieldType ft : fieldTypes) {
            if (ft instanceof JoinFieldType) {
                joinFields.add((JoinFieldType) ft);
            }
        }
        return joinFields;
    }
}
