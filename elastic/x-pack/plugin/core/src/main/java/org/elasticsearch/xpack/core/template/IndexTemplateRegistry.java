/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.template;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.template.put.PutComponentTemplateAction;
import org.elasticsearch.action.admin.indices.template.put.PutComposableIndexTemplateAction;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateRequest;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateListener;
import org.elasticsearch.cluster.metadata.ComponentTemplate;
import org.elasticsearch.cluster.metadata.ComposableIndexTemplate;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.gateway.GatewayService;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentParserConfiguration;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xcontent.json.JsonXContent;
import org.elasticsearch.xpack.core.ilm.IndexLifecycleMetadata;
import org.elasticsearch.xpack.core.ilm.LifecyclePolicy;
import org.elasticsearch.xpack.core.ilm.action.PutLifecycleAction;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.Executor;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.stream.Collectors;

import static org.elasticsearch.core.Strings.format;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

/**
 * Abstracts the logic of managing versioned index templates and lifecycle policies for plugins that require such things.
 */
public abstract class IndexTemplateRegistry implements ClusterStateListener {
    private static final Logger logger = LogManager.getLogger(IndexTemplateRegistry.class);

    protected final Settings settings;
    protected final Client client;
    protected final ThreadPool threadPool;
    protected final NamedXContentRegistry xContentRegistry;
    protected final ClusterService clusterService;
    protected final ConcurrentMap<String, AtomicBoolean> templateCreationsInProgress = new ConcurrentHashMap<>();
    protected final ConcurrentMap<String, AtomicBoolean> policyCreationsInProgress = new ConcurrentHashMap<>();

    public IndexTemplateRegistry(
        Settings nodeSettings,
        ClusterService clusterService,
        ThreadPool threadPool,
        Client client,
        NamedXContentRegistry xContentRegistry
    ) {
        this.settings = nodeSettings;
        this.client = client;
        this.threadPool = threadPool;
        this.xContentRegistry = xContentRegistry;
        this.clusterService = clusterService;
    }

    /**
     * Initialize the template registry, adding it as a listener so templates will be installed as necessary
     */
    public void initialize() {
        clusterService.addListener(this);
    }

    /**
     * Retrieves return a list of {@link IndexTemplateConfig} that represents
     * the index templates that should be installed and managed.
     * @return The configurations for the templates that should be installed.
     */
    protected List<IndexTemplateConfig> getLegacyTemplateConfigs() {
        return Collections.emptyList();
    }

    /**
     * Retrieves return a list of {@link IndexTemplateConfig} that represents
     * the component templates that should be installed and managed. Component
     * templates are always installed prior composable templates, so they may
     * be referenced by a composable template.
     * @return The configurations for the templates that should be installed.
     */
    protected Map<String, ComponentTemplate> getComponentTemplateConfigs() {
        return Map.of();
    }

    /**
     * Retrieves return a list of {@link IndexTemplateConfig} that represents
     * the composable templates that should be installed and managed.
     * @return The configurations for the templates that should be installed.
     */
    protected Map<String, ComposableIndexTemplate> getComposableTemplateConfigs() {
        return Map.of();
    }

    /**
     * Retrieves a list of {@link LifecyclePolicy} that represents the ILM
     * policies that should be installed and managed. Only called if ILM is enabled.
     * @return The configurations for the lifecycle policies that should be installed.
     */
    protected List<LifecyclePolicy> getPolicyConfigs() {
        return Collections.emptyList();
    }

    /**
     * Retrieves an identifier that is used to identify which plugin is asking for this.
     * @return A string ID for the plugin managing these templates.
     */
    protected abstract String getOrigin();

    /**
     * Called when creation of an index template fails.
     * @param templateName the template name that failed to be created.
     * @param e The exception that caused the failure.
     */
    protected void onPutTemplateFailure(String templateName, Exception e) {
        logger.error(() -> format("error adding index template [%s] for [%s]", templateName, getOrigin()), e);
    }

    /**
     * Called when creation of a lifecycle policy fails.
     * @param policy The lifecycle policy that failed to be created.
     * @param e The exception that caused the failure.
     */
    protected void onPutPolicyFailure(LifecyclePolicy policy, Exception e) {
        logger.error(() -> format("error adding lifecycle policy [%s] for [%s]", policy.getName(), getOrigin()), e);
    }

