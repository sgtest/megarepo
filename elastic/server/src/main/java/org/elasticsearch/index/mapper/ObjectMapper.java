/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.index.LeafReader;
import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.Version;
import org.elasticsearch.common.Explicit;
import org.elasticsearch.common.logging.DeprecationCategory;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.index.mapper.MapperService.MergeReason;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Comparator;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;

public class ObjectMapper extends Mapper implements Cloneable {
    private static final DeprecationLogger deprecationLogger = DeprecationLogger.getLogger(ObjectMapper.class);

    public static final String CONTENT_TYPE = "object";

    public static class Defaults {
        public static final boolean ENABLED = true;
    }

    public enum Dynamic {
        TRUE {
            @Override
            DynamicFieldsBuilder getDynamicFieldsBuilder() {
                return DynamicFieldsBuilder.DYNAMIC_TRUE;
            }
        },
        FALSE,
        STRICT,
        RUNTIME {
            @Override
            DynamicFieldsBuilder getDynamicFieldsBuilder() {
                return DynamicFieldsBuilder.DYNAMIC_RUNTIME;
            }
        };

        DynamicFieldsBuilder getDynamicFieldsBuilder() {
            throw new UnsupportedOperationException("Cannot create dynamic fields when dynamic is set to [" + this + "]");
        };
    }

    public static class Builder extends Mapper.Builder {

        protected Explicit<Boolean> enabled = Explicit.IMPLICIT_TRUE;

        protected Dynamic dynamic;

        protected final List<Mapper.Builder> mappersBuilders = new ArrayList<>();

        public Builder(String name) {
            super(name);
        }

        public Builder enabled(boolean enabled) {
            this.enabled = Explicit.explicitBoolean(enabled);
            return this;
        }

        public Builder dynamic(Dynamic dynamic) {
            this.dynamic = dynamic;
            return this;
        }

        public Builder add(Mapper.Builder builder) {
            mappersBuilders.add(builder);
            return this;
        }

        Builder addMappers(Map<String, Mapper> mappers) {
            mappers.forEach((name, mapper) -> mappersBuilders.add(new Mapper.Builder(name) {
                @Override
                public Mapper build(MapperBuilderContext context) {
                    return mapper;
                }
            }));
            return this;
        }

        /**
         * Adds a dynamically created Mapper to this builder.
         *
         * @param name      the name of the Mapper, including object prefixes
         * @param prefix    the object prefix of this mapper
         * @param mapper    the mapper to add
         * @param context   the DocumentParserContext in which the mapper has been built
         */
        public void addDynamic(String name, String prefix, Mapper mapper, DocumentParserContext context) {
            // If the mapper to add has no dots and is therefore
            // a leaf mapper, we just add it here
            if (name.contains(".") == false) {
                mappersBuilders.add(new Mapper.Builder(name) {
                    @Override
                    public Mapper build(MapperBuilderContext context) {
                        return mapper;
                    }
                });
            }
            // otherwise we strip off the first object path of the mapper name, load or create
            // the relevant object mapper, and then recurse down into it, passing the remainder
            // of the mapper name. So for a mapper 'foo.bar.baz', we locate 'foo' and then
            // call addDynamic on it with the name 'bar.baz'.
            else {
                int firstDotIndex = name.indexOf(".");
                String childName = name.substring(0, firstDotIndex);
                String fullChildName = prefix == null ? childName : prefix + "." + childName;
                ObjectMapper.Builder childBuilder = findChild(fullChildName, context);
                childBuilder.addDynamic(name.substring(firstDotIndex + 1), fullChildName, mapper, context);
                mappersBuilders.add(childBuilder);
            }
        }

        private static ObjectMapper.Builder findChild(String fullChildName, DocumentParserContext context) {
            // does the child mapper already exist? if so, use that
            ObjectMapper child = context.mappingLookup().objectMappers().get(fullChildName);
            if (child != null) {
                return child.newBuilder(context.indexSettings().getIndexVersionCreated());
            }
            // has the child mapper been added as a dynamic update already?
            child = context.getDynamicObjectMapper(fullChildName);
            if (child != null) {
                return child.newBuilder(context.indexSettings().getIndexVersionCreated());
            }
            throw new IllegalArgumentException("Missing intermediate object " + fullChildName);
        }

        protected final Map<String, Mapper> buildMappers(boolean root, MapperBuilderContext context) {
            if (root == false) {
                context = context.createChildContext(name);
            }
            Map<String, Mapper> mappers = new HashMap<>();
            for (Mapper.Builder builder : mappersBuilders) {
                Mapper mapper = builder.build(context);
                Mapper existing = mappers.get(mapper.simpleName());
                if (existing != null) {
                    mapper = existing.merge(mapper);
                }
                mappers.put(mapper.simpleName(), mapper);
            }
            return mappers;
        }

