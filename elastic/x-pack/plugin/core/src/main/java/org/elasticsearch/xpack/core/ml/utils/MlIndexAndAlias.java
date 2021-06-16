/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ml.utils;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthRequest;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthResponse;
import org.elasticsearch.action.admin.indices.alias.Alias;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequestBuilder;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequestBuilder;
import org.elasticsearch.action.admin.indices.create.CreateIndexResponse;
import org.elasticsearch.action.admin.indices.template.put.PutComposableIndexTemplateAction;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateRequest;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.Requests;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.ComposableIndexTemplate;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.xpack.core.template.IndexTemplateConfig;

import java.io.IOException;
import java.util.Arrays;
import java.util.Comparator;
import java.util.Optional;
import java.util.function.Predicate;
import java.util.regex.Pattern;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

/**
 * Utils to create an ML index with alias ready for rollover with a 6-digit suffix
 */
public final class MlIndexAndAlias {

    private static final Logger logger = LogManager.getLogger(MlIndexAndAlias.class);

    // Visible for testing
    static final Comparator<String> INDEX_NAME_COMPARATOR = new Comparator<>() {

        private final Predicate<String> HAS_SIX_DIGIT_SUFFIX = Pattern.compile("\\d{6}").asMatchPredicate();

        @Override
        public int compare(String index1, String index2) {
            String[] index1Parts = index1.split("-");
            String index1Suffix = index1Parts[index1Parts.length - 1];
            boolean index1HasSixDigitsSuffix = HAS_SIX_DIGIT_SUFFIX.test(index1Suffix);
            String[] index2Parts = index2.split("-");
            String index2Suffix = index2Parts[index2Parts.length - 1];
            boolean index2HasSixDigitsSuffix = HAS_SIX_DIGIT_SUFFIX.test(index2Suffix);
            if (index1HasSixDigitsSuffix && index2HasSixDigitsSuffix) {
                return index1Suffix.compareTo(index2Suffix);
            } else if (index1HasSixDigitsSuffix != index2HasSixDigitsSuffix) {
                return Boolean.compare(index1HasSixDigitsSuffix, index2HasSixDigitsSuffix);
            } else {
                return index1.compareTo(index2);
            }
        }
    };

    private MlIndexAndAlias() {}

