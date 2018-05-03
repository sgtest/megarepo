/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.index.mapper;

import com.carrotsearch.hppc.ObjectHashSet;
import com.carrotsearch.hppc.cursors.ObjectCursor;

import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.DelegatingAnalyzerWrapper;
import org.apache.lucene.index.Term;
import org.elasticsearch.ElasticsearchGenerationException;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.AbstractIndexComponent;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.IndexSortConfig;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.mapper.Mapper.BuilderContext;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.index.similarity.SimilarityService;
import org.elasticsearch.indices.InvalidTypeNameException;
import org.elasticsearch.indices.mapper.MapperRegistry;

import java.io.Closeable;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.function.Function;
import java.util.function.Supplier;

import static java.util.Collections.emptyMap;
import static java.util.Collections.unmodifiableMap;

public class MapperService extends AbstractIndexComponent implements Closeable {

    /**
     * The reason why a mapping is being merged.
     */
    public enum MergeReason {
        /**
         * Create or update a mapping.
         */
        MAPPING_UPDATE,
        /**
         * Recovery of an existing mapping, for instance because of a restart,
         * if a shard was moved to a different node or for administrative
         * purposes.
         */
        MAPPING_RECOVERY;
    }

    public static final String DEFAULT_MAPPING = "_default_";
    public static final String SINGLE_MAPPING_NAME = "_doc";
    public static final Setting<Long> INDEX_MAPPING_NESTED_FIELDS_LIMIT_SETTING =
        Setting.longSetting("index.mapping.nested_fields.limit", 50L, 0, Property.Dynamic, Property.IndexScope);
    // maximum allowed number of nested json objects across all fields in a single document
    public static final Setting<Long> INDEX_MAPPING_NESTED_DOCS_LIMIT_SETTING =
        Setting.longSetting("index.mapping.nested_objects.limit", 10000L, 0, Property.Dynamic, Property.IndexScope);
    public static final Setting<Long> INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING =
        Setting.longSetting("index.mapping.total_fields.limit", 1000L, 0, Property.Dynamic, Property.IndexScope);
    public static final Setting<Long> INDEX_MAPPING_DEPTH_LIMIT_SETTING =
            Setting.longSetting("index.mapping.depth.limit", 20L, 1, Property.Dynamic, Property.IndexScope);
    public static final boolean INDEX_MAPPER_DYNAMIC_DEFAULT = true;
    @Deprecated
    public static final Setting<Boolean> INDEX_MAPPER_DYNAMIC_SETTING =
        Setting.boolSetting("index.mapper.dynamic", INDEX_MAPPER_DYNAMIC_DEFAULT,
                Property.Dynamic, Property.IndexScope, Property.Deprecated);

    //TODO this needs to be cleaned up: _timestamp and _ttl are not supported anymore, _field_names, _seq_no, _version and _source are
    //also missing, not sure if on purpose. See IndicesModule#getMetadataMappers
    private static ObjectHashSet<String> META_FIELDS = ObjectHashSet.from(
            "_id", "_type", "_routing", "_index",
            "_size", "_timestamp", "_ttl", IgnoredFieldMapper.NAME
    );

    private static final DeprecationLogger DEPRECATION_LOGGER = new DeprecationLogger(Loggers.getLogger(MapperService.class));

    private final IndexAnalyzers indexAnalyzers;

    private volatile String defaultMappingSource;

    private volatile DocumentMapper mapper;
    private volatile DocumentMapper defaultMapper;

    private volatile FieldTypeLookup fieldTypes;
    private volatile Map<String, ObjectMapper> fullPathObjectMappers = emptyMap();
    private boolean hasNested = false; // updated dynamically to true when a nested object is added

    private final DocumentMapperParser documentParser;

    private final MapperAnalyzerWrapper indexAnalyzer;
    private final MapperAnalyzerWrapper searchAnalyzer;
    private final MapperAnalyzerWrapper searchQuoteAnalyzer;

    private volatile Map<String, MappedFieldType> unmappedFieldTypes = emptyMap();

    final MapperRegistry mapperRegistry;