        @Override
        public ObjectMapper build(MapperBuilderContext context) {
            return new ObjectMapper(name, context.buildFullName(name), enabled, dynamic, buildMappers(false, context));
        }
    }

    public static class TypeParser implements Mapper.TypeParser {

        @Override
        public boolean supportsVersion(Version indexCreatedVersion) {
            return true;
        }

        @Override
        public Mapper.Builder parse(String name, Map<String, Object> node, MappingParserContext parserContext)
            throws MapperParsingException {
            ObjectMapper.Builder builder = new Builder(name);
            for (Iterator<Map.Entry<String, Object>> iterator = node.entrySet().iterator(); iterator.hasNext();) {
                Map.Entry<String, Object> entry = iterator.next();
                String fieldName = entry.getKey();
                Object fieldNode = entry.getValue();
                if (parseObjectOrDocumentTypeProperties(fieldName, fieldNode, parserContext, builder)) {
                    iterator.remove();
                }
            }
            return builder;
        }

        @SuppressWarnings({ "unchecked", "rawtypes" })
        protected static boolean parseObjectOrDocumentTypeProperties(
            String fieldName,
            Object fieldNode,
            MappingParserContext parserContext,
            ObjectMapper.Builder builder
        ) {
            if (fieldName.equals("dynamic")) {
                String value = fieldNode.toString();
                if (value.equalsIgnoreCase("strict")) {
                    builder.dynamic(Dynamic.STRICT);
                } else if (value.equalsIgnoreCase("runtime")) {
                    builder.dynamic(Dynamic.RUNTIME);
                } else {
                    boolean dynamic = XContentMapValues.nodeBooleanValue(fieldNode, fieldName + ".dynamic");
                    builder.dynamic(dynamic ? Dynamic.TRUE : Dynamic.FALSE);
                }
                return true;
            } else if (fieldName.equals("enabled")) {
                builder.enabled(XContentMapValues.nodeBooleanValue(fieldNode, fieldName + ".enabled"));
                return true;
            } else if (fieldName.equals("properties")) {
                if (fieldNode instanceof Collection && ((Collection) fieldNode).isEmpty()) {
                    // nothing to do here, empty (to support "properties: []" case)
                } else if ((fieldNode instanceof Map) == false) {
                    throw new ElasticsearchParseException("properties must be a map type");
                } else {
                    parseProperties(builder, (Map<String, Object>) fieldNode, parserContext);
                }
                return true;
            } else if (fieldName.equals("include_in_all")) {
                deprecationLogger.warn(
                    DeprecationCategory.MAPPINGS,
                    "include_in_all",
                    "[include_in_all] is deprecated, the _all field have been removed in this version"
                );
                return true;
            }
            return false;
        }

        protected static void parseProperties(
            ObjectMapper.Builder objBuilder,
            Map<String, Object> propsNode,
            MappingParserContext parserContext
        ) {
            Iterator<Map.Entry<String, Object>> iterator = propsNode.entrySet().iterator();
            while (iterator.hasNext()) {
                Map.Entry<String, Object> entry = iterator.next();
                String fieldName = entry.getKey();
                // Should accept empty arrays, as a work around for when the
                // user can't provide an empty Map. (PHP for example)
                boolean isEmptyList = entry.getValue() instanceof List && ((List<?>) entry.getValue()).isEmpty();

                if (entry.getValue() instanceof Map) {
                    @SuppressWarnings("unchecked")
                    Map<String, Object> propNode = (Map<String, Object>) entry.getValue();
                    String type;
                    Object typeNode = propNode.get("type");
                    if (typeNode != null) {
                        type = typeNode.toString();
                    } else {
                        // lets see if we can derive this...
                        if (propNode.get("properties") != null) {
                            type = ObjectMapper.CONTENT_TYPE;
                        } else if (propNode.size() == 1 && propNode.get("enabled") != null) {
                            // if there is a single property with the enabled
                            // flag on it, make it an object
                            // (usually, setting enabled to false to not index
                            // any type, including core values, which
                            type = ObjectMapper.CONTENT_TYPE;
                        } else {
                            throw new MapperParsingException("No type specified for field [" + fieldName + "]");
                        }
                    }

                    Mapper.TypeParser typeParser = parserContext.typeParser(type);
                    if (typeParser == null) {
                        throw new MapperParsingException("No handler for type [" + type + "] declared on field [" + fieldName + "]");
                    }
                    String[] fieldNameParts = fieldName.split("\\.");
                    String realFieldName = fieldNameParts[fieldNameParts.length - 1];
                    Mapper.Builder fieldBuilder = typeParser.parse(realFieldName, propNode, parserContext);
                    for (int i = fieldNameParts.length - 2; i >= 0; --i) {
                        ObjectMapper.Builder intermediate = new ObjectMapper.Builder(fieldNameParts[i]);
                        intermediate.add(fieldBuilder);
                        fieldBuilder = intermediate;
                    }
                    objBuilder.add(fieldBuilder);
                    propNode.remove("type");
                    MappingParser.checkNoRemainingFields(fieldName, propNode);
                    iterator.remove();
                } else if (isEmptyList) {
                    iterator.remove();
                } else {
                    throw new MapperParsingException(
                        "Expected map for property [fields] on field [" + fieldName + "] but got a " + fieldName.getClass()
                    );
                }
            }

            MappingParser.checkNoRemainingFields(propsNode, "DocType mapping definition has unsupported parameters: ");
        }
    }

