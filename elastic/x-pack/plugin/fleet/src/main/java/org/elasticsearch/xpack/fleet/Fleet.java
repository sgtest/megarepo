/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.fleet;

import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.admin.cluster.snapshots.features.ResetFeatureStateResponse.ResetFeatureStateStatus;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateRequest;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.IndicesOptions.Option;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.ComposableIndexTemplate;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.indices.SystemDataStreamDescriptor;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.indices.SystemIndexDescriptor.Type;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.SystemIndexPlugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.action.DeleteDataStreamAction;
import org.elasticsearch.xpack.core.action.DeleteDataStreamAction.Request;
import org.elasticsearch.xpack.core.template.TemplateUtils;
import org.elasticsearch.xpack.fleet.action.GetGlobalCheckpointsAction;
import org.elasticsearch.xpack.fleet.action.GetGlobalCheckpointsShardAction;
import org.elasticsearch.xpack.fleet.rest.RestGetGlobalCheckpointsAction;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.EnumSet;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.core.ClientHelper.FLEET_ORIGIN;

/**
 * A plugin to manage and provide access to the system indices used by Fleet.
 *
 * Currently only exposes general-purpose APIs on {@code _fleet}-prefixed routes, to be more specialized as Fleet's requirements stabilize.
 */
public class Fleet extends Plugin implements SystemIndexPlugin {

    private static final int CURRENT_INDEX_VERSION = 7;
    private static final String VERSION_KEY = "version";
    private static final String MAPPING_VERSION_VARIABLE = "fleet.version";
    private static final List<String> ALLOWED_PRODUCTS = List.of("kibana", "fleet");

    @Override
    public Collection<Object> createComponents(
        Client client,
        ClusterService clusterService,
        ThreadPool threadPool,
        ResourceWatcherService resourceWatcherService,
        ScriptService scriptService,
        NamedXContentRegistry xContentRegistry,
        Environment environment,
        NodeEnvironment nodeEnvironment,
        NamedWriteableRegistry namedWriteableRegistry,
        IndexNameExpressionResolver expressionResolver,
        Supplier<RepositoriesService> repositoriesServiceSupplier
    ) {
        FleetTemplateRegistry registry = new FleetTemplateRegistry(
            environment.settings(),
            clusterService,
            threadPool,
            client,
            xContentRegistry
        );
        registry.initialize();
        return List.of();
    }

    @Override
    public Collection<SystemIndexDescriptor> getSystemIndexDescriptors(Settings settings) {
        return List.of(
            fleetActionsSystemIndexDescriptor(),
            fleetAgentsSystemIndexDescriptor(),
            fleetEnrollmentApiKeysSystemIndexDescriptor(),
            fleetPoliciesSystemIndexDescriptor(),
            fleetPoliciesLeaderSystemIndexDescriptor(),
            fleetServersSystemIndexDescriptors(),
            fleetArtifactsSystemIndexDescriptors()
        );
    }

    @Override
    public Collection<SystemDataStreamDescriptor> getSystemDataStreamDescriptors() {
        return List.of(fleetActionsResultsDescriptor());
    }

    @Override
    public String getFeatureName() {
        return "fleet";
    }

    @Override
    public String getFeatureDescription() {
        return "Manages configuration for Fleet";
    }

