/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authz;

import org.elasticsearch.action.AliasesRequest;
import org.elasticsearch.action.IndicesRequest;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest;
import org.elasticsearch.action.admin.indices.alias.get.GetAliasesRequest;
import org.elasticsearch.action.admin.indices.exists.indices.IndicesExistsRequest;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingRequest;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesRequest;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.cluster.metadata.AliasMetaData;
import org.elasticsearch.cluster.metadata.AliasOrIndex;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.transport.RemoteClusterAware;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.xpack.core.graph.action.GraphExploreRequest;
import org.elasticsearch.xpack.core.security.authz.IndicesAndAliasesResolverField;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.SortedMap;
import java.util.concurrent.CopyOnWriteArraySet;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.core.security.authz.IndicesAndAliasesResolverField.NO_INDEX_PLACEHOLDER;

class IndicesAndAliasesResolver {

    //`*,-*` what we replace indices with if we need Elasticsearch to return empty responses without throwing exception
    private static final String[] NO_INDICES_ARRAY = new String[] { "*", "-*" };
    static final List<String> NO_INDICES_LIST = Arrays.asList(NO_INDICES_ARRAY);

    private final IndexNameExpressionResolver nameExpressionResolver;
    private final RemoteClusterResolver remoteClusterResolver;

    IndicesAndAliasesResolver(Settings settings, ClusterService clusterService) {
        this.nameExpressionResolver = new IndexNameExpressionResolver(settings);
        this.remoteClusterResolver = new RemoteClusterResolver(settings, clusterService.getClusterSettings());
    }

    /**
     * Resolves, and if necessary updates, the list of index names in the provided <code>request</code> in accordance with the user's
     * <code>authorizedIndices</code>.
     * <p>
     * Wildcards are expanded at this phase to ensure that all security and execution decisions are made against a fixed set of index names
     * that is consistent and does not change during the life of the request.
     * </p>
     * <p>
     * If the provided <code>request</code> is of a type that {@link #allowsRemoteIndices(IndicesRequest) allows remote indices},
     * then the index names will be categorized into those that refer to {@link ResolvedIndices#getLocal() local indices}, and those that
     * refer to {@link ResolvedIndices#getRemote() remote indices}. This categorization follows the standard
     * {@link RemoteClusterAware#buildRemoteIndexName(String, String) remote index-name format} and also respects the currently defined
     * {@link RemoteClusterAware#getRemoteClusterNames() remote clusters}.
     * </p><br>
     * Thus an index name <em>N</em> will considered to be <em>remote</em> if-and-only-if all of the following are true
     * <ul>
     * <li><code>request</code> supports remote indices</li>
     * <li>
     * <em>N</em> is in the format <i>cluster</i><code>:</code><i>index</i>.
     * It is allowable for <i>cluster</i> and <i>index</i> to contain wildcards, but the separator (<code>:</code>) must be explicit.
     * </li>
     * <li><i>cluster</i> matches one or more remote cluster names that are registered within this cluster.</li>
     * </ul>
     * In which case, any wildcards in the <i>cluster</i> portion of the name will be expanded and the resulting remote-index-name(s) will
     * be added to the <em>remote</em> index list.
     * <br>
     * Otherwise, <em>N</em> will be added to the <em>local</em> index list.
     */

    ResolvedIndices resolve(TransportRequest request, MetaData metaData, AuthorizedIndices authorizedIndices) {
        if (request instanceof IndicesAliasesRequest) {
            ResolvedIndices.Builder resolvedIndicesBuilder = new ResolvedIndices.Builder();
            IndicesAliasesRequest indicesAliasesRequest = (IndicesAliasesRequest) request;
            for (IndicesRequest indicesRequest : indicesAliasesRequest.getAliasActions()) {
                final ResolvedIndices resolved = resolveIndicesAndAliases(indicesRequest, metaData, authorizedIndices);
                resolvedIndicesBuilder.addLocal(resolved.getLocal());
                resolvedIndicesBuilder.addRemote(resolved.getRemote());
            }
            return resolvedIndicesBuilder.build();
        }

        // if for some reason we are missing an action... just for safety we'll reject
        if (request instanceof IndicesRequest == false) {
            throw new IllegalStateException("Request [" + request + "] is not an Indices request, but should be.");
        }
        return resolveIndicesAndAliases((IndicesRequest) request, metaData, authorizedIndices);
    }