    public MapperService(IndexSettings indexSettings, IndexAnalyzers indexAnalyzers, NamedXContentRegistry xContentRegistry,
                         SimilarityService similarityService, MapperRegistry mapperRegistry,
                         Supplier<QueryShardContext> queryShardContextSupplier) {
        super(indexSettings);
        this.indexAnalyzers = indexAnalyzers;
        this.fieldTypes = new FieldTypeLookup();
        this.documentParser = new DocumentMapperParser(indexSettings, this, indexAnalyzers, xContentRegistry, similarityService,
                mapperRegistry, queryShardContextSupplier);
        this.indexAnalyzer = new MapperAnalyzerWrapper(indexAnalyzers.getDefaultIndexAnalyzer(), p -> p.indexAnalyzer());
        this.searchAnalyzer = new MapperAnalyzerWrapper(indexAnalyzers.getDefaultSearchAnalyzer(), p -> p.searchAnalyzer());
        this.searchQuoteAnalyzer = new MapperAnalyzerWrapper(indexAnalyzers.getDefaultSearchQuoteAnalyzer(), p -> p.searchQuoteAnalyzer());
        this.mapperRegistry = mapperRegistry;

        if (INDEX_MAPPER_DYNAMIC_SETTING.exists(indexSettings.getSettings()) &&
                indexSettings.getIndexVersionCreated().onOrAfter(Version.V_7_0_0_alpha1)) {
            throw new IllegalArgumentException("Setting " + INDEX_MAPPER_DYNAMIC_SETTING.getKey() + " was removed after version 6.0.0");
        }

        defaultMappingSource = "{\"_default_\":{}}";

        if (logger.isTraceEnabled()) {
            logger.trace("default mapping source[{}]", defaultMappingSource);
        }
    }

    public boolean hasNested() {
        return this.hasNested;
    }

    public IndexAnalyzers getIndexAnalyzers() {
        return this.indexAnalyzers;
    }

    public NamedAnalyzer getNamedAnalyzer(String analyzerName) {
        return this.indexAnalyzers.get(analyzerName);
    }

    public DocumentMapperParser documentMapperParser() {
        return this.documentParser;
    }

    /**
     * Parses the mappings (formatted as JSON) into a map
     */
    public static Map<String, Object> parseMapping(NamedXContentRegistry xContentRegistry, String mappingSource) throws Exception {
        try (XContentParser parser = XContentType.JSON.xContent()
                .createParser(xContentRegistry, LoggingDeprecationHandler.INSTANCE, mappingSource)) {
            return parser.map();
        }
    }

    /**
     * Update mapping by only merging the metadata that is different between received and stored entries
     */
    public boolean updateMapping(IndexMetaData indexMetaData) throws IOException {
        assert indexMetaData.getIndex().equals(index()) : "index mismatch: expected " + index() + " but was " + indexMetaData.getIndex();
        // go over and add the relevant mappings (or update them)
        Set<String> existingMappers = new HashSet<>();
        if (mapper != null) {
            existingMappers.add(mapper.type());
        }
        if (defaultMapper != null) {
            existingMappers.add(DEFAULT_MAPPING);
        }
        final Map<String, DocumentMapper> updatedEntries;
        try {
            // only update entries if needed
            updatedEntries = internalMerge(indexMetaData, MergeReason.MAPPING_RECOVERY, true);
        } catch (Exception e) {
            logger.warn(() -> new ParameterizedMessage("[{}] failed to apply mappings", index()), e);
            throw e;
        }

        boolean requireRefresh = false;

        for (DocumentMapper documentMapper : updatedEntries.values()) {
            String mappingType = documentMapper.type();
            CompressedXContent incomingMappingSource = indexMetaData.mapping(mappingType).source();

            String op = existingMappers.contains(mappingType) ? "updated" : "added";
            if (logger.isDebugEnabled() && incomingMappingSource.compressed().length < 512) {
                logger.debug("[{}] {} mapping [{}], source [{}]", index(), op, mappingType, incomingMappingSource.string());
            } else if (logger.isTraceEnabled()) {
                logger.trace("[{}] {} mapping [{}], source [{}]", index(), op, mappingType, incomingMappingSource.string());
            } else {
                logger.debug("[{}] {} mapping [{}] (source suppressed due to length, use TRACE level if needed)", index(), op, mappingType);
            }

            // refresh mapping can happen when the parsing/merging of the mapping from the metadata doesn't result in the same
            // mapping, in this case, we send to the master to refresh its own version of the mappings (to conform with the
            // merge version of it, which it does when refreshing the mappings), and warn log it.
            if (documentMapper(mappingType).mappingSource().equals(incomingMappingSource) == false) {
                logger.debug("[{}] parsed mapping [{}], and got different sources\noriginal:\n{}\nparsed:\n{}", index(), mappingType,
                    incomingMappingSource, documentMapper(mappingType).mappingSource());

                requireRefresh = true;
            }
        }

        return requireRefresh;
    }