    private SystemIndexDescriptor fleetActionsSystemIndexDescriptor() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-actions.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-actions-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-actions~(-results*)")
            .setAliasName(".fleet-actions")
            .setDescription("Fleet agents")
            .build();
    }

    private SystemIndexDescriptor fleetAgentsSystemIndexDescriptor() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-agents.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-agents-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-agents*")
            .setAliasName(".fleet-agents")
            .setDescription("Configuration of fleet servers")
            .build();
    }

    private SystemIndexDescriptor fleetEnrollmentApiKeysSystemIndexDescriptor() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-enrollment-api-keys.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-enrollment-api-keys-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-enrollment-api-keys*")
            .setAliasName(".fleet-enrollment-api-keys")
            .setDescription("Fleet API Keys for enrollment")
            .build();
    }

    private SystemIndexDescriptor fleetPoliciesSystemIndexDescriptor() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-policies.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-policies-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-policies-[0-9]+*")
            .setAliasName(".fleet-policies")
            .setDescription("Fleet Policies")
            .build();
    }

    private SystemIndexDescriptor fleetPoliciesLeaderSystemIndexDescriptor() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-policies-leader.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-policies-leader-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-policies-leader*")
            .setAliasName(".fleet-policies-leader")
            .setDescription("Fleet Policies leader")
            .build();
    }

    private SystemIndexDescriptor fleetServersSystemIndexDescriptors() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-servers.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-servers-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-servers*")
            .setAliasName(".fleet-servers")
            .setDescription("Fleet servers")
            .build();
    }

    private SystemIndexDescriptor fleetArtifactsSystemIndexDescriptors() {
        PutIndexTemplateRequest request = new PutIndexTemplateRequest();
        request.source(loadTemplateSource("/fleet-artifacts.json"), XContentType.JSON);

        return SystemIndexDescriptor.builder()
            .setType(Type.EXTERNAL_MANAGED)
            .setAllowedElasticProductOrigins(ALLOWED_PRODUCTS)
            .setOrigin(FLEET_ORIGIN)
            .setVersionMetaKey(VERSION_KEY)
            .setMappings(request.mappings())
            .setSettings(request.settings())
            .setPrimaryIndex(".fleet-artifacts-" + CURRENT_INDEX_VERSION)
            .setIndexPattern(".fleet-artifacts*")
            .setAliasName(".fleet-artifacts")
            .setDescription("Fleet artifacts")
            .build();
    }

    private SystemDataStreamDescriptor fleetActionsResultsDescriptor() {
        final String source = loadTemplateSource("/fleet-actions-results.json");
        try (
            XContentParser parser = XContentType.JSON.xContent()
                .createParser(NamedXContentRegistry.EMPTY, DeprecationHandler.THROW_UNSUPPORTED_OPERATION, source)
        ) {
            ComposableIndexTemplate composableIndexTemplate = ComposableIndexTemplate.parse(parser);
            return new SystemDataStreamDescriptor(
                ".fleet-actions-results",
                "Result history of fleet actions",
                SystemDataStreamDescriptor.Type.EXTERNAL,
                composableIndexTemplate,
                Map.of(),
                ALLOWED_PRODUCTS
            );
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    @Override
    public void cleanUpFeature(ClusterService clusterService, Client client, ActionListener<ResetFeatureStateStatus> listener) {
        Collection<SystemDataStreamDescriptor> dataStreamDescriptors = getSystemDataStreamDescriptors();
        if (dataStreamDescriptors.isEmpty() == false) {
            try {
                Request request = new Request(
                    dataStreamDescriptors.stream()
                        .map(SystemDataStreamDescriptor::getDataStreamName)
                        .collect(Collectors.toList())
                        .toArray(Strings.EMPTY_ARRAY)
                );
                EnumSet<Option> options = request.indicesOptions().getOptions();
                options.add(Option.IGNORE_UNAVAILABLE);
                options.add(Option.ALLOW_NO_INDICES);
                request.indicesOptions(new IndicesOptions(options, request.indicesOptions().getExpandWildcards()));

                client.execute(
                    DeleteDataStreamAction.INSTANCE,
                    request,
                    ActionListener.wrap(response -> SystemIndexPlugin.super.cleanUpFeature(clusterService, client, listener), e -> {
                        Throwable unwrapped = ExceptionsHelper.unwrapCause(e);
                        if (unwrapped instanceof ResourceNotFoundException) {
                            SystemIndexPlugin.super.cleanUpFeature(clusterService, client, listener);
                        } else {
                            listener.onFailure(e);
                        }
                    })
                );
            } catch (Exception e) {
                Throwable unwrapped = ExceptionsHelper.unwrapCause(e);
                if (unwrapped instanceof ResourceNotFoundException) {
                    SystemIndexPlugin.super.cleanUpFeature(clusterService, client, listener);
                } else {
                    listener.onFailure(e);
                }
            }
        } else {
            SystemIndexPlugin.super.cleanUpFeature(clusterService, client, listener);
        }
    }

    private String loadTemplateSource(String resource) {
        return TemplateUtils.loadTemplate(resource, Version.CURRENT.toString(), MAPPING_VERSION_VARIABLE);
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        return Arrays.asList(
            new ActionHandler<>(GetGlobalCheckpointsAction.INSTANCE, GetGlobalCheckpointsAction.TransportAction.class),
            new ActionHandler<>(GetGlobalCheckpointsShardAction.INSTANCE, GetGlobalCheckpointsShardAction.TransportAction.class)
        );
    }

    @Override
    public List<RestHandler> getRestHandlers(
        Settings settings,
        RestController restController,
        ClusterSettings clusterSettings,
        IndexScopedSettings indexScopedSettings,
        SettingsFilter settingsFilter,
        IndexNameExpressionResolver indexNameExpressionResolver,
        Supplier<DiscoveryNodes> nodesInCluster
    ) {
        return Collections.singletonList(new RestGetGlobalCheckpointsAction());
    }
}