    ResolvedIndices resolveIndicesAndAliases(IndicesRequest indicesRequest, MetaData metaData, AuthorizedIndices authorizedIndices) {
        final ResolvedIndices.Builder resolvedIndicesBuilder = new ResolvedIndices.Builder();
        boolean indicesReplacedWithNoIndices = false;
        if (indicesRequest instanceof PutMappingRequest && ((PutMappingRequest) indicesRequest).getConcreteIndex() != null) {
            /*
             * This is a special case since PutMappingRequests from dynamic mapping updates have a concrete index
             * if this index is set and it's in the list of authorized indices we are good and don't need to put
             * the list of indices in there, if we do so it will result in an invalid request and the update will fail.
             */
            assert indicesRequest.indices() == null || indicesRequest.indices().length == 0
                    : "indices are: " + Arrays.toString(indicesRequest.indices()); // Arrays.toString() can handle null values - all good
            resolvedIndicesBuilder.addLocal(getPutMappingIndexOrAlias((PutMappingRequest) indicesRequest, authorizedIndices, metaData));
        } else if (indicesRequest instanceof IndicesRequest.Replaceable) {
            IndicesRequest.Replaceable replaceable = (IndicesRequest.Replaceable) indicesRequest;
            final boolean replaceWildcards = indicesRequest.indicesOptions().expandWildcardsOpen()
                    || indicesRequest.indicesOptions().expandWildcardsClosed();
            IndicesOptions indicesOptions = indicesRequest.indicesOptions();
            if (indicesRequest instanceof IndicesExistsRequest) {
                //indices exists api should never throw exception, make sure that ignore_unavailable and allow_no_indices are true
                //we have to mimic what TransportIndicesExistsAction#checkBlock does in es core
                indicesOptions = IndicesOptions.fromOptions(true, true,
                        indicesOptions.expandWildcardsOpen(), indicesOptions.expandWildcardsClosed());
            }

            // check for all and return list of authorized indices
            if (IndexNameExpressionResolver.isAllIndices(indicesList(indicesRequest.indices()))) {
                if (replaceWildcards) {
                    for (String authorizedIndex : authorizedIndices.get()) {
                        if (isIndexVisible(authorizedIndex, indicesOptions, metaData)) {
                            resolvedIndicesBuilder.addLocal(authorizedIndex);
                        }
                    }
                }
                // if we cannot replace wildcards the indices list stays empty. Same if there are no authorized indices.
                // we honour allow_no_indices like es core does.
            } else {
                final ResolvedIndices split;
                if (allowsRemoteIndices(indicesRequest)) {
                    split = remoteClusterResolver.splitLocalAndRemoteIndexNames(indicesRequest.indices());
                } else {
                    split = new ResolvedIndices(Arrays.asList(indicesRequest.indices()), Collections.emptyList());
                }
                List<String> replaced = replaceWildcardsWithAuthorizedIndices(split.getLocal(), indicesOptions, metaData,
                        authorizedIndices.get(), replaceWildcards);
                if (indicesOptions.ignoreUnavailable()) {
                    //out of all the explicit names (expanded from wildcards and original ones that were left untouched)
                    //remove all the ones that the current user is not authorized for and ignore them
                    replaced = replaced.stream().filter(authorizedIndices.get()::contains).collect(Collectors.toList());
                }
                resolvedIndicesBuilder.addLocal(replaced);
                resolvedIndicesBuilder.addRemote(split.getRemote());
            }

            if (resolvedIndicesBuilder.isEmpty()) {
                if (indicesOptions.allowNoIndices()) {
                    //this is how we tell es core to return an empty response, we can let the request through being sure
                    //that the '-*' wildcard expression will be resolved to no indices. We can't let empty indices through
                    //as that would be resolved to _all by es core.
                    replaceable.indices(NO_INDICES_ARRAY);
                    indicesReplacedWithNoIndices = true;
                    resolvedIndicesBuilder.addLocal(NO_INDEX_PLACEHOLDER);
                } else {
                    throw new IndexNotFoundException(Arrays.toString(indicesRequest.indices()));
                }
            } else {
                replaceable.indices(resolvedIndicesBuilder.build().toArray());
            }
        } else {
            if (containsWildcards(indicesRequest)) {
                //an alias can still contain '*' in its name as of 5.0. Such aliases cannot be referred to when using
                //the security plugin, otherwise the following exception gets thrown
                throw new IllegalStateException("There are no external requests known to support wildcards that don't support replacing " +
                        "their indices");
            }
            //NOTE: shard level requests do support wildcards (as they hold the original indices options) but don't support
            // replacing their indices.
            //That is fine though because they never contain wildcards, as they get replaced as part of the authorization of their
            //corresponding parent request on the coordinating node. Hence wildcards don't need to get replaced nor exploded for
            // shard level requests.
            for (String name : indicesRequest.indices()) {
                resolvedIndicesBuilder.addLocal(nameExpressionResolver.resolveDateMathExpression(name));
            }
        }

        if (indicesRequest instanceof AliasesRequest) {
            //special treatment for AliasesRequest since we need to replace wildcards among the specified aliases too.
            //AliasesRequest extends IndicesRequest.Replaceable, hence its indices have already been properly replaced.
            AliasesRequest aliasesRequest = (AliasesRequest) indicesRequest;
            if (aliasesRequest.expandAliasesWildcards()) {
                List<String> aliases = replaceWildcardsWithAuthorizedAliases(aliasesRequest.aliases(),
                        loadAuthorizedAliases(authorizedIndices.get(), metaData));
                //it may be that we replace aliases with an empty array, in case there are no authorized aliases for the action.
                //MetaData#findAliases will return nothing when some alias was originally requested, which was replaced with empty.
                aliasesRequest.replaceAliases(aliases.toArray(new String[aliases.size()]));
            }
            if (indicesReplacedWithNoIndices) {
                if (indicesRequest instanceof GetAliasesRequest == false) {
                    throw new IllegalStateException(GetAliasesRequest.class.getSimpleName() + " is the only known " +
                            "request implementing " + AliasesRequest.class.getSimpleName() + " that may allow no indices. Found [" +
                            indicesRequest.getClass().getName() + "] which ended up with an empty set of indices.");
                }
                //if we replaced the indices with '-*' we shouldn't be adding the aliases to the list otherwise the request will
                //not get authorized. Leave only '-*' and ignore the rest, result will anyway be empty.
            } else {
                resolvedIndicesBuilder.addLocal(aliasesRequest.aliases());
            }
        }
        return resolvedIndicesBuilder.build();
    }