    public void merge(Map<String, Map<String, Object>> mappings, MergeReason reason) {
        Map<String, CompressedXContent> mappingSourcesCompressed = new LinkedHashMap<>(mappings.size());
        for (Map.Entry<String, Map<String, Object>> entry : mappings.entrySet()) {
            try {
                mappingSourcesCompressed.put(entry.getKey(), new CompressedXContent(Strings.toString(XContentFactory.jsonBuilder().map(entry.getValue()))));
            } catch (Exception e) {
                throw new MapperParsingException("Failed to parse mapping [{}]: {}", e, entry.getKey(), e.getMessage());
            }
        }

        internalMerge(mappingSourcesCompressed, reason);
    }

    public void merge(IndexMetaData indexMetaData, MergeReason reason) {
        internalMerge(indexMetaData, reason, false);
    }

    public DocumentMapper merge(String type, CompressedXContent mappingSource, MergeReason reason) {
        return internalMerge(Collections.singletonMap(type, mappingSource), reason).get(type);
    }

    private synchronized Map<String, DocumentMapper> internalMerge(IndexMetaData indexMetaData, MergeReason reason, boolean onlyUpdateIfNeeded) {
        Map<String, CompressedXContent> map = new LinkedHashMap<>();
        for (ObjectCursor<MappingMetaData> cursor : indexMetaData.getMappings().values()) {
            MappingMetaData mappingMetaData = cursor.value;
            if (onlyUpdateIfNeeded) {
                DocumentMapper existingMapper = documentMapper(mappingMetaData.type());
                if (existingMapper == null || mappingMetaData.source().equals(existingMapper.mappingSource()) == false) {
                    map.put(mappingMetaData.type(), mappingMetaData.source());
                }
            } else {
                map.put(mappingMetaData.type(), mappingMetaData.source());
            }
        }
        return internalMerge(map, reason);
    }

    private synchronized Map<String, DocumentMapper> internalMerge(Map<String, CompressedXContent> mappings, MergeReason reason) {
        DocumentMapper defaultMapper = null;
        String defaultMappingSource = null;

        if (mappings.containsKey(DEFAULT_MAPPING)) {
            // verify we can parse it
            // NOTE: never apply the default here
            try {
                defaultMapper = documentParser.parse(DEFAULT_MAPPING, mappings.get(DEFAULT_MAPPING));
            } catch (Exception e) {
                throw new MapperParsingException("Failed to parse mapping [{}]: {}", e, DEFAULT_MAPPING, e.getMessage());
            }
            try {
                defaultMappingSource = mappings.get(DEFAULT_MAPPING).string();
            } catch (IOException e) {
                throw new ElasticsearchGenerationException("failed to un-compress", e);
            }
        }

        final String defaultMappingSourceOrLastStored;
        if (defaultMappingSource != null) {
            defaultMappingSourceOrLastStored = defaultMappingSource;
        } else {
            defaultMappingSourceOrLastStored = this.defaultMappingSource;
        }

        DocumentMapper documentMapper = null;
        for (Map.Entry<String, CompressedXContent> entry : mappings.entrySet()) {
            String type = entry.getKey();
            if (type.equals(DEFAULT_MAPPING)) {
                continue;
            }

            if (documentMapper != null) {
                throw new IllegalArgumentException("Cannot put multiple mappings: " + mappings.keySet());
            }

            final boolean applyDefault =
                // the default was already applied if we are recovering
                reason != MergeReason.MAPPING_RECOVERY
                    // only apply the default mapping if we don't have the type yet
                    && this.mapper == null;

            try {
                documentMapper =
                    documentParser.parse(type, entry.getValue(), applyDefault ? defaultMappingSourceOrLastStored : null);
            } catch (Exception e) {
                throw new MapperParsingException("Failed to parse mapping [{}]: {}", e, entry.getKey(), e.getMessage());
            }
        }

        return internalMerge(defaultMapper, defaultMappingSource, documentMapper, reason);
    }