    @Override
    public void clusterChanged(ClusterChangedEvent event) {
        ClusterState state = event.state();
        if (state.blocks().hasGlobalBlock(GatewayService.STATE_NOT_RECOVERED_BLOCK)) {
            // wait until the gateway has recovered from disk, otherwise we think may not have the index templates,
            // while they actually do exist
            return;
        }

        // no master node, exit immediately
        DiscoveryNode masterNode = event.state().getNodes().getMasterNode();
        if (masterNode == null) {
            return;
        }

        // This registry requires to run on a master node.
        // If not a master node, exit.
        if (requiresMasterNode() && state.nodes().isLocalNodeElectedMaster() == false) {
            return;
        }

        // if this node is newer than the master node, we probably need to add the template, which might be newer than the
        // template the master node has, so we need potentially add new templates despite being not the master node
        DiscoveryNode localNode = event.state().getNodes().getLocalNode();
        boolean localNodeVersionAfterMaster = localNode.getVersion().after(masterNode.getVersion());

        if (event.localNodeMaster() || localNodeVersionAfterMaster) {
            addTemplatesIfMissing(state);
            addIndexLifecyclePoliciesIfMissing(state);
        }
    }

    /**
     * Whether the registry should only apply changes when running on the master node.
     * This is useful for plugins where certain actions are performed on master nodes
     * and the templates should match the respective version.
     */
    protected boolean requiresMasterNode() {
        return false;
    }

    private void addTemplatesIfMissing(ClusterState state) {
        addLegacyTemplatesIfMissing(state);
        addComponentTemplatesIfMissing(state);
        addComposableTemplatesIfMissing(state);
    }

    private void addLegacyTemplatesIfMissing(ClusterState state) {
        final List<IndexTemplateConfig> indexTemplates = getLegacyTemplateConfigs();
        for (IndexTemplateConfig newTemplate : indexTemplates) {
            final String templateName = newTemplate.getTemplateName();
            final AtomicBoolean creationCheck = templateCreationsInProgress.computeIfAbsent(templateName, key -> new AtomicBoolean(false));
            if (creationCheck.compareAndSet(false, true)) {
                IndexTemplateMetadata currentTemplate = state.metadata().getTemplates().get(templateName);
                if (Objects.isNull(currentTemplate)) {
                    logger.debug("adding legacy template [{}] for [{}], because it doesn't exist", templateName, getOrigin());
                    putLegacyTemplate(newTemplate, creationCheck);
                } else if (Objects.isNull(currentTemplate.getVersion()) || newTemplate.getVersion() > currentTemplate.getVersion()) {
                    // IndexTemplateConfig now enforces templates contain a `version` property, so if the template doesn't have one we can
                    // safely assume it's an old version of the template.
                    logger.info(
                        "upgrading legacy template [{}] for [{}] from version [{}] to version [{}]",
                        templateName,
                        getOrigin(),
                        currentTemplate.getVersion(),
                        newTemplate.getVersion()
                    );
                    putLegacyTemplate(newTemplate, creationCheck);
                } else {
                    creationCheck.set(false);
                    logger.trace(
                        "not adding legacy template [{}] for [{}], because it already exists at version [{}]",
                        templateName,
                        getOrigin(),
                        currentTemplate.getVersion()
                    );
                }
            } else {
                logger.trace(
                    "skipping the creation of legacy template [{}] for [{}], because its creation is in progress",
                    templateName,
                    getOrigin()
                );
            }
        }
    }

    private void addComponentTemplatesIfMissing(ClusterState state) {
        final Map<String, ComponentTemplate> indexTemplates = getComponentTemplateConfigs();
        for (Map.Entry<String, ComponentTemplate> newTemplate : indexTemplates.entrySet()) {
            final String templateName = newTemplate.getKey();
            final AtomicBoolean creationCheck = templateCreationsInProgress.computeIfAbsent(templateName, key -> new AtomicBoolean(false));
            if (creationCheck.compareAndSet(false, true)) {
                ComponentTemplate currentTemplate = state.metadata().componentTemplates().get(templateName);
                if (Objects.isNull(currentTemplate)) {
                    logger.debug("adding component template [{}] for [{}], because it doesn't exist", templateName, getOrigin());
                    putComponentTemplate(templateName, newTemplate.getValue(), creationCheck);
                } else if (Objects.isNull(currentTemplate.version()) || newTemplate.getValue().version() > currentTemplate.version()) {
                    // IndexTemplateConfig now enforces templates contain a `version` property, so if the template doesn't have one we can
                    // safely assume it's an old version of the template.
                    logger.info(
                        "upgrading component template [{}] for [{}] from version [{}] to version [{}]",
                        templateName,
                        getOrigin(),
                        currentTemplate.version(),
                        newTemplate.getValue().version()
                    );
                    putComponentTemplate(templateName, newTemplate.getValue(), creationCheck);
                } else {
                    creationCheck.set(false);
                    logger.trace(
                        "not adding component template [{}] for [{}], because it already exists at version [{}]",
                        templateName,
                        getOrigin(),
                        currentTemplate.version()
                    );
                }
            } else {
                logger.trace(
                    "skipping the creation of component template [{}] for [{}], because its creation is in progress",
                    templateName,
                    getOrigin()
                );
            }
        }
    }