    /**
     * Creates the first index with a name of the given {@code indexPatternPrefix} followed by "-000001", if the index is missing.
     * Adds an {@code alias} to that index if it was created,
     * or to the index with the highest suffix if the index did not have to be created.
     * The listener is notified with a {@code boolean} that informs whether the index or the alias were created.
     * If the index is created, the listener is not called until the index is ready to use via the supplied alias,
     * so that a method that receives a success response from this method can safely use the index immediately.
     */
    public static void createIndexAndAliasIfNecessary(Client client,
                                                      ClusterState clusterState,
                                                      IndexNameExpressionResolver resolver,
                                                      String indexPatternPrefix,
                                                      String alias,
                                                      TimeValue masterNodeTimeout,
                                                      ActionListener<Boolean> finalListener) {

        final ActionListener<Boolean> loggingListener = ActionListener.wrap(
            finalListener::onResponse,
            e -> {
                logger.error(new ParameterizedMessage(
                        "Failed to create alias and index with pattern [{}] and alias [{}]",
                        indexPatternPrefix,
                        alias),
                    e);
                finalListener.onFailure(e);
            }
        );

        // If both the index and alias were successfully created then wait for the shards of the index that the alias points to be ready
        ActionListener<Boolean> indexCreatedListener = ActionListener.wrap(
            created -> {
                if (created) {
                    waitForShardsReady(client, alias, masterNodeTimeout, loggingListener);
                } else {
                    loggingListener.onResponse(false);
                }
            },
            loggingListener::onFailure
        );

        String legacyIndexWithoutSuffix = indexPatternPrefix;
        String indexPattern = indexPatternPrefix + "*";
        // The initial index name must be suitable for rollover functionality.
        String firstConcreteIndex = indexPatternPrefix + "-000001";
        String[] concreteIndexNames =
            resolver.concreteIndexNames(clusterState, IndicesOptions.lenientExpandHidden(), indexPattern);
        Optional<IndexMetadata> indexPointedByCurrentWriteAlias = clusterState.getMetadata().hasAlias(alias)
            ? clusterState.getMetadata().getIndicesLookup().get(alias).getIndices().stream().findFirst()
            : Optional.empty();

        if (concreteIndexNames.length == 0) {
            if (indexPointedByCurrentWriteAlias.isEmpty()) {
                createFirstConcreteIndex(client, firstConcreteIndex, alias, true, indexCreatedListener);
                return;
            }
            logger.error(
                "There are no indices matching '{}' pattern but '{}' alias points at [{}]. This should never happen.",
                indexPattern, alias, indexPointedByCurrentWriteAlias.get());
        } else if (concreteIndexNames.length == 1 && concreteIndexNames[0].equals(legacyIndexWithoutSuffix)) {
            if (indexPointedByCurrentWriteAlias.isEmpty()) {
                createFirstConcreteIndex(client, firstConcreteIndex, alias, true, indexCreatedListener);
                return;
            }
            if (indexPointedByCurrentWriteAlias.get().getIndex().getName().equals(legacyIndexWithoutSuffix)) {
                createFirstConcreteIndex(
                    client,
                    firstConcreteIndex,
                    alias,
                    false,
                    ActionListener.wrap(
                        unused -> updateWriteAlias(client, alias, legacyIndexWithoutSuffix, firstConcreteIndex, indexCreatedListener),
                        loggingListener::onFailure)
                );
                return;
            }
            logger.error(
                "There is exactly one index (i.e. '{}') matching '{}' pattern but '{}' alias points at [{}]. This should never happen.",
                legacyIndexWithoutSuffix, indexPattern, alias, indexPointedByCurrentWriteAlias.get());
        } else {
            if (indexPointedByCurrentWriteAlias.isEmpty()) {
                assert concreteIndexNames.length > 0;
                String latestConcreteIndexName = Arrays.stream(concreteIndexNames).max(INDEX_NAME_COMPARATOR).get();
                updateWriteAlias(client, alias, null, latestConcreteIndexName, loggingListener);
                return;
            }
        }
        // If the alias is set, there is nothing more to do.
        loggingListener.onResponse(false);
    }

    public static void createSystemIndexIfNecessary(Client client,
                                                    ClusterState clusterState,
                                                    SystemIndexDescriptor descriptor,
                                                    TimeValue masterNodeTimeout,
                                                    ActionListener<Boolean> finalListener) {

        final String primaryIndex = descriptor.getPrimaryIndex();

        // The check for existence of the index is against the cluster state, so very cheap
        if (hasIndex(clusterState, primaryIndex)) {
            finalListener.onResponse(true);
            return;
        }

        ActionListener<Boolean> indexCreatedListener = ActionListener.wrap(
            created -> {
                if (created) {
                    waitForShardsReady(client, primaryIndex, masterNodeTimeout, finalListener);
                } else {
                    finalListener.onResponse(false);
                }
            },
            e -> {
                if (ExceptionsHelper.unwrapCause(e) instanceof ResourceAlreadyExistsException) {
                    finalListener.onResponse(true);
                } else {
                    finalListener.onFailure(e);
                }
            }
        );

        CreateIndexRequest createIndexRequest = new CreateIndexRequest(primaryIndex);
        createIndexRequest.settings(descriptor.getSettings());
        createIndexRequest.mapping(descriptor.getMappings());
        createIndexRequest.origin(ML_ORIGIN);
        createIndexRequest.masterNodeTimeout(masterNodeTimeout);

        executeAsyncWithOrigin(client.threadPool().getThreadContext(), ML_ORIGIN, createIndexRequest,
            ActionListener.<CreateIndexResponse>wrap(
                r -> indexCreatedListener.onResponse(r.isAcknowledged()),
                indexCreatedListener::onFailure
            ), client.admin().indices()::create);
    }

