/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.Assertions;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.MappingMetadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.time.DateFormatter;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.AbstractIndexComponent;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.index.analysis.CharFilterFactory;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.analysis.ReloadableCustomAnalyzer;
import org.elasticsearch.index.analysis.TokenFilterFactory;
import org.elasticsearch.index.analysis.TokenizerFactory;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.similarity.SimilarityService;
import org.elasticsearch.indices.IndicesModule;
import org.elasticsearch.indices.mapper.MapperRegistry;
import org.elasticsearch.script.ScriptService;

import java.io.Closeable;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.BooleanSupplier;
import java.util.function.Function;
import java.util.function.Supplier;

public class MapperService extends AbstractIndexComponent implements Closeable {

    /**
     * The reason why a mapping is being merged.
     */
    public enum MergeReason {
        /**
         * Pre-flight check before sending a mapping update to the master
         */
        MAPPING_UPDATE_PREFLIGHT,
        /**
         * Create or update a mapping.
         */
        MAPPING_UPDATE,
        /**
         * Merge mappings from a composable index template.
         */
        INDEX_TEMPLATE,
        /**
         * Recovery of an existing mapping, for instance because of a restart,
         * if a shard was moved to a different node or for administrative
         * purposes.
         */
        MAPPING_RECOVERY
    }

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
    public static final Setting<Long> INDEX_MAPPING_FIELD_NAME_LENGTH_LIMIT_SETTING =
        Setting.longSetting("index.mapping.field_name_length.limit", Long.MAX_VALUE, 1L, Property.Dynamic, Property.IndexScope);

    private final IndexAnalyzers indexAnalyzers;
    private final DocumentMapperParser documentMapperParser;
    private final DocumentParser documentParser;
    private final Version indexVersionCreated;
    private final MapperRegistry mapperRegistry;
    private final Supplier<Mapper.TypeParser.ParserContext> parserContextSupplier;

    private volatile DocumentMapper mapper;

    public MapperService(IndexSettings indexSettings, IndexAnalyzers indexAnalyzers, NamedXContentRegistry xContentRegistry,
                         SimilarityService similarityService, MapperRegistry mapperRegistry,
                         Supplier<SearchExecutionContext> searchExecutionContextSupplier, BooleanSupplier idFieldDataEnabled,
                         ScriptService scriptService) {
        super(indexSettings);
        this.indexVersionCreated = indexSettings.getIndexVersionCreated();
        this.indexAnalyzers = indexAnalyzers;
        this.mapperRegistry = mapperRegistry;
        Function<DateFormatter, Mapper.TypeParser.ParserContext> parserContextFunction =
            dateFormatter -> new Mapper.TypeParser.ParserContext(similarityService::getSimilarity, mapperRegistry.getMapperParsers()::get,
                mapperRegistry.getRuntimeFieldTypeParsers()::get, indexVersionCreated, searchExecutionContextSupplier, dateFormatter,
                scriptService, indexAnalyzers, indexSettings, idFieldDataEnabled, mapperRegistry.getDynamicRuntimeFieldsBuilder() != null);
        this.documentParser = new DocumentParser(xContentRegistry, parserContextFunction, mapperRegistry.getDynamicRuntimeFieldsBuilder());
        Map<String, MetadataFieldMapper.TypeParser> metadataMapperParsers =
            mapperRegistry.getMetadataMapperParsers(indexSettings.getIndexVersionCreated());
        this.parserContextSupplier = () -> parserContextFunction.apply(null);
        this.documentMapperParser = new DocumentMapperParser(indexSettings, indexAnalyzers, this::resolveDocumentType, documentParser,
            this::getMetadataMappers, parserContextSupplier, metadataMapperParsers);
    }

    public boolean hasNested() {
        return mappingLookup().hasNested();
    }

    public IndexAnalyzers getIndexAnalyzers() {
        return this.indexAnalyzers;
    }

    public NamedAnalyzer getNamedAnalyzer(String analyzerName) {
        return this.indexAnalyzers.get(analyzerName);
    }

    public Mapper.TypeParser.ParserContext parserContext() {
        return parserContextSupplier.get();
    }

    DocumentParser documentParser() {
        return this.documentParser;
    }