    static void validateTypeName(String type) {
        if (type.length() == 0) {
            throw new InvalidTypeNameException("mapping type name is empty");
        }
        if (type.length() > 255) {
            throw new InvalidTypeNameException("mapping type name [" + type + "] is too long; limit is length 255 but was [" + type.length() + "]");
        }
        if (type.charAt(0) == '_' && SINGLE_MAPPING_NAME.equals(type) == false) {
            throw new InvalidTypeNameException("mapping type name [" + type + "] can't start with '_' unless it is called [" + SINGLE_MAPPING_NAME + "]");
        }
        if (type.contains("#")) {
            throw new InvalidTypeNameException("mapping type name [" + type + "] should not include '#' in it");
        }
        if (type.contains(",")) {
            throw new InvalidTypeNameException("mapping type name [" + type + "] should not include ',' in it");
        }
        if (type.charAt(0) == '.') {
            throw new IllegalArgumentException("mapping type name [" + type + "] must not start with a '.'");
        }
    }

    private synchronized Map<String, DocumentMapper> internalMerge(@Nullable DocumentMapper defaultMapper, @Nullable String defaultMappingSource,
                                                                   DocumentMapper mapper, MergeReason reason) {
        boolean hasNested = this.hasNested;
        Map<String, ObjectMapper> fullPathObjectMappers = this.fullPathObjectMappers;
        FieldTypeLookup fieldTypes = this.fieldTypes;

        Map<String, DocumentMapper> results = new LinkedHashMap<>(2);

        if (defaultMapper != null) {
            if (indexSettings.getIndexVersionCreated().onOrAfter(Version.V_7_0_0_alpha1)) {
                throw new IllegalArgumentException("The [default] mapping cannot be updated on index [" + index().getName() +
                        "]: defaults mappings are not useful anymore now that indices can have at most one type.");
            } else if (reason == MergeReason.MAPPING_UPDATE) { // only log in case of explicit mapping updates
                DEPRECATION_LOGGER.deprecated("[_default_] mapping is deprecated since it is not useful anymore now that indexes " +
                        "cannot have more than one type");
            }
            assert defaultMapper.type().equals(DEFAULT_MAPPING);
            results.put(DEFAULT_MAPPING, defaultMapper);
        }

        {
            if (mapper != null && this.mapper != null && Objects.equals(this.mapper.type(), mapper.type()) == false) {
                throw new IllegalArgumentException(
                        "Rejecting mapping update to [" + index().getName() + "] as the final mapping would have more than 1 type: " + Arrays.asList(this.mapper.type(), mapper.type()));
            }
        }

        DocumentMapper newMapper = null;
        if (mapper != null) {
            // check naming
            validateTypeName(mapper.type());

            // compute the merged DocumentMapper
            DocumentMapper oldMapper = this.mapper;
            if (oldMapper != null) {
                newMapper = oldMapper.merge(mapper.mapping());
            } else {
                newMapper = mapper;
            }

            // check basic sanity of the new mapping
            List<ObjectMapper> objectMappers = new ArrayList<>();
            List<FieldMapper> fieldMappers = new ArrayList<>();
            Collections.addAll(fieldMappers, newMapper.mapping().metadataMappers);
            MapperUtils.collect(newMapper.mapping().root(), objectMappers, fieldMappers);
            checkFieldUniqueness(newMapper.type(), objectMappers, fieldMappers, fullPathObjectMappers, fieldTypes);
            checkObjectsCompatibility(objectMappers, fullPathObjectMappers);
            checkPartitionedIndexConstraints(newMapper);

            // update lookup data-structures
            // this will in particular make sure that the merged fields are compatible with other types
            fieldTypes = fieldTypes.copyAndAddAll(newMapper.type(), fieldMappers);

            for (ObjectMapper objectMapper : objectMappers) {
                if (fullPathObjectMappers == this.fullPathObjectMappers) {
                    // first time through the loops
                    fullPathObjectMappers = new HashMap<>(this.fullPathObjectMappers);
                }
                fullPathObjectMappers.put(objectMapper.fullPath(), objectMapper);

                if (objectMapper.nested().isNested()) {
                    hasNested = true;
                }
            }

            validateCopyTo(fieldMappers, fullPathObjectMappers, fieldTypes);

            if (reason == MergeReason.MAPPING_UPDATE) {
                // this check will only be performed on the master node when there is
                // a call to the update mapping API. For all other cases like
                // the master node restoring mappings from disk or data nodes
                // deserializing cluster state that was sent by the master node,
                // this check will be skipped.
                checkTotalFieldsLimit(objectMappers.size() + fieldMappers.size());
            }

            results.put(newMapper.type(), newMapper);
        }

        if (reason == MergeReason.MAPPING_UPDATE) {
            // this check will only be performed on the master node when there is
            // a call to the update mapping API. For all other cases like
            // the master node restoring mappings from disk or data nodes
            // deserializing cluster state that was sent by the master node,
            // this check will be skipped.
            checkNestedFieldsLimit(fullPathObjectMappers);
            checkDepthLimit(fullPathObjectMappers.keySet());
        }
        checkIndexSortCompatibility(indexSettings.getIndexSortConfig(), hasNested);

        if (newMapper != null) {
            DocumentMapper updatedDocumentMapper = newMapper.updateFieldType(fieldTypes.fullNameToFieldType);
            if (updatedDocumentMapper != newMapper) {
                // update both mappers and result
                newMapper = updatedDocumentMapper;
                results.put(updatedDocumentMapper.type(), updatedDocumentMapper);
            }
        }

        // make structures immutable
        results = Collections.unmodifiableMap(results);

        // only need to immutably rewrap these if the previous reference was changed.
        // if not then they are already implicitly immutable.
        if (fullPathObjectMappers != this.fullPathObjectMappers) {
            fullPathObjectMappers = Collections.unmodifiableMap(fullPathObjectMappers);
        }

        // commit the change
        if (defaultMappingSource != null) {
            this.defaultMappingSource = defaultMappingSource;
        }
        if (newMapper != null) {
            this.mapper = newMapper;
        }
        this.defaultMapper = defaultMapper;
        this.fieldTypes = fieldTypes;
        this.hasNested = hasNested;
        this.fullPathObjectMappers = fullPathObjectMappers;

        assert assertMappersShareSameFieldType();
        assert results.values().stream().allMatch(this::assertSerialization);

        return results;
    }