    private static void waitForShardsReady(Client client, String index, TimeValue masterNodeTimeout, ActionListener<Boolean> listener) {
        ClusterHealthRequest healthRequest = Requests.clusterHealthRequest(index)
            .waitForYellowStatus()
            .waitForNoRelocatingShards(true)
            .waitForNoInitializingShards(true)
            .masterNodeTimeout(masterNodeTimeout);
        executeAsyncWithOrigin(
            client.threadPool().getThreadContext(),
            ML_ORIGIN,
            healthRequest,
            ActionListener.<ClusterHealthResponse>wrap(
                response -> listener.onResponse(response.isTimedOut() == false),
                listener::onFailure),
            client.admin().cluster()::health
        );
    }

    private static void createFirstConcreteIndex(Client client,
                                                 String index,
                                                 String alias,
                                                 boolean addAlias,
                                                 ActionListener<Boolean> listener) {
        logger.info("About to create first concrete index [{}] with alias [{}]", index, alias);
        CreateIndexRequestBuilder requestBuilder = client.admin()
            .indices()
            .prepareCreate(index);
        if (addAlias) {
            requestBuilder.addAlias(new Alias(alias).isHidden(true));
        }
        CreateIndexRequest request = requestBuilder.request();

        executeAsyncWithOrigin(client.threadPool().getThreadContext(),
            ML_ORIGIN,
            request,
            ActionListener.<CreateIndexResponse>wrap(
                createIndexResponse -> listener.onResponse(true),
                createIndexFailure -> {
                    if (ExceptionsHelper.unwrapCause(createIndexFailure) instanceof ResourceAlreadyExistsException) {
                        // If it was created between our last check and this request being handled, we should add the alias
                        // if we were asked to add it on creation.  Adding an alias that already exists is idempotent. So
                        // no need to double check if the alias exists as well.  But if we weren't asked to add the alias
                        // on creation then we should leave it up to the caller to decide what to do next (some call sites
                        // already have more advanced alias update logic in their success handlers).
                        if (addAlias) {
                            updateWriteAlias(client, alias, null, index, listener);
                        } else {
                            listener.onResponse(true);
                        }
                    } else {
                        listener.onFailure(createIndexFailure);
                    }
                }),
            client.admin().indices()::create);
    }

    private static void updateWriteAlias(Client client,
                                         String alias,
                                         @Nullable String currentIndex,
                                         String newIndex,
                                         ActionListener<Boolean> listener) {
        logger.info("About to move write alias [{}] from index [{}] to index [{}]", alias, currentIndex, newIndex);
        IndicesAliasesRequestBuilder requestBuilder = client.admin()
            .indices()
            .prepareAliases()
            .addAliasAction(IndicesAliasesRequest.AliasActions.add().index(newIndex).alias(alias).isHidden(true));
        if (currentIndex != null) {
            requestBuilder.removeAlias(currentIndex, alias);
        }
        IndicesAliasesRequest request = requestBuilder.request();

        executeAsyncWithOrigin(client.threadPool().getThreadContext(),
            ML_ORIGIN,
            request,
            ActionListener.<AcknowledgedResponse>wrap(
                resp -> listener.onResponse(resp.isAcknowledged()),
                listener::onFailure),
            client.admin().indices()::aliases);
    }

