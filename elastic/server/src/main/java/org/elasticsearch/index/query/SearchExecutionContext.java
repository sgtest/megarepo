/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.query;

import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.DelegatingAnalyzerWrapper;
import org.apache.lucene.index.FieldInfo;
import org.apache.lucene.index.FieldInfos;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.join.BitSetProducer;
import org.apache.lucene.search.similarities.Similarity;
import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.common.ParsingException;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.lucene.search.Queries;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.core.CheckedFunction;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.IndexSortConfig;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.cache.bitset.BitsetFilterCache;
import org.elasticsearch.index.fielddata.FieldDataContext;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.MappedFieldType.FielddataOperation;
import org.elasticsearch.index.mapper.Mapper;
import org.elasticsearch.index.mapper.MapperBuilderContext;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.MappingLookup;
import org.elasticsearch.index.mapper.MappingParserContext;
import org.elasticsearch.index.mapper.NestedLookup;
import org.elasticsearch.index.mapper.ParsedDocument;
import org.elasticsearch.index.mapper.RuntimeField;
import org.elasticsearch.index.mapper.SourceLoader;
import org.elasticsearch.index.mapper.SourceToParse;
import org.elasticsearch.index.mapper.TextFieldMapper;
import org.elasticsearch.index.query.support.NestedScope;
import org.elasticsearch.index.similarity.SimilarityService;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.script.ScriptFactory;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.search.NestedDocuments;
import org.elasticsearch.search.aggregations.support.ValuesSourceRegistry;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.lookup.SourceProvider;
import org.elasticsearch.transport.RemoteClusterAware;
import org.elasticsearch.xcontent.XContentParserConfiguration;

import java.io.IOException;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;
import java.util.function.BiFunction;
import java.util.function.BooleanSupplier;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Predicate;

/**
 * The context used to execute a search request on a shard. It provides access
 * to required information like mapping definitions and document data.
 *
 * This context is used in several components of search execution, including
 * building queries and fetching hits.
 */
public class SearchExecutionContext extends QueryRewriteContext {

    private final ScriptService scriptService;
    private final IndexSettings indexSettings;
    private final MapperService mapperService;
    private final MappingLookup mappingLookup;
    private final SimilarityService similarityService;
    private final BitsetFilterCache bitsetFilterCache;
    private final BiFunction<MappedFieldType, FieldDataContext, IndexFieldData<?>> indexFieldDataLookup;
    private SearchLookup lookup = null;

    private final int shardId;
    private final int shardRequestIndex;
    private final IndexSearcher searcher;
    private boolean cacheable = true;
    private final SetOnce<Boolean> frozen = new SetOnce<>();
    private Set<String> fieldsInIndex = null;

    private final Index fullyQualifiedIndex;
    private final Predicate<String> indexNameMatcher;
    private final BooleanSupplier allowExpensiveQueries;

    private final Map<String, Query> namedQueries = new HashMap<>();
    private boolean allowUnmappedFields;
    private boolean mapUnmappedFieldAsString;
    private NestedScope nestedScope;
    private final ValuesSourceRegistry valuesSourceRegistry;
    private final Map<String, MappedFieldType> runtimeMappings;
    private Predicate<String> allowedFields;

    /**
     * Build a {@linkplain SearchExecutionContext}.
     */
    public SearchExecutionContext(
        int shardId,
        int shardRequestIndex,
        IndexSettings indexSettings,
        BitsetFilterCache bitsetFilterCache,
        BiFunction<MappedFieldType, FieldDataContext, IndexFieldData<?>> indexFieldDataLookup,
        MapperService mapperService,
        MappingLookup mappingLookup,
        SimilarityService similarityService,
        ScriptService scriptService,
        XContentParserConfiguration parserConfiguration,
        NamedWriteableRegistry namedWriteableRegistry,
        Client client,
        IndexSearcher searcher,
        LongSupplier nowInMillis,
        String clusterAlias,
        Predicate<String> indexNameMatcher,
        BooleanSupplier allowExpensiveQueries,
        ValuesSourceRegistry valuesSourceRegistry,
        Map<String, Object> runtimeMappings
    ) {
        this(
            shardId,
            shardRequestIndex,
            indexSettings,
            bitsetFilterCache,
            indexFieldDataLookup,
            mapperService,
            mappingLookup,
            similarityService,
            scriptService,
            parserConfiguration,
            namedWriteableRegistry,
            client,
            searcher,
            nowInMillis,
            indexNameMatcher,
            new Index(
                RemoteClusterAware.buildRemoteIndexName(clusterAlias, indexSettings.getIndex().getName()),
                indexSettings.getIndex().getUUID()
            ),
            allowExpensiveQueries,
            valuesSourceRegistry,
            parseRuntimeMappings(runtimeMappings, mapperService, indexSettings, mappingLookup),
            null
        );
    }