    /**
     * Special handling of the value to authorize for a put mapping request. Dynamic put mapping
     * requests use a concrete index, but we allow permissions to be defined on aliases so if the
     * request's concrete index is not in the list of authorized indices, then we need to look to
     * see if this can be authorized against an alias
     */
    static String getPutMappingIndexOrAlias(PutMappingRequest request, AuthorizedIndices authorizedIndices, MetaData metaData) {
        final String concreteIndexName = request.getConcreteIndex().getName();
        final List<String> authorizedIndicesList = authorizedIndices.get();

        // validate that the concrete index exists, otherwise there is no remapping that we could do
        final AliasOrIndex aliasOrIndex = metaData.getAliasAndIndexLookup().get(concreteIndexName);
        final String resolvedAliasOrIndex;
        if (aliasOrIndex == null) {
            resolvedAliasOrIndex = concreteIndexName;
        } else if (aliasOrIndex.isAlias()) {
            throw new IllegalStateException("concrete index [" + concreteIndexName + "] is an alias but should not be");
        } else if (authorizedIndicesList.contains(concreteIndexName)) {
            // user is authorized to put mappings for this index
            resolvedAliasOrIndex = concreteIndexName;
        } else {
            // the user is not authorized to put mappings for this index, but could have been
            // authorized for a write using an alias that triggered a dynamic mapping update
            ImmutableOpenMap<String, List<AliasMetaData>> foundAliases = metaData.findAllAliases(new String[] { concreteIndexName });
            List<AliasMetaData> aliasMetaData = foundAliases.get(concreteIndexName);
            if (aliasMetaData != null) {
                Optional<String> foundAlias = aliasMetaData.stream()
                    .map(AliasMetaData::alias)
                    .filter(authorizedIndicesList::contains)
                    .filter(aliasName -> metaData.getAliasAndIndexLookup().get(aliasName).getIndices().size() == 1)
                    .findFirst();
                resolvedAliasOrIndex = foundAlias.orElse(concreteIndexName);
            } else {
                resolvedAliasOrIndex = concreteIndexName;
            }
        }

        return resolvedAliasOrIndex;
    }