    private final String fullPath;

    protected Explicit<Boolean> enabled;
    protected volatile Dynamic dynamic;

    protected Map<String, Mapper> mappers;

    ObjectMapper(String name, String fullPath, Explicit<Boolean> enabled, Dynamic dynamic, Map<String, Mapper> mappers) {
        super(name);
        if (name.isEmpty()) {
            throw new IllegalArgumentException("name cannot be empty string");
        }
        this.fullPath = internFieldName(fullPath);
        this.enabled = enabled;
        this.dynamic = dynamic;
        if (mappers == null) {
            this.mappers = Map.of();
        } else {
            this.mappers = Map.copyOf(mappers);
        }
    }

    @Override
    protected ObjectMapper clone() {
        ObjectMapper clone;
        try {
            clone = (ObjectMapper) super.clone();
        } catch (CloneNotSupportedException e) {
            throw new RuntimeException(e);
        }
        clone.mappers = Map.copyOf(clone.mappers);
        return clone;
    }

    /**
     * @return a Builder that will produce an empty ObjectMapper with the same configuration as this one
     */
    public ObjectMapper.Builder newBuilder(Version indexVersionCreated) {
        ObjectMapper.Builder builder = new ObjectMapper.Builder(simpleName());
        builder.enabled = this.enabled;
        builder.dynamic = this.dynamic;
        return builder;
    }

    @Override
    public String name() {
        return this.fullPath;
    }

    @Override
    public String typeName() {
        return CONTENT_TYPE;
    }

    public boolean isEnabled() {
        return this.enabled.value();
    }

    public boolean isNested() {
        return false;
    }

    public Mapper getMapper(String field) {
        return mappers.get(field);
    }

    @Override
    public Iterator<Mapper> iterator() {
        return mappers.values().iterator();
    }

    public String fullPath() {
        return this.fullPath;
    }

    public final Dynamic dynamic() {
        return dynamic;
    }

    @Override
    public ObjectMapper merge(Mapper mergeWith) {
        return merge(mergeWith, MergeReason.MAPPING_UPDATE);
    }

    @Override
    public void validate(MappingLookup mappers) {
        for (Mapper mapper : this.mappers.values()) {
            mapper.validate(mappers);
        }
    }

    public ObjectMapper merge(Mapper mergeWith, MergeReason reason) {
        if ((mergeWith instanceof ObjectMapper) == false) {
            throw new IllegalArgumentException("can't merge a non object mapping [" + mergeWith.name() + "] with an object mapping");
        }
        if (mergeWith instanceof NestedObjectMapper) {
            // TODO stop NestedObjectMapper extending ObjectMapper?
            throw new IllegalArgumentException("can't merge a nested mapping [" + mergeWith.name() + "] with a non-nested mapping");
        }
        ObjectMapper mergeWithObject = (ObjectMapper) mergeWith;
        ObjectMapper merged = clone();
        merged.doMerge(mergeWithObject, reason);
        return merged;
    }