    public SearchExecutionContext(SearchExecutionContext source) {
        this(
            source.shardId,
            source.shardRequestIndex,
            source.indexSettings,
            source.bitsetFilterCache,
            source.indexFieldDataLookup,
            source.mapperService,
            source.mappingLookup,
            source.similarityService,
            source.scriptService,
            source.getParserConfig(),
            source.getWriteableRegistry(),
            source.client,
            source.searcher,
            source.nowInMillis,
            source.indexNameMatcher,
            source.fullyQualifiedIndex,
            source.allowExpensiveQueries,
            source.valuesSourceRegistry,
            source.runtimeMappings,
            source.allowedFields
        );
    }

    private SearchExecutionContext(
        int shardId,
        int shardRequestIndex,
        IndexSettings indexSettings,
        BitsetFilterCache bitsetFilterCache,
        BiFunction<MappedFieldType, FieldDataContext, IndexFieldData<?>> indexFieldDataLookup,
        MapperService mapperService,
        MappingLookup mappingLookup,
        SimilarityService similarityService,
        ScriptService scriptService,
        XContentParserConfiguration parserConfig,
        NamedWriteableRegistry namedWriteableRegistry,
        Client client,
        IndexSearcher searcher,
        LongSupplier nowInMillis,
        Predicate<String> indexNameMatcher,
        Index fullyQualifiedIndex,
        BooleanSupplier allowExpensiveQueries,
        ValuesSourceRegistry valuesSourceRegistry,
        Map<String, MappedFieldType> runtimeMappings,
        Predicate<String> allowedFields
    ) {
        super(parserConfig, namedWriteableRegistry, client, nowInMillis);
        this.shardId = shardId;
        this.shardRequestIndex = shardRequestIndex;
        this.similarityService = similarityService;
        this.mapperService = mapperService;
        this.mappingLookup = mappingLookup;
        this.bitsetFilterCache = bitsetFilterCache;
        this.indexFieldDataLookup = indexFieldDataLookup;
        this.allowUnmappedFields = indexSettings.isDefaultAllowUnmappedFields();
        this.nestedScope = new NestedScope();
        this.scriptService = scriptService;
        this.indexSettings = indexSettings;
        this.searcher = searcher;
        this.indexNameMatcher = indexNameMatcher;
        this.fullyQualifiedIndex = fullyQualifiedIndex;
        this.allowExpensiveQueries = allowExpensiveQueries;
        this.valuesSourceRegistry = valuesSourceRegistry;
        this.runtimeMappings = runtimeMappings;
        this.allowedFields = allowedFields;
    }

    private void reset() {
        allowUnmappedFields = indexSettings.isDefaultAllowUnmappedFields();
        this.lookup = null;
        this.namedQueries.clear();
        this.nestedScope = new NestedScope();
    }

    /**
     * The similarity to use in searches, which takes into account per-field configuration.
     */
    public Similarity getSearchSimilarity() {
        return similarityService != null ? similarityService.similarity(this::fieldType) : null;
    }

    /**
     * The default similarity configured in the index settings.
     */
    public Similarity getDefaultSimilarity() {
        return similarityService != null ? similarityService.getDefaultSimilarity() : null;
    }

    public List<String> defaultFields() {
        return indexSettings.getDefaultFields();
    }