    private void addComposableTemplatesIfMissing(ClusterState state) {
        final Map<String, ComposableIndexTemplate> indexTemplates = getComposableTemplateConfigs();
        for (Map.Entry<String, ComposableIndexTemplate> newTemplate : indexTemplates.entrySet()) {
            final String templateName = newTemplate.getKey();
            final AtomicBoolean creationCheck = templateCreationsInProgress.computeIfAbsent(templateName, key -> new AtomicBoolean(false));
            if (creationCheck.compareAndSet(false, true)) {
                ComposableIndexTemplate currentTemplate = state.metadata().templatesV2().get(templateName);
                boolean componentTemplatesAvailable = componentTemplatesExist(state, newTemplate.getValue());
                if (componentTemplatesAvailable == false) {
                    creationCheck.set(false);
                    logger.trace(
                        "not adding composable template [{}] for [{}] because its required component templates do not exist",
                        templateName,
                        getOrigin()
                    );
                } else if (Objects.isNull(currentTemplate)) {
                    logger.debug("adding composable template [{}] for [{}], because it doesn't exist", templateName, getOrigin());
                    putComposableTemplate(templateName, newTemplate.getValue(), creationCheck);
                } else if (Objects.isNull(currentTemplate.version()) || newTemplate.getValue().version() > currentTemplate.version()) {
                    // IndexTemplateConfig now enforces templates contain a `version` property, so if the template doesn't have one we can
                    // safely assume it's an old version of the template.
                    logger.info(
                        "upgrading composable template [{}] for [{}] from version [{}] to version [{}]",
                        templateName,
                        getOrigin(),
                        currentTemplate.version(),
                        newTemplate.getValue().version()
                    );
                    putComposableTemplate(templateName, newTemplate.getValue(), creationCheck);
                } else {
                    creationCheck.set(false);
                    logger.trace(
                        "not adding composable template [{}] for [{}], because it already exists at version [{}]",
                        templateName,
                        getOrigin(),
                        currentTemplate.version()
                    );
                }
            } else {
                logger.trace(
                    "skipping the creation of composable template [{}] for [{}], because its creation is in progress",
                    templateName,
                    getOrigin()
                );
            }
        }
    }

    /**
     * Returns true if the cluster state contains all of the component templates needed by the composable template
     */
    private static boolean componentTemplatesExist(ClusterState state, ComposableIndexTemplate indexTemplate) {
        return state.metadata().componentTemplates().keySet().containsAll(indexTemplate.composedOf());
    }