    private boolean assertMappersShareSameFieldType() {
        if (mapper != null) {
            List<FieldMapper> fieldMappers = new ArrayList<>();
            Collections.addAll(fieldMappers, mapper.mapping().metadataMappers);
            MapperUtils.collect(mapper.root(), new ArrayList<>(), fieldMappers);
            for (FieldMapper fieldMapper : fieldMappers) {
                assert fieldMapper.fieldType() == fieldTypes.get(fieldMapper.name()) : fieldMapper.name();
            }
        }
        return true;
    }

    private boolean assertSerialization(DocumentMapper mapper) {
        // capture the source now, it may change due to concurrent parsing
        final CompressedXContent mappingSource = mapper.mappingSource();
        DocumentMapper newMapper = parse(mapper.type(), mappingSource, false);

        if (newMapper.mappingSource().equals(mappingSource) == false) {
            throw new IllegalStateException("DocumentMapper serialization result is different from source. \n--> Source ["
                + mappingSource + "]\n--> Result ["
                + newMapper.mappingSource() + "]");
        }
        return true;
    }

    private static void checkFieldUniqueness(String type, Collection<ObjectMapper> objectMappers, Collection<FieldMapper> fieldMappers,
                                             Map<String, ObjectMapper> fullPathObjectMappers, FieldTypeLookup fieldTypes) {

        // first check within mapping
        final Set<String> objectFullNames = new HashSet<>();
        for (ObjectMapper objectMapper : objectMappers) {
            final String fullPath = objectMapper.fullPath();
            if (objectFullNames.add(fullPath) == false) {
                throw new IllegalArgumentException("Object mapper [" + fullPath + "] is defined twice in mapping for type [" + type + "]");
            }
        }

        final Set<String> fieldNames = new HashSet<>();
        for (FieldMapper fieldMapper : fieldMappers) {
            final String name = fieldMapper.name();
            if (objectFullNames.contains(name)) {
                throw new IllegalArgumentException("Field [" + name + "] is defined both as an object and a field in [" + type + "]");
            } else if (fieldNames.add(name) == false) {
                throw new IllegalArgumentException("Field [" + name + "] is defined twice in [" + type + "]");
            }
        }

        // then check other types
        for (String fieldName : fieldNames) {
            if (fullPathObjectMappers.containsKey(fieldName)) {
                throw new IllegalArgumentException("[" + fieldName + "] is defined as a field in mapping [" + type
                        + "] but this name is already used for an object in other types");
            }
        }

        for (String objectPath : objectFullNames) {
            if (fieldTypes.get(objectPath) != null) {
                throw new IllegalArgumentException("[" + objectPath + "] is defined as an object in mapping [" + type
                        + "] but this name is already used for a field in other types");
            }
        }
    }