    public boolean queryStringLenient() {
        return indexSettings.isQueryStringLenient();
    }

    public boolean queryStringAnalyzeWildcard() {
        return indexSettings.isQueryStringAnalyzeWildcard();
    }

    public boolean queryStringAllowLeadingWildcard() {
        return indexSettings.isQueryStringAllowLeadingWildcard();
    }

    public BitSetProducer bitsetFilter(Query filter) {
        return bitsetFilterCache.getBitSetProducer(filter);
    }

    public boolean allowExpensiveQueries() {
        return allowExpensiveQueries.getAsBoolean();
    }

    @SuppressWarnings("unchecked")
    public <IFD extends IndexFieldData<?>> IFD getForField(MappedFieldType fieldType, FielddataOperation fielddataOperation) {
        return (IFD) indexFieldDataLookup.apply(
            fieldType,
            new FieldDataContext(
                fullyQualifiedIndex.getName(),
                () -> this.lookup().forkAndTrackFieldReferences(fieldType.name()),
                this::sourcePath,
                fielddataOperation
            )
        );
    }

    public void addNamedQuery(String name, Query query) {
        if (query != null) {
            namedQueries.put(name, query);
        }
    }

    public Map<String, Query> copyNamedQueries() {
        // This might be a good use case for CopyOnWriteHashMap
        return Map.copyOf(namedQueries);
    }

    /**
     * Parse a document with current mapping.
     */
    public ParsedDocument parseDocument(SourceToParse source) throws MapperParsingException {
        return mapperService.documentParser().parseDocument(source, mappingLookup);
    }

    public NestedLookup nestedLookup() {
        return mappingLookup.nestedLookup();
    }

    public boolean hasMappings() {
        return mappingLookup.hasMappings();
    }

    /**
     * Returns the names of all mapped fields that match a given pattern
     *
     * All names returned by this method are guaranteed to resolve to a
     * MappedFieldType if passed to {@link #getFieldType(String)}
     *
     * @param pattern the field name pattern
     */
    public Set<String> getMatchingFieldNames(String pattern) {
        if (runtimeMappings.isEmpty()) {
            return mappingLookup.getMatchingFieldNames(pattern);
        }
        Set<String> matches = new HashSet<>(mappingLookup.getMatchingFieldNames(pattern));
        if ("*".equals(pattern)) {
            matches.addAll(runtimeMappings.keySet());
        } else if (Regex.isSimpleMatchPattern(pattern) == false) {
            // no wildcard
            if (runtimeMappings.containsKey(pattern)) {
                matches.add(pattern);
            }
        } else {
            for (String name : runtimeMappings.keySet()) {
                if (Regex.simpleMatch(pattern, name)) {
                    matches.add(name);
                }
            }
        }
        return matches;
    }

    /**
     * Returns the {@link MappedFieldType} for the provided field name.
     * If the field is not mapped, the behaviour depends on the index.query.parse.allow_unmapped_fields setting, which defaults to true.
     * In case unmapped fields are allowed, null is returned when the field is not mapped.
     * In case unmapped fields are not allowed, either an exception is thrown or the field is automatically mapped as a text field.
     * @throws QueryShardException if unmapped fields are not allowed and automatically mapping unmapped fields as text is disabled.
     * @see SearchExecutionContext#setAllowUnmappedFields(boolean)
     * @see SearchExecutionContext#setMapUnmappedFieldAsString(boolean)
     */
    public MappedFieldType getFieldType(String name) {
        return failIfFieldMappingNotFound(name, fieldType(name));
    }

    /**
     * Returns true if the field identified by the provided name is mapped, false otherwise
     */
    public boolean isFieldMapped(String name) {
        return fieldType(name) != null;
    }

    private MappedFieldType fieldType(String name) {
        // If the field is not allowed, behave as if it is not mapped
        if (allowedFields != null && false == allowedFields.test(name)) {
            return null;
        }
        MappedFieldType fieldType = runtimeMappings.get(name);
        return fieldType == null ? mappingLookup.getFieldType(name) : fieldType;
    }

    public boolean isMetadataField(String field) {
        return mapperService.isMetadataField(field);
    }

