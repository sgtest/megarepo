/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.dlm;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.client.internal.OriginSettingClient;
import org.elasticsearch.cluster.metadata.DataLifecycle;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.routing.allocation.AllocationService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.IOUtils;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.tracing.Tracer;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xcontent.NamedXContentRegistry;

import java.io.IOException;
import java.time.Clock;
import java.util.Collection;
import java.util.List;
import java.util.function.Supplier;

import static org.elasticsearch.cluster.metadata.DataLifecycle.DLM_ORIGIN;

/**
 * Plugin encapsulating Data Lifecycle Management Service.
 */
public class DataLifecyclePlugin extends Plugin implements ActionPlugin {

    private final Settings settings;
    private final SetOnce<DataLifecycleService> dataLifecycleInitialisationService = new SetOnce<>();

    public DataLifecyclePlugin(Settings settings) {
        this.settings = settings;
    }

    protected Clock getClock() {
        return Clock.systemUTC();
    }

    @Override
    public List<NamedWriteableRegistry.Entry> getNamedWriteables() {
        return List.of();
    }

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
        IndexNameExpressionResolver indexNameExpressionResolver,
        Supplier<RepositoriesService> repositoriesServiceSupplier,
        Tracer tracer,
        AllocationService allocationService
    ) {
        if (DataLifecycle.isEnabled() == false) {
            return List.of();
        }

        dataLifecycleInitialisationService.set(
            new DataLifecycleService(
                settings,
                new OriginSettingClient(client, DLM_ORIGIN),
                clusterService,
                getClock(),
                threadPool,
                threadPool::absoluteTimeInMillis
            )
        );
        dataLifecycleInitialisationService.get().init();
        return List.of(dataLifecycleInitialisationService.get());
    }

    @Override
    public List<Setting<?>> getSettings() {
        if (DataLifecycle.isEnabled() == false) {
            return List.of();
        }

        return List.of(DataLifecycleService.DLM_POLL_INTERVAL_SETTING);
    }

    @Override
    public void close() throws IOException {
        try {
            IOUtils.close(dataLifecycleInitialisationService.get());
        } catch (IOException e) {
            throw new ElasticsearchException("unable to close the data lifecycle service", e);
        }
    }
}