    /**
     * Installs the index template specified by {@code templateConfig} if it is not in already
     * installed in {@code clusterState}.
     *
     * The check for presence is simple and will return the listener on
     * the calling thread if successful. If the template has to be installed
     * an async call will be made.
     *
     * @param clusterState The cluster state
     * @param client For putting the template
     * @param templateConfig The config
     * @param listener Async listener
     */
    public static void installIndexTemplateIfRequired(
        ClusterState clusterState,
        Client client,
        Version versionComposableTemplateExpected,
        IndexTemplateConfig legacyTemplateConfig,
        IndexTemplateConfig templateConfig,
        TimeValue masterTimeout,
        ActionListener<Boolean> listener
    ) {
        String legacyTemplateName = legacyTemplateConfig.getTemplateName();
        String templateName = templateConfig.getTemplateName();

        // The check for existence of the template is against the cluster state, so very cheap
        if (hasIndexTemplate(clusterState, legacyTemplateName, templateName, versionComposableTemplateExpected)) {
            listener.onResponse(true);
            return;
        }

        PutIndexTemplateRequest legacyRequest = new PutIndexTemplateRequest(legacyTemplateName)
            .source(legacyTemplateConfig.loadBytes(), XContentType.JSON).masterNodeTimeout(masterTimeout);

        PutComposableIndexTemplateAction.Request request;
        try {
            request = new PutComposableIndexTemplateAction.Request(templateConfig.getTemplateName())
                .indexTemplate(ComposableIndexTemplate.parse(JsonXContent.jsonXContent.createParser(NamedXContentRegistry.EMPTY,
                    DeprecationHandler.THROW_UNSUPPORTED_OPERATION, templateConfig.loadBytes())))
                .masterNodeTimeout(masterTimeout);
        } catch (IOException e) {
            throw new ElasticsearchParseException("unable to parse composable template " + templateConfig.getTemplateName(), e);
        }

        installIndexTemplateIfRequired(clusterState, client, versionComposableTemplateExpected, legacyRequest, request, listener);
    }

    /**
     * See {@link #installIndexTemplateIfRequired(ClusterState, Client, Version, IndexTemplateConfig, IndexTemplateConfig, TimeValue,
     * ActionListener)}.
     *
     * Overload takes a {@code PutIndexTemplateRequest} instead of {@code IndexTemplateConfig}
     *
     * @param clusterState The cluster state
     * @param client For putting the template
     * @param templateRequest The Put template request
     * @param listener Async listener
     */
    public static void installIndexTemplateIfRequired(
        ClusterState clusterState,
        Client client,
        Version versionComposableTemplateExpected,
        PutIndexTemplateRequest legacyTemplateRequest,
        PutComposableIndexTemplateAction.Request templateRequest,
        ActionListener<Boolean> listener
    ) {
        // The check for existence of the template is against the cluster state, so very cheap
        if (hasIndexTemplate(clusterState, legacyTemplateRequest.name(), templateRequest.name(), versionComposableTemplateExpected)) {
            listener.onResponse(true);
            return;
        }

        if (versionComposableTemplateExpected != null &&
            clusterState.nodes().getMinNodeVersion().onOrAfter(versionComposableTemplateExpected)) {
            ActionListener<AcknowledgedResponse> innerListener = ActionListener.wrap(
                response -> {
                    if (response.isAcknowledged() == false) {
                        logger.warn("error adding template [{}], request was not acknowledged", templateRequest.name());
                    }
                    listener.onResponse(response.isAcknowledged());
                },
                listener::onFailure);

            executeAsyncWithOrigin(client, ML_ORIGIN, PutComposableIndexTemplateAction.INSTANCE, templateRequest, innerListener);
        } else {
            ActionListener<AcknowledgedResponse> innerListener = ActionListener.wrap(
                response -> {
                    if (response.isAcknowledged() == false) {
                        logger.warn("error adding legacy template [{}], request was not acknowledged", legacyTemplateRequest.name());
                    }
                    listener.onResponse(response.isAcknowledged());
                },
                listener::onFailure);

            executeAsyncWithOrigin(client.threadPool().getThreadContext(), ML_ORIGIN, legacyTemplateRequest, innerListener,
                client.admin().indices()::putTemplate);
        }
    }

    public static boolean hasIndexTemplate(ClusterState state, String legacyTemplateName,
                                           String templateName, Version versionComposableTemplateExpected) {
        if (versionComposableTemplateExpected != null && state.nodes().getMinNodeVersion().onOrAfter(versionComposableTemplateExpected)) {
            return state.getMetadata().templatesV2().containsKey(templateName);
        } else {
            return state.getMetadata().getTemplates().containsKey(legacyTemplateName);
        }
    }

    public static boolean hasIndex(ClusterState state, String index) {
        return state.getMetadata().getIndicesLookup().containsKey(index);
    }
}