    static boolean allowsRemoteIndices(IndicesRequest request) {
        return request instanceof SearchRequest || request instanceof FieldCapabilitiesRequest
                || request instanceof GraphExploreRequest;
    }

    private List<String> loadAuthorizedAliases(List<String> authorizedIndices, MetaData metaData) {
        List<String> authorizedAliases = new ArrayList<>();
        SortedMap<String, AliasOrIndex> existingAliases = metaData.getAliasAndIndexLookup();
        for (String authorizedIndex : authorizedIndices) {
            AliasOrIndex aliasOrIndex = existingAliases.get(authorizedIndex);
            if (aliasOrIndex != null && aliasOrIndex.isAlias()) {
                authorizedAliases.add(authorizedIndex);
            }
        }
        return authorizedAliases;
    }

    private List<String> replaceWildcardsWithAuthorizedAliases(String[] aliases, List<String> authorizedAliases) {
        List<String> finalAliases = new ArrayList<>();

        //IndicesAliasesRequest doesn't support empty aliases (validation fails) but GetAliasesRequest does (in which case empty means _all)
        if (aliases.length == 0) {
            finalAliases.addAll(authorizedAliases);
        }

        for (String aliasPattern : aliases) {
            if (aliasPattern.equals(MetaData.ALL)) {
                finalAliases.addAll(authorizedAliases);
            } else if (Regex.isSimpleMatchPattern(aliasPattern)) {
                for (String authorizedAlias : authorizedAliases) {
                    if (Regex.simpleMatch(aliasPattern, authorizedAlias)) {
                        finalAliases.add(authorizedAlias);
                    }
                }
            } else {
                finalAliases.add(aliasPattern);
            }
        }
        return finalAliases;
    }

    private boolean containsWildcards(IndicesRequest indicesRequest) {
        if (IndexNameExpressionResolver.isAllIndices(indicesList(indicesRequest.indices()))) {
            return true;
        }
        for (String index : indicesRequest.indices()) {
            if (Regex.isSimpleMatchPattern(index)) {
                return true;
            }
        }
        return false;
    }