    Map<Class<? extends MetadataFieldMapper>, MetadataFieldMapper> getMetadataMappers() {
        final DocumentMapper existingMapper = mapper;
        final Map<String, MetadataFieldMapper.TypeParser> metadataMapperParsers =
            mapperRegistry.getMetadataMapperParsers(indexSettings.getIndexVersionCreated());
        Map<Class<? extends MetadataFieldMapper>, MetadataFieldMapper> metadataMappers = new LinkedHashMap<>();
        if (existingMapper == null) {
            for (MetadataFieldMapper.TypeParser parser : metadataMapperParsers.values()) {
                MetadataFieldMapper metadataFieldMapper = parser.getDefault(parserContext());
                metadataMappers.put(metadataFieldMapper.getClass(), metadataFieldMapper);
            }

        } else {
            metadataMappers.putAll(existingMapper.mapping().metadataMappersMap);
        }
        return metadataMappers;
    }

    /**
     * Parses the mappings (formatted as JSON) into a map
     */
    public static Map<String, Object> parseMapping(NamedXContentRegistry xContentRegistry, String mappingSource) throws IOException {
        try (XContentParser parser = XContentType.JSON.xContent()
                .createParser(xContentRegistry, LoggingDeprecationHandler.INSTANCE, mappingSource)) {
            return parser.map();
        }
    }

    /**
     * Update mapping by only merging the metadata that is different between received and stored entries
     */
    public boolean updateMapping(final IndexMetadata currentIndexMetadata, final IndexMetadata newIndexMetadata) throws IOException {
        assert newIndexMetadata.getIndex().equals(index()) : "index mismatch: expected " + index()
            + " but was " + newIndexMetadata.getIndex();

        if (currentIndexMetadata != null && currentIndexMetadata.getMappingVersion() == newIndexMetadata.getMappingVersion()) {
            assertMappingVersion(currentIndexMetadata, newIndexMetadata, this.mapper);
            return false;
        }

        final DocumentMapper updatedMapper;
        try {
            updatedMapper = internalMerge(newIndexMetadata, MergeReason.MAPPING_RECOVERY);
        } catch (Exception e) {
            logger.warn(() -> new ParameterizedMessage("[{}] failed to apply mappings", index()), e);
            throw e;
        }

        if (updatedMapper == null) {
            return false;
        }

        boolean requireRefresh = false;

        assertMappingVersion(currentIndexMetadata, newIndexMetadata, updatedMapper);

        MappingMetadata mappingMetadata = newIndexMetadata.mapping();
        CompressedXContent incomingMappingSource = mappingMetadata.source();

        String op = mapper != null ? "updated" : "added";
        if (logger.isDebugEnabled() && incomingMappingSource.compressed().length < 512) {
            logger.debug("[{}] {} mapping, source [{}]", index(), op, incomingMappingSource.string());
        } else if (logger.isTraceEnabled()) {
            logger.trace("[{}] {} mapping, source [{}]", index(), op, incomingMappingSource.string());
        } else {
            logger.debug("[{}] {} mapping (source suppressed due to length, use TRACE level if needed)",
                index(), op);
        }

        // refresh mapping can happen when the parsing/merging of the mapping from the metadata doesn't result in the same
        // mapping, in this case, we send to the master to refresh its own version of the mappings (to conform with the
        // merge version of it, which it does when refreshing the mappings), and warn log it.
        if (documentMapper().mappingSource().equals(incomingMappingSource) == false) {
            logger.debug("[{}] parsed mapping, and got different sources\noriginal:\n{}\nparsed:\n{}",
                index(), incomingMappingSource, documentMapper().mappingSource());

            requireRefresh = true;
        }
        return requireRefresh;
    }

    private void assertMappingVersion(
            final IndexMetadata currentIndexMetadata,
            final IndexMetadata newIndexMetadata,
            final DocumentMapper updatedMapper) throws IOException {
        if (Assertions.ENABLED && currentIndexMetadata != null) {
            if (currentIndexMetadata.getMappingVersion() == newIndexMetadata.getMappingVersion()) {
                // if the mapping version is unchanged, then there should not be any updates and all mappings should be the same
                assert updatedMapper == mapper;

                MappingMetadata mapping = newIndexMetadata.mapping();
                if (mapping != null) {
                    final CompressedXContent currentSource = currentIndexMetadata.mapping().source();
                    final CompressedXContent newSource = mapping.source();
                    assert currentSource.equals(newSource) :
                        "expected current mapping [" + currentSource + "] for type [" + mapping.type() + "] "
                            + "to be the same as new mapping [" + newSource + "]";
                    final CompressedXContent mapperSource = new CompressedXContent(Strings.toString(mapper));
                    assert currentSource.equals(mapperSource) :
                        "expected current mapping [" + currentSource + "] for type [" + mapping.type() + "] "
                            + "to be the same as new mapping [" + mapperSource + "]";
                }

            } else {
                // the mapping version should increase, there should be updates, and the mapping should be different
                final long currentMappingVersion = currentIndexMetadata.getMappingVersion();
                final long newMappingVersion = newIndexMetadata.getMappingVersion();
                assert currentMappingVersion < newMappingVersion :
                    "expected current mapping version [" + currentMappingVersion + "] "
                        + "to be less than new mapping version [" + newMappingVersion + "]";
                assert updatedMapper != null;
                final MappingMetadata currentMapping = currentIndexMetadata.mapping();
                if (currentMapping != null) {
                    final CompressedXContent currentSource = currentMapping.source();
                    final CompressedXContent newSource = updatedMapper.mappingSource();
                    assert currentSource.equals(newSource) == false :
                        "expected current mapping [" + currentSource + "] to be different than new mapping [" + newSource + "]";
                }
            }
        }
    }