    private void putLegacyTemplate(final IndexTemplateConfig config, final AtomicBoolean creationCheck) {
        final Executor executor = threadPool.generic();
        executor.execute(() -> {
            final String templateName = config.getTemplateName();

            PutIndexTemplateRequest request = new PutIndexTemplateRequest(templateName).source(config.loadBytes(), XContentType.JSON);
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1));
            executeAsyncWithOrigin(
                client.threadPool().getThreadContext(),
                getOrigin(),
                request,
                new ActionListener<AcknowledgedResponse>() {
                    @Override
                    public void onResponse(AcknowledgedResponse response) {
                        creationCheck.set(false);
                        if (response.isAcknowledged() == false) {
                            logger.error(
                                "error adding legacy template [{}] for [{}], request was not acknowledged",
                                templateName,
                                getOrigin()
                            );
                        }
                    }

                    @Override
                    public void onFailure(Exception e) {
                        creationCheck.set(false);
                        onPutTemplateFailure(templateName, e);
                    }
                },
                client.admin().indices()::putTemplate
            );
        });
    }

    private void putComponentTemplate(final String templateName, final ComponentTemplate template, final AtomicBoolean creationCheck) {
        final Executor executor = threadPool.generic();
        executor.execute(() -> {
            PutComponentTemplateAction.Request request = new PutComponentTemplateAction.Request(templateName).componentTemplate(template);
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1));
            executeAsyncWithOrigin(
                client.threadPool().getThreadContext(),
                getOrigin(),
                request,
                new ActionListener<AcknowledgedResponse>() {
                    @Override
                    public void onResponse(AcknowledgedResponse response) {
                        creationCheck.set(false);
                        if (response.isAcknowledged() == false) {
                            logger.error(
                                "error adding component template [{}] for [{}], request was not acknowledged",
                                templateName,
                                getOrigin()
                            );
                        }
                    }

                    @Override
                    public void onFailure(Exception e) {
                        creationCheck.set(false);
                        onPutTemplateFailure(templateName, e);
                    }
                },
                (req, listener) -> client.execute(PutComponentTemplateAction.INSTANCE, req, listener)
            );
        });
    }

    private void putComposableTemplate(
        final String templateName,
        final ComposableIndexTemplate indexTemplate,
        final AtomicBoolean creationCheck
    ) {
        final Executor executor = threadPool.generic();
        executor.execute(() -> {
            PutComposableIndexTemplateAction.Request request = new PutComposableIndexTemplateAction.Request(templateName).indexTemplate(
                indexTemplate
            );
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1));
            executeAsyncWithOrigin(
                client.threadPool().getThreadContext(),
                getOrigin(),
                request,
                new ActionListener<AcknowledgedResponse>() {
                    @Override
                    public void onResponse(AcknowledgedResponse response) {
                        creationCheck.set(false);
                        if (response.isAcknowledged() == false) {
                            logger.error(
                                "error adding composable template [{}] for [{}], request was not acknowledged",
                                templateName,
                                getOrigin()
                            );
                        }
                    }

                    @Override
                    public void onFailure(Exception e) {
                        creationCheck.set(false);
                        onPutTemplateFailure(templateName, e);
                    }
                },
                (req, listener) -> client.execute(PutComposableIndexTemplateAction.INSTANCE, req, listener)
            );
        });
    }

    private void addIndexLifecyclePoliciesIfMissing(ClusterState state) {
        Optional<IndexLifecycleMetadata> maybeMeta = Optional.ofNullable(state.metadata().custom(IndexLifecycleMetadata.TYPE));
        for (LifecyclePolicy policy : getPolicyConfigs()) {
            final AtomicBoolean creationCheck = policyCreationsInProgress.computeIfAbsent(
                policy.getName(),
                key -> new AtomicBoolean(false)
            );
            if (creationCheck.compareAndSet(false, true)) {
                final boolean policyNeedsToBeCreated = maybeMeta.flatMap(
                    ilmMeta -> Optional.ofNullable(ilmMeta.getPolicies().get(policy.getName()))
                ).isPresent() == false;
                if (policyNeedsToBeCreated) {
                    logger.debug("adding lifecycle policy [{}] for [{}], because it doesn't exist", policy.getName(), getOrigin());
                    putPolicy(policy, creationCheck);
                } else {
                    logger.trace("not adding lifecycle policy [{}] for [{}], because it already exists", policy.getName(), getOrigin());
                    creationCheck.set(false);
                }
            }
        }
    }

    private void putPolicy(final LifecyclePolicy policy, final AtomicBoolean creationCheck) {
        final Executor executor = threadPool.generic();
        executor.execute(() -> {
            PutLifecycleAction.Request request = new PutLifecycleAction.Request(policy);
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1));
            executeAsyncWithOrigin(
                client.threadPool().getThreadContext(),
                getOrigin(),
                request,
                new ActionListener<AcknowledgedResponse>() {
                    @Override
                    public void onResponse(AcknowledgedResponse response) {
                        creationCheck.set(false);
                        if (response.isAcknowledged() == false) {
                            logger.error(
                                "error adding lifecycle policy [{}] for [{}], request was not acknowledged",
                                policy.getName(),
                                getOrigin()
                            );
                        }
                    }

                    @Override
                    public void onFailure(Exception e) {
                        creationCheck.set(false);
                        onPutPolicyFailure(policy, e);
                    }
                },
                (req, listener) -> client.execute(PutLifecycleAction.INSTANCE, req, listener)
            );
        });
    }

    protected static Map<String, ComposableIndexTemplate> parseComposableTemplates(IndexTemplateConfig... config) {
        return Arrays.stream(config).collect(Collectors.toUnmodifiableMap(IndexTemplateConfig::getTemplateName, indexTemplateConfig -> {
            try {
                return ComposableIndexTemplate.parse(
                    JsonXContent.jsonXContent.createParser(XContentParserConfiguration.EMPTY, indexTemplateConfig.loadBytes())
                );
            } catch (IOException e) {
                throw new AssertionError(e);
            }
        }));
    }

}