    //TODO Investigate reusing code from vanilla es to resolve index names and wildcards
    private List<String> replaceWildcardsWithAuthorizedIndices(Iterable<String> indices, IndicesOptions indicesOptions, MetaData metaData,
                                                               List<String> authorizedIndices, boolean replaceWildcards) {
        //the order matters when it comes to exclusions
        List<String> finalIndices = new ArrayList<>();
        boolean wildcardSeen = false;
        for (String index : indices) {
            String aliasOrIndex;
            boolean minus = false;
            if (index.charAt(0) == '-' && wildcardSeen) {
                aliasOrIndex = index.substring(1);
                minus = true;
            } else {
                aliasOrIndex = index;
            }

            // we always need to check for date math expressions
            final String dateMathName = nameExpressionResolver.resolveDateMathExpression(aliasOrIndex);
            if (dateMathName != aliasOrIndex) {
                assert dateMathName.equals(aliasOrIndex) == false;
                if (replaceWildcards && Regex.isSimpleMatchPattern(dateMathName)) {
                    // continue
                    aliasOrIndex = dateMathName;
                } else if (authorizedIndices.contains(dateMathName) && isIndexVisible(dateMathName, indicesOptions, metaData, true)) {
                    if (minus) {
                        finalIndices.remove(dateMathName);
                    } else {
                        finalIndices.add(dateMathName);
                    }
                } else {
                    if (indicesOptions.ignoreUnavailable() == false) {
                        throw new IndexNotFoundException(dateMathName);
                    }
                }
            }

            if (replaceWildcards && Regex.isSimpleMatchPattern(aliasOrIndex)) {
                wildcardSeen = true;
                Set<String> resolvedIndices = new HashSet<>();
                for (String authorizedIndex : authorizedIndices) {
                    if (Regex.simpleMatch(aliasOrIndex, authorizedIndex) && isIndexVisible(authorizedIndex, indicesOptions, metaData)) {
                        resolvedIndices.add(authorizedIndex);
                    }
                }
                if (resolvedIndices.isEmpty()) {
                    //es core honours allow_no_indices for each wildcard expression, we do the same here by throwing index not found.
                    if (indicesOptions.allowNoIndices() == false) {
                        throw new IndexNotFoundException(aliasOrIndex);
                    }
                } else {
                    if (minus) {
                        finalIndices.removeAll(resolvedIndices);
                    } else {
                        finalIndices.addAll(resolvedIndices);
                    }
                }
            } else if (dateMathName == aliasOrIndex) {
                // we can use == here to compare strings since the name expression resolver returns the same instance, but add an assert
                // to ensure we catch this if it changes

                assert dateMathName.equals(aliasOrIndex);
                //MetaData#convertFromWildcards checks if the index exists here and throws IndexNotFoundException if not (based on
                // ignore_unavailable). We only add/remove the index: if the index is missing or the current user is not authorized
                // to access it either an AuthorizationException will be thrown later in AuthorizationService, or the index will be
                // removed from the list, based on the ignore_unavailable option.
                if (minus) {
                    finalIndices.remove(aliasOrIndex);
                } else {
                    finalIndices.add(aliasOrIndex);
                }
            }
        }
        return finalIndices;
    }

    private static boolean isIndexVisible(String index, IndicesOptions indicesOptions, MetaData metaData) {
        return isIndexVisible(index, indicesOptions, metaData, false);
    }

    private static boolean isIndexVisible(String index, IndicesOptions indicesOptions, MetaData metaData, boolean dateMathExpression) {
        AliasOrIndex aliasOrIndex = metaData.getAliasAndIndexLookup().get(index);
        if (aliasOrIndex.isAlias()) {
            //it's an alias, ignore expandWildcardsOpen and expandWildcardsClosed.
            //complicated to support those options with aliases pointing to multiple indices...
            //TODO investigate supporting expandWildcards option for aliases too, like es core does.
            return indicesOptions.ignoreAliases() == false;
        }
        assert aliasOrIndex.getIndices().size() == 1 : "concrete index must point to a single index";
        IndexMetaData indexMetaData = aliasOrIndex.getIndices().get(0);
        if (indexMetaData.getState() == IndexMetaData.State.CLOSE && (indicesOptions.expandWildcardsClosed() || dateMathExpression)) {
            return true;
        }
        if (indexMetaData.getState() == IndexMetaData.State.OPEN && (indicesOptions.expandWildcardsOpen() || dateMathExpression)) {
            return true;
        }
        return false;
    }

    private static List<String> indicesList(String[] list) {
        return (list == null) ? null : Arrays.asList(list);
    }

    private static class RemoteClusterResolver extends RemoteClusterAware {

        private final CopyOnWriteArraySet<String> clusters;