    public boolean isMultiField(String field) {
        if (runtimeMappings.containsKey(field)) {
            return false;
        }
        return mapperService.isMultiField(field);
    }

    public Set<String> sourcePath(String fullName) {
        return mappingLookup.sourcePaths(fullName);
    }

    /**
     * Will there be {@code _source}.
     */
    public boolean isSourceEnabled() {
        return mappingLookup.isSourceEnabled();
    }

    /**
     * Does the source need to be rebuilt on the fly?
     */
    public boolean isSourceSynthetic() {
        return mappingLookup.isSourceSynthetic();
    }

    /**
     * Build something to load source {@code _source}.
     */
    public SourceLoader newSourceLoader(boolean forceSyntheticSource) {
        if (forceSyntheticSource) {
            return new SourceLoader.Synthetic(mappingLookup.getMapping());
        }
        return mappingLookup.newSourceLoader();
    }

    /**
     * Given a type (eg. long, string, ...), returns an anonymous field type that can be used for search operations.
     * Generally used to handle unmapped fields in the context of sorting.
     */
    public MappedFieldType buildAnonymousFieldType(String type) {
        MappingParserContext parserContext = mapperService.parserContext();
        Mapper.TypeParser typeParser = parserContext.typeParser(type);
        if (typeParser == null) {
            throw new IllegalArgumentException("No mapper found for type [" + type + "]");
        }
        Mapper.Builder builder = typeParser.parse("__anonymous_", Collections.emptyMap(), parserContext);
        Mapper mapper = builder.build(MapperBuilderContext.root(false));
        if (mapper instanceof FieldMapper) {
            return ((FieldMapper) mapper).fieldType();
        }
        throw new IllegalArgumentException("Mapper for type [" + type + "] must be a leaf field");
    }

    public IndexAnalyzers getIndexAnalyzers() {
        return mapperService.getIndexAnalyzers();
    }

    /**
     * Return the index-time analyzer for the current index
     * @param unindexedFieldAnalyzer    a function that builds an analyzer for unindexed fields
     */
    public Analyzer getIndexAnalyzer(Function<String, NamedAnalyzer> unindexedFieldAnalyzer) {
        return new DelegatingAnalyzerWrapper(Analyzer.PER_FIELD_REUSE_STRATEGY) {
            @Override
            protected Analyzer getWrappedAnalyzer(String fieldName) {
                return mappingLookup.indexAnalyzer(fieldName, unindexedFieldAnalyzer);
            }
        };
    }

    public ValuesSourceRegistry getValuesSourceRegistry() {
        return valuesSourceRegistry;
    }

    public void setAllowUnmappedFields(boolean allowUnmappedFields) {
        this.allowUnmappedFields = allowUnmappedFields;
    }

    public void setMapUnmappedFieldAsString(boolean mapUnmappedFieldAsString) {
        this.mapUnmappedFieldAsString = mapUnmappedFieldAsString;
    }

    public void setAllowedFields(Predicate<String> allowedFields) {
        this.allowedFields = allowedFields;
    }

    MappedFieldType failIfFieldMappingNotFound(String name, MappedFieldType fieldMapping) {
        if (fieldMapping != null || allowUnmappedFields) {
            return fieldMapping;
        } else if (mapUnmappedFieldAsString) {
            TextFieldMapper.Builder builder = new TextFieldMapper.Builder(name, getIndexAnalyzers());
            return builder.build(MapperBuilderContext.root(false)).fieldType();
        } else {
            throw new QueryShardException(this, "No field mapping can be found for the field with name [{}]", name);
        }
    }

    /**
     * Does the index analyzer for this field have token filters that may produce
     * backwards offsets in term vectors
     */
    public boolean containsBrokenAnalysis(String field) {
        NamedAnalyzer a = mappingLookup.indexAnalyzer(field, f -> null);
        return a == null ? false : a.containsBrokenAnalysis();
    }