    protected void doMerge(final ObjectMapper mergeWith, MergeReason reason) {

        if (mergeWith.dynamic != null) {
            this.dynamic = mergeWith.dynamic;
        }

        if (mergeWith.enabled.explicit()) {
            if (reason == MergeReason.INDEX_TEMPLATE) {
                this.enabled = mergeWith.enabled;
            } else if (isEnabled() != mergeWith.isEnabled()) {
                throw new MapperException("the [enabled] parameter can't be updated for the object mapping [" + name() + "]");
            }
        }

        Map<String, Mapper> mergedMappers = null;
        for (Mapper mergeWithMapper : mergeWith) {
            Mapper mergeIntoMapper = (mergedMappers == null ? mappers : mergedMappers).get(mergeWithMapper.simpleName());

            Mapper merged;
            if (mergeIntoMapper == null) {
                merged = mergeWithMapper;
            } else if (mergeIntoMapper instanceof ObjectMapper objectMapper) {
                merged = objectMapper.merge(mergeWithMapper, reason);
            } else {
                assert mergeIntoMapper instanceof FieldMapper || mergeIntoMapper instanceof FieldAliasMapper;
                if (mergeWithMapper instanceof ObjectMapper) {
                    throw new IllegalArgumentException(
                        "can't merge a non object mapping [" + mergeWithMapper.name() + "] with an object mapping"
                    );
                }

                // If we're merging template mappings when creating an index, then a field definition always
                // replaces an existing one.
                if (reason == MergeReason.INDEX_TEMPLATE) {
                    merged = mergeWithMapper;
                } else {
                    merged = mergeIntoMapper.merge(mergeWithMapper);
                }
            }
            if (mergedMappers == null) {
                mergedMappers = new HashMap<>(mappers);
            }
            mergedMappers.put(merged.simpleName(), merged);
        }
        if (mergedMappers != null) {
            mappers = Map.copyOf(mergedMappers);
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        toXContent(builder, params, null);
        return builder;
    }

    void toXContent(XContentBuilder builder, Params params, ToXContent custom) throws IOException {
        builder.startObject(simpleName());
        if (mappers.isEmpty() && custom == null) {
            // only write the object content type if there are no properties, otherwise, it is automatically detected
            builder.field("type", CONTENT_TYPE);
        }
        if (dynamic != null) {
            builder.field("dynamic", dynamic.name().toLowerCase(Locale.ROOT));
        }
        if (isEnabled() != Defaults.ENABLED) {
            builder.field("enabled", enabled.value());
        }
        if (custom != null) {
            custom.toXContent(builder, params);
        }

        doXContent(builder, params);
        serializeMappers(builder, params);
        builder.endObject();
    }

    protected void serializeMappers(XContentBuilder builder, Params params) throws IOException {
        // sort the mappers so we get consistent serialization format
        Mapper[] sortedMappers = mappers.values().toArray(Mapper[]::new);
        Arrays.sort(sortedMappers, Comparator.comparing(Mapper::name));

        int count = 0;
        for (Mapper mapper : sortedMappers) {
            if ((mapper instanceof MetadataFieldMapper) == false) {
                if (count++ == 0) {
                    builder.startObject("properties");
                }
                mapper.toXContent(builder, params);
            }
        }
        if (count > 0) {
            builder.endObject();
        }
    }

    protected void doXContent(XContentBuilder builder, Params params) throws IOException {

    }

    @Override
    public SourceLoader.SyntheticFieldLoader syntheticFieldLoader() {
        List<SourceLoader.SyntheticFieldLoader> fields = new ArrayList<>();
        mappers.values().stream().sorted(Comparator.comparing(Mapper::name)).forEach(sub -> {
            SourceLoader.SyntheticFieldLoader subLoader = sub.syntheticFieldLoader();
            if (subLoader != null) {
                fields.add(subLoader);
            }
        });
        return new SourceLoader.SyntheticFieldLoader() {
            @Override
            public Leaf leaf(LeafReader reader) throws IOException {
                List<SourceLoader.SyntheticFieldLoader.Leaf> leaves = new ArrayList<>();
                for (SourceLoader.SyntheticFieldLoader field : fields) {
                    leaves.add(field.leaf(reader));
                }
                return new SourceLoader.SyntheticFieldLoader.Leaf() {
                    @Override
                    public void advanceToDoc(int docId) throws IOException {
                        for (SourceLoader.SyntheticFieldLoader.Leaf leaf : leaves) {
                            leaf.advanceToDoc(docId);
                        }
                    }

                    @Override
                    public boolean hasValue() {
                        for (SourceLoader.SyntheticFieldLoader.Leaf leaf : leaves) {
                            if (leaf.hasValue()) {
                                return true;
                            }
                        }
                        return false;
                    }

                    @Override
                    public void load(XContentBuilder b) throws IOException {
                        boolean started = false;
                        for (SourceLoader.SyntheticFieldLoader.Leaf leaf : leaves) {
                            if (leaf.hasValue()) {
                                if (false == started) {
                                    started = true;
                                    startSyntheticField(b);
                                }
                                leaf.load(b);
                            }
                        }
                        if (started) {
                            b.endObject();
                        }
                    }
                };
            }
        };
    }

    protected void startSyntheticField(XContentBuilder b) throws IOException {
        b.startObject(simpleName());
    }
}