    private static void checkObjectsCompatibility(Collection<ObjectMapper> objectMappers,
                                                  Map<String, ObjectMapper> fullPathObjectMappers) {
        for (ObjectMapper newObjectMapper : objectMappers) {
            ObjectMapper existingObjectMapper = fullPathObjectMappers.get(newObjectMapper.fullPath());
            if (existingObjectMapper != null) {
                // simulate a merge and ignore the result, we are just interested
                // in exceptions here
                existingObjectMapper.merge(newObjectMapper);
            }
        }
    }

    private void checkNestedFieldsLimit(Map<String, ObjectMapper> fullPathObjectMappers) {
        long allowedNestedFields = indexSettings.getValue(INDEX_MAPPING_NESTED_FIELDS_LIMIT_SETTING);
        long actualNestedFields = 0;
        for (ObjectMapper objectMapper : fullPathObjectMappers.values()) {
            if (objectMapper.nested().isNested()) {
                actualNestedFields++;
            }
        }
        if (actualNestedFields > allowedNestedFields) {
            throw new IllegalArgumentException("Limit of nested fields [" + allowedNestedFields + "] in index [" + index().getName() + "] has been exceeded");
        }
    }

    private void checkTotalFieldsLimit(long totalMappers) {
        long allowedTotalFields = indexSettings.getValue(INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING);
        if (allowedTotalFields < totalMappers) {
            throw new IllegalArgumentException("Limit of total fields [" + allowedTotalFields + "] in index [" + index().getName() + "] has been exceeded");
        }
    }

    private void checkDepthLimit(Collection<String> objectPaths) {
        final long maxDepth = indexSettings.getValue(INDEX_MAPPING_DEPTH_LIMIT_SETTING);
        for (String objectPath : objectPaths) {
            checkDepthLimit(objectPath, maxDepth);
        }
    }

    private void checkDepthLimit(String objectPath, long maxDepth) {
        int numDots = 0;
        for (int i = 0; i < objectPath.length(); ++i) {
            if (objectPath.charAt(i) == '.') {
                numDots += 1;
            }
        }
        final int depth = numDots + 2;
        if (depth > maxDepth) {
            throw new IllegalArgumentException("Limit of mapping depth [" + maxDepth + "] in index [" + index().getName()
                    + "] has been exceeded due to object field [" + objectPath + "]");
        }
    }

    private void checkPartitionedIndexConstraints(DocumentMapper newMapper) {
        if (indexSettings.getIndexMetaData().isRoutingPartitionedIndex()) {
            if (!newMapper.routingFieldMapper().required()) {
                throw new IllegalArgumentException("mapping type [" + newMapper.type() + "] must have routing "
                        + "required for partitioned index [" + indexSettings.getIndex().getName() + "]");
            }
        }
    }