    /**
     * Get the lookup to use during the search.
     */
    public SearchLookup lookup() {
        if (this.lookup == null) {
            SourceProvider sourceProvider = isSourceSynthetic()
                ? (ctx, doc) -> { throw new IllegalArgumentException("Cannot access source from scripts in synthetic mode"); }
                : SourceProvider.fromStoredFields();
            setSourceProvider(sourceProvider);
        }
        return this.lookup;
    }

    /**
     * Replace the standard source provider on the SearchLookup
     *
     * Note that this will replace the current SearchLookup with a new one, but will not update
     * the source provider on previously build lookups. This method should only be called before
     * IndexReader access by the current context
     */
    public void setSourceProvider(SourceProvider sourceProvider) {
        // TODO can we assert that this is only called during FetchPhase?
        this.lookup = new SearchLookup(
            this::getFieldType,
            (fieldType, searchLookup, fielddataOperation) -> indexFieldDataLookup.apply(
                fieldType,
                new FieldDataContext(fullyQualifiedIndex.getName(), searchLookup, this::sourcePath, fielddataOperation)
            ),
            sourceProvider
        );
    }

    public NestedScope nestedScope() {
        return nestedScope;
    }

    public Version indexVersionCreated() {
        return indexSettings.getIndexVersionCreated();
    }

    /**
     *  Given an index pattern, checks whether it matches against the current shard. The pattern
     *  may represent a fully qualified index name if the search targets remote shards.
     */
    public boolean indexMatches(String pattern) {
        return indexNameMatcher.test(pattern);
    }

    public boolean indexSortedOnField(String field) {
        IndexSortConfig indexSortConfig = indexSettings.getIndexSortConfig();
        return indexSortConfig.hasPrimarySortOnField(field);
    }

    public ParsedQuery toQuery(QueryBuilder queryBuilder) {
        return toQuery(queryBuilder, q -> {
            Query query = q.toQuery(this);
            if (query == null) {
                query = Queries.newMatchNoDocsQuery("No query left after rewrite.");
            }
            return query;
        });
    }

    private ParsedQuery toQuery(QueryBuilder queryBuilder, CheckedFunction<QueryBuilder, Query, IOException> filterOrQuery) {
        reset();
        try {
            QueryBuilder rewriteQuery = Rewriteable.rewrite(queryBuilder, this, true);
            return new ParsedQuery(filterOrQuery.apply(rewriteQuery), copyNamedQueries());
        } catch (QueryShardException | ParsingException e) {
            throw e;
        } catch (Exception e) {
            throw new QueryShardException(this, "failed to create query: {}", e, e.getMessage());
        } finally {
            reset();
        }
    }

    public Index index() {
        return indexSettings.getIndex();
    }

    /** Compile script using script service */
    public <FactoryType> FactoryType compile(Script script, ScriptContext<FactoryType> context) {
        FactoryType factory = scriptService.compile(script, context);
        if (factory instanceof ScriptFactory && ((ScriptFactory) factory).isResultDeterministic() == false) {
            failIfFrozen();
        }
        return factory;
    }

    /**
     * if this method is called the query context will throw exception if methods are accessed
     * that could yield different results across executions like {@link #getClient()}
     */
    public final void freezeContext() {
        this.frozen.set(Boolean.TRUE);
    }

    /**
     * Marks this context as not cacheable.
     * This method fails if {@link #freezeContext()} is called before on this context.
     */
    public void disableCache() {
        failIfFrozen();
    }

    /**
     * This method fails if {@link #freezeContext()} is called before on this
     * context. This is used to <i>seal</i>.
     *
     * This methods and all methods that call it should be final to ensure that
     * setting the request as not cacheable and the freezing behaviour of this
     * class cannot be bypassed. This is important so we can trust when this
     * class says a request can be cached.
     */
    protected final void failIfFrozen() {
        this.cacheable = false;
        if (frozen.get() == Boolean.TRUE) {
            throw new IllegalArgumentException("features that prevent cachability are disabled on this context");
        } else {
            assert frozen.get() == null : frozen.get();
        }
    }

    @Override
    public void registerAsyncAction(BiConsumer<Client, ActionListener<?>> asyncAction) {
        failIfFrozen();
        super.registerAsyncAction(asyncAction);
    }