    public void merge(String type, Map<String, Object> mappings, MergeReason reason) throws IOException {
        CompressedXContent content = new CompressedXContent(Strings.toString(XContentFactory.jsonBuilder().map(mappings)));
        internalMerge(type, content, reason);
    }

    public void merge(IndexMetadata indexMetadata, MergeReason reason) {
        internalMerge(indexMetadata, reason);
    }

    public DocumentMapper merge(String type, CompressedXContent mappingSource, MergeReason reason) {
        return internalMerge(type, mappingSource, reason);
    }

    private synchronized DocumentMapper internalMerge(IndexMetadata indexMetadata, MergeReason reason) {
        assert reason != MergeReason.MAPPING_UPDATE_PREFLIGHT;
        MappingMetadata mappingMetadata = indexMetadata.mapping();
        if (mappingMetadata != null) {
            return internalMerge(mappingMetadata.type(), mappingMetadata.source(), reason);
        }
        return null;
    }

    private synchronized DocumentMapper internalMerge(String type, CompressedXContent mappings, MergeReason reason) {

        DocumentMapper documentMapper;

        try {
            documentMapper = documentMapperParser.parse(type, mappings);
        } catch (Exception e) {
            throw new MapperParsingException("Failed to parse mapping: {}", e, e.getMessage());
        }

        return internalMerge(documentMapper, reason);
    }

    private synchronized DocumentMapper internalMerge(DocumentMapper mapper, MergeReason reason) {

        assert mapper != null;

        // compute the merged DocumentMapper
        DocumentMapper oldMapper = this.mapper;
        DocumentMapper newMapper;
        if (oldMapper != null) {
            newMapper = oldMapper.merge(mapper.mapping(), reason);
        } else {
            newMapper = mapper;
        }
        newMapper.root().fixRedundantIncludes();
        newMapper.validate(indexSettings, reason != MergeReason.MAPPING_RECOVERY);

        if (reason == MergeReason.MAPPING_UPDATE_PREFLIGHT) {
            return newMapper;
        }

        // commit the change
        this.mapper = newMapper;
        assert assertSerialization(newMapper);

        return newMapper;
    }

    private boolean assertSerialization(DocumentMapper mapper) {
        // capture the source now, it may change due to concurrent parsing
        final CompressedXContent mappingSource = mapper.mappingSource();
        DocumentMapper newMapper = parse(mapper.type(), mappingSource);

        if (newMapper.mappingSource().equals(mappingSource) == false) {
            throw new IllegalStateException("DocumentMapper serialization result is different from source. \n--> Source ["
                + mappingSource + "]\n--> Result ["
                + newMapper.mappingSource() + "]");
        }
        return true;
    }

    public DocumentMapper parse(String mappingType, CompressedXContent mappingSource) throws MapperParsingException {
        return documentMapperParser.parse(mappingType, mappingSource);
    }

    /**
     * Return the document mapper, or {@code null} if no mapping has been put yet.
     */
    public DocumentMapper documentMapper() {
        return mapper;
    }

    /**
     * Returns {@code true} if the given {@code mappingSource} includes a type
     * as a top-level object.
     */
    public static boolean isMappingSourceTyped(String type, Map<String, Object> mapping) {
        return mapping.size() == 1 && mapping.keySet().iterator().next().equals(type);
    }

    /**
     * Resolves a type from a mapping-related request into the type that should be used when
     * merging and updating mappings.
     *
     * If the special `_doc` type is provided, then we replace it with the actual type that is
     * being used in the mappings. This allows typeless APIs such as 'index' or 'put mappings'
     * to work against indices with a custom type name.
     */
    private String resolveDocumentType(String type) {
        if (MapperService.SINGLE_MAPPING_NAME.equals(type)) {
            if (mapper != null) {
                return mapper.type();
            }
        }
        return type;
    }