        private RemoteClusterResolver(Settings settings, ClusterSettings clusterSettings) {
            super(settings);
            clusters = new CopyOnWriteArraySet<>(buildRemoteClustersSeeds(settings).keySet());
            listenForUpdates(clusterSettings);
        }

        @Override
        protected Set<String> getRemoteClusterNames() {
            return clusters;
        }

        @Override
        protected void updateRemoteCluster(String clusterAlias, List<String> addresses) {
            if (addresses.isEmpty()) {
                clusters.remove(clusterAlias);
            } else {
                clusters.add(clusterAlias);
            }
        }

        ResolvedIndices splitLocalAndRemoteIndexNames(String... indices) {
            final Map<String, List<String>> map = super.groupClusterIndices(indices, exists -> false);
            final List<String> local = map.remove(LOCAL_CLUSTER_GROUP_KEY);
            final List<String> remote = map.entrySet().stream()
                    .flatMap(e -> e.getValue().stream().map(v -> e.getKey() + REMOTE_CLUSTER_INDEX_SEPARATOR + v))
                    .collect(Collectors.toList());
            return new ResolvedIndices(local == null ? Collections.emptyList() : local, remote);
        }
    }

    /**
     * Stores a collection of index names separated into "local" and "remote".
     * This allows the resolution and categorization to take place exactly once per-request.
     */
    public static class ResolvedIndices {
        private final List<String> local;
        private final List<String> remote;

        ResolvedIndices(List<String> local, List<String> remote) {
            this.local = Collections.unmodifiableList(local);
            this.remote = Collections.unmodifiableList(remote);
        }

        /**
         * Returns the collection of index names that have been stored as "local" indices.
         * This is a <code>List</code> because order may be important. For example <code>[ "a*" , "-a1" ]</code> is interpreted differently
         * to <code>[ "-a1", "a*" ]</code>. As a consequence, this list <em>may contain duplicates</em>.
         */
        public List<String> getLocal() {
            return local;
        }

        /**
         * Returns the collection of index names that have been stored as "remote" indices.
         */
        public List<String> getRemote() {
            return remote;
        }

        /**
         * @return <code>true</code> if both the {@link #getLocal() local} and {@link #getRemote() remote} index lists are empty.
         */
        public boolean isEmpty() {
            return local.isEmpty() && remote.isEmpty();
        }

        /**
         * @return <code>true</code> if the {@link #getRemote() remote} index lists is empty, and the local index list contains the
         * {@link IndicesAndAliasesResolverField#NO_INDEX_PLACEHOLDER no-index-placeholder} and nothing else.
         */
        public boolean isNoIndicesPlaceholder() {
            return remote.isEmpty() && local.size() == 1 && local.contains(NO_INDEX_PLACEHOLDER);
        }

        private String[] toArray() {
            final String[] array = new String[local.size() + remote.size()];
            int i = 0;
            for (String index : local) {
                array[i++] = index;
            }
            for (String index : remote) {
                array[i++] = index;
            }
            return array;
        }

        /**
         * Builder class for ResolvedIndices that allows for the building of a list of indices
         * without the need to construct new objects and merging them together
         */
        private static class Builder {

            private final List<String> local = new ArrayList<>();
            private final List<String> remote = new ArrayList<>();

            /** add a local index name */
            private void addLocal(String index) {
                local.add(index);
            }

            /** adds the array of local index names */
            private void addLocal(String[] indices) {
                local.addAll(Arrays.asList(indices));
            }

            /** adds the list of local index names */
            private void addLocal(List<String> indices) {
                local.addAll(indices);
            }

            /** adds the list of remote index names */
            private void addRemote(List<String> indices) {
                remote.addAll(indices);
            }

            /** @return <code>true</code> if both the local and remote index lists are empty. */
            private boolean isEmpty() {
                return local.isEmpty() && remote.isEmpty();
            }

            /** @return a immutable ResolvedIndices instance with the local and remote index lists */
            private ResolvedIndices build() {
                return new ResolvedIndices(local, remote);
            }
        }
    }
}