    @Override
    @SuppressWarnings("rawtypes")
    public void executeAsyncActions(ActionListener listener) {
        failIfFrozen();
        super.executeAsyncActions(listener);
    }

    /**
     * Returns <code>true</code> iff the result of the processed search request is cacheable. Otherwise <code>false</code>
     */
    public final boolean isCacheable() {
        return cacheable;
    }

    /**
     * Returns the shard ID this context was created for.
     */
    public int getShardId() {
        return shardId;
    }

    /**
     * Returns the shard request ordinal that is used by the main search request
     * to reference this shard.
     */
    public int getShardRequestIndex() {
        return shardRequestIndex;
    }

    @Override
    public final long nowInMillis() {
        failIfFrozen();
        return super.nowInMillis();
    }

    public Client getClient() {
        failIfFrozen(); // we somebody uses a terms filter with lookup for instance can't be cached...
        return client;
    }

    @Override
    public final SearchExecutionContext convertToSearchExecutionContext() {
        return this;
    }

    /**
     * Returns the index settings for this context. This might return null if the
     * context has not index scope.
     */
    public IndexSettings getIndexSettings() {
        return indexSettings;
    }

    /** Return the current {@link IndexReader}, or {@code null} if no index reader is available,
     *  for instance if this rewrite context is used to index queries (percolation). */
    public IndexReader getIndexReader() {
        return searcher == null ? null : searcher.getIndexReader();
    }

    /** Return the current {@link IndexSearcher}, or {@code null} if no index reader is available,
     *  for instance if this rewrite context is used to index queries (percolation). */
    public IndexSearcher searcher() {
        return searcher;
    }

    /**
     * Is this field present in the underlying lucene index for the current shard?
     */
    public boolean fieldExistsInIndex(String fieldname) {
        if (searcher == null) {
            return false;
        }
        if (fieldsInIndex == null) {
            fieldsInIndex = new HashSet<>();
            for (LeafReaderContext ctx : searcher.getIndexReader().leaves()) {
                FieldInfos fis = ctx.reader().getFieldInfos();
                for (FieldInfo fi : fis) {
                    fieldsInIndex.add(fi.name);
                }
            }
        }
        return fieldsInIndex.contains(fieldname);
    }

    /**
     * Returns the fully qualified index including a remote cluster alias if applicable, and the index uuid
     */
    public Index getFullyQualifiedIndex() {
        return fullyQualifiedIndex;
    }

    private static Map<String, MappedFieldType> parseRuntimeMappings(
        Map<String, Object> runtimeMappings,
        MapperService mapperService,
        IndexSettings indexSettings,
        MappingLookup lookup
    ) {
        if (runtimeMappings.isEmpty()) {
            return Collections.emptyMap();
        }
        // TODO add specific tests to SearchExecutionTests similar to the ones in FieldTypeLookupTests
        MappingParserContext parserContext = mapperService.parserContext();
        Map<String, RuntimeField> runtimeFields = RuntimeField.parseRuntimeFields(new HashMap<>(runtimeMappings), parserContext, false);
        Map<String, MappedFieldType> runtimeFieldTypes = RuntimeField.collectFieldTypes(runtimeFields.values());
        if (false == indexSettings.getIndexMetadata().getRoutingPaths().isEmpty()) {
            for (String r : runtimeMappings.keySet()) {
                if (Regex.simpleMatch(indexSettings.getIndexMetadata().getRoutingPaths(), r)) {
                    throw new IllegalArgumentException("runtime fields may not match [routing_path] but [" + r + "] matched");
                }
            }
        }
        runtimeFieldTypes.keySet().forEach(lookup::validateDoesNotShadow);
        return runtimeFieldTypes;
    }

    /**
     * Cache key for current mapping.
     */
    public MappingLookup.CacheKey mappingCacheKey() {
        return mappingLookup.cacheKey();
    }

    public NestedDocuments getNestedDocuments() {
        return new NestedDocuments(mappingLookup, bitsetFilterCache::getBitSetProducer, indexVersionCreated());
    }
}