    /**
     * Returns the document mapper for this MapperService.  If no mapper exists,
     * creates one and returns that.
     */
    public DocumentMapperForType documentMapperWithAutoCreate() {
        DocumentMapper mapper = documentMapper();
        if (mapper != null) {
            return new DocumentMapperForType(mapper, null);
        }
        mapper = parse(SINGLE_MAPPING_NAME, null);
        return new DocumentMapperForType(mapper, mapper.mapping());
    }

    /**
     * Given the full name of a field, returns its {@link MappedFieldType}.
     */
    public MappedFieldType fieldType(String fullName) {
        return mappingLookup().fieldTypes().get(fullName);
    }

    /**
     * Returns all the fields that match the given pattern. If the pattern is prefixed with a type
     * then the fields will be returned with a type prefix.
     */
    public Set<String> simpleMatchToFullName(String pattern) {
        return mappingLookup().simpleMatchToFullName(pattern);
    }

    /**
     * {@code volatile} read a (mostly) immutable snapshot current mapping.
     */
    public MappingLookup mappingLookup() {
        DocumentMapper mapper = this.mapper;
        return mapper == null ? MappingLookup.EMPTY : mapper.mappers();
    }

    /**
     * Returns all mapped field types.
     */
    public Iterable<MappedFieldType> getEagerGlobalOrdinalsFields() {
        return this.mapper == null ? Collections.emptySet() :
            this.mapper.mappers().fieldTypes().filter(MappedFieldType::eagerGlobalOrdinals);
    }

    public ObjectMapper getObjectMapper(String name) {
        return this.mapper == null ? null : this.mapper.mappers().objectMappers().get(name);
    }

    /**
     * Return the index-time analyzer associated with a particular field
     * @param field                     the field name
     * @param unindexedFieldAnalyzer    a function to return an Analyzer for a field with no
     *                                  directly associated index-time analyzer
     */
    public NamedAnalyzer indexAnalyzer(String field, Function<String, NamedAnalyzer> unindexedFieldAnalyzer) {
        return mappingLookup().indexAnalyzer(field, unindexedFieldAnalyzer);
    }

    @Override
    public void close() throws IOException {
        indexAnalyzers.close();
    }

    /**
     * @return Whether a field is a metadata field
     * Deserialization of SearchHit objects sent from pre 7.8 nodes and GetResults objects sent from pre 7.3 nodes,
     * uses this method to divide fields into meta and document fields.
     * TODO: remove in v 9.0
     * @deprecated  Use an instance method isMetadataField instead
     */
    @Deprecated
    public static boolean isMetadataFieldStatic(String fieldName) {
        if (IndicesModule.getBuiltInMetadataFields().contains(fieldName)) {
            return true;
        }
        // if a node had Size Plugin installed, _size field should also be considered a meta-field
        return fieldName.equals("_size");
    }

    /**
     * @return Whether a field is a metadata field.
     * this method considers all mapper plugins
     */
    public boolean isMetadataField(String field) {
        return mapperRegistry.getMetadataMapperParsers(indexVersionCreated).containsKey(field);
    }

    public synchronized List<String> reloadSearchAnalyzers(AnalysisRegistry registry) throws IOException {
        logger.info("reloading search analyzers");
        // refresh indexAnalyzers and search analyzers
        final Map<String, TokenizerFactory> tokenizerFactories = registry.buildTokenizerFactories(indexSettings);
        final Map<String, CharFilterFactory> charFilterFactories = registry.buildCharFilterFactories(indexSettings);
        final Map<String, TokenFilterFactory> tokenFilterFactories = registry.buildTokenFilterFactories(indexSettings);
        final Map<String, Settings> settings = indexSettings.getSettings().getGroups("index.analysis.analyzer");
        final List<String> reloadedAnalyzers = new ArrayList<>();
        for (NamedAnalyzer namedAnalyzer : indexAnalyzers.getAnalyzers().values()) {
            if (namedAnalyzer.analyzer() instanceof ReloadableCustomAnalyzer) {
                ReloadableCustomAnalyzer analyzer = (ReloadableCustomAnalyzer) namedAnalyzer.analyzer();
                String analyzerName = namedAnalyzer.name();
                Settings analyzerSettings = settings.get(analyzerName);
                analyzer.reload(analyzerName, analyzerSettings, tokenizerFactories, charFilterFactories, tokenFilterFactories);
                reloadedAnalyzers.add(analyzerName);
            }
        }
        // TODO this should bust the cache somehow. Tracked in https://github.com/elastic/elasticsearch/issues/66722
        return reloadedAnalyzers;
    }
}