    private static void checkIndexSortCompatibility(IndexSortConfig sortConfig, boolean hasNested) {
        if (sortConfig.hasIndexSort() && hasNested) {
            throw new IllegalArgumentException("cannot have nested fields when index sort is activated");
        }
    }

    private static void validateCopyTo(List<FieldMapper> fieldMappers, Map<String, ObjectMapper> fullPathObjectMappers,
            FieldTypeLookup fieldTypes) {
        for (FieldMapper mapper : fieldMappers) {
            if (mapper.copyTo() != null && mapper.copyTo().copyToFields().isEmpty() == false) {
                String sourceParent = parentObject(mapper.name());
                if (sourceParent != null && fieldTypes.get(sourceParent) != null) {
                    throw new IllegalArgumentException("[copy_to] may not be used to copy from a multi-field: [" + mapper.name() + "]");
                }

                final String sourceScope = getNestedScope(mapper.name(), fullPathObjectMappers);
                for (String copyTo : mapper.copyTo().copyToFields()) {
                    String copyToParent = parentObject(copyTo);
                    if (copyToParent != null && fieldTypes.get(copyToParent) != null) {
                        throw new IllegalArgumentException("[copy_to] may not be used to copy to a multi-field: [" + copyTo + "]");
                    }

                    if (fullPathObjectMappers.containsKey(copyTo)) {
                        throw new IllegalArgumentException("Cannot copy to field [" + copyTo + "] since it is mapped as an object");
                    }

                    final String targetScope = getNestedScope(copyTo, fullPathObjectMappers);
                    checkNestedScopeCompatibility(sourceScope, targetScope);
                }
            }
        }
    }

    private static String getNestedScope(String path, Map<String, ObjectMapper> fullPathObjectMappers) {
        for (String parentPath = parentObject(path); parentPath != null; parentPath = parentObject(parentPath)) {
            ObjectMapper objectMapper = fullPathObjectMappers.get(parentPath);
            if (objectMapper != null && objectMapper.nested().isNested()) {
                return parentPath;
            }
        }
        return null;
    }

    private static void checkNestedScopeCompatibility(String source, String target) {
        boolean targetIsParentOfSource;
        if (source == null || target == null) {
            targetIsParentOfSource = target == null;
        } else {
            targetIsParentOfSource = source.equals(target) || source.startsWith(target + ".");
        }
        if (targetIsParentOfSource == false) {
            throw new IllegalArgumentException(
                    "Illegal combination of [copy_to] and [nested] mappings: [copy_to] may only copy data to the current nested " +
                            "document or any of its parents, however one [copy_to] directive is trying to copy data from nested object [" +
                            source + "] to [" + target + "]");
        }
    }

    private static String parentObject(String field) {
        int lastDot = field.lastIndexOf('.');
        if (lastDot == -1) {
            return null;
        }
        return field.substring(0, lastDot);
    }

    public DocumentMapper parse(String mappingType, CompressedXContent mappingSource, boolean applyDefault) throws MapperParsingException {
        return documentParser.parse(mappingType, mappingSource, applyDefault ? defaultMappingSource : null);
    }

    /**
     * Return the document mapper, or {@code null} if no mapping has been put yet.
     */
    public DocumentMapper documentMapper() {
        return mapper;
    }

    /**
     * Return the {@link DocumentMapper} for the given type. By using the special
     * {@value #DEFAULT_MAPPING} type, you can get a {@link DocumentMapper} for
     * the default mapping.
     */
    public DocumentMapper documentMapper(String type) {
        if (mapper != null && type.equals(mapper.type())) {
            return mapper;
        }
        if (DEFAULT_MAPPING.equals(type)) {
            return defaultMapper;
        }
        return null;
    }

    /**
     * Returns the document mapper created, including a mapping update if the
     * type has been dynamically created.
     */
    public DocumentMapperForType documentMapperWithAutoCreate(String type) {
        DocumentMapper mapper = documentMapper(type);
        if (mapper != null) {
            return new DocumentMapperForType(mapper, null);
        }
        mapper = parse(type, null, true);
        return new DocumentMapperForType(mapper, mapper.mapping());
    }

    /**
     * Returns the {@link MappedFieldType} for the give fullName.
     *
     * If multiple types have fields with the same full name, the first is returned.
     */
    public MappedFieldType fullName(String fullName) {
        return fieldTypes.get(fullName);
    }

    /**
     * Returns all the fields that match the given pattern. If the pattern is prefixed with a type
     * then the fields will be returned with a type prefix.
     */
    public Collection<String> simpleMatchToIndexNames(String pattern) {
        if (Regex.isSimpleMatchPattern(pattern) == false) {
            // no wildcards
            return Collections.singletonList(pattern);
        }
        return fieldTypes.simpleMatchToFullName(pattern);
    }

    public ObjectMapper getObjectMapper(String name) {
        return fullPathObjectMappers.get(name);
    }

    /**
     * Given a type (eg. long, string, ...), return an anonymous field mapper that can be used for search operations.
     */
    public MappedFieldType unmappedFieldType(String type) {
        if (type.equals("string")) {
            deprecationLogger.deprecated("[unmapped_type:string] should be replaced with [unmapped_type:keyword]");
            type = "keyword";
        }
        MappedFieldType fieldType = unmappedFieldTypes.get(type);
        if (fieldType == null) {
            final Mapper.TypeParser.ParserContext parserContext = documentMapperParser().parserContext(type);
            Mapper.TypeParser typeParser = parserContext.typeParser(type);
            if (typeParser == null) {
                throw new IllegalArgumentException("No mapper found for type [" + type + "]");
            }
            final Mapper.Builder<?, ?> builder = typeParser.parse("__anonymous_" + type, emptyMap(), parserContext);
            final BuilderContext builderContext = new BuilderContext(indexSettings.getSettings(), new ContentPath(1));
            fieldType = ((FieldMapper)builder.build(builderContext)).fieldType();

            // There is no need to synchronize writes here. In the case of concurrent access, we could just
            // compute some mappers several times, which is not a big deal
            Map<String, MappedFieldType> newUnmappedFieldTypes = new HashMap<>(unmappedFieldTypes);
            newUnmappedFieldTypes.put(type, fieldType);
            unmappedFieldTypes = unmodifiableMap(newUnmappedFieldTypes);
        }
        return fieldType;
    }

    public Analyzer indexAnalyzer() {
        return this.indexAnalyzer;
    }

    public Analyzer searchAnalyzer() {
        return this.searchAnalyzer;
    }

    public Analyzer searchQuoteAnalyzer() {
        return this.searchQuoteAnalyzer;
    }

    @Override
    public void close() throws IOException {
        indexAnalyzers.close();
    }

    /**
     * @return Whether a field is a metadata field.
     */
    public static boolean isMetadataField(String fieldName) {
        return META_FIELDS.contains(fieldName);
    }

    public static String[] getAllMetaFields() {
        return META_FIELDS.toArray(String.class);
    }

    /** An analyzer wrapper that can lookup fields within the index mappings */
    final class MapperAnalyzerWrapper extends DelegatingAnalyzerWrapper {

        private final Analyzer defaultAnalyzer;
        private final Function<MappedFieldType, Analyzer> extractAnalyzer;

        MapperAnalyzerWrapper(Analyzer defaultAnalyzer, Function<MappedFieldType, Analyzer> extractAnalyzer) {
            super(Analyzer.PER_FIELD_REUSE_STRATEGY);
            this.defaultAnalyzer = defaultAnalyzer;
            this.extractAnalyzer = extractAnalyzer;
        }

        @Override
        protected Analyzer getWrappedAnalyzer(String fieldName) {
            MappedFieldType fieldType = fullName(fieldName);
            if (fieldType != null) {
                Analyzer analyzer = extractAnalyzer.apply(fieldType);
                if (analyzer != null) {
                    return analyzer;
                }
            }
            return defaultAnalyzer;
        }
    }

    /** Return a term that uniquely identifies the document, or {@code null} if the type is not allowed. */
    public Term createUidTerm(String type, String id) {
        if (mapper == null || mapper.type().equals(type) == false) {
            return null;
        }
        return new Term(IdFieldMapper.NAME, Uid.encodeId(id));
    }
}
