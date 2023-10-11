/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.inference.external.http;

import org.apache.http.impl.nio.conn.PoolingNHttpClientConnectionManager;
import org.apache.http.impl.nio.reactor.DefaultConnectingIOReactor;
import org.apache.http.nio.reactor.ConnectingIOReactor;
import org.apache.http.nio.reactor.IOReactorException;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.threadpool.ThreadPool;

import java.io.Closeable;
import java.io.IOException;
import java.util.List;

public class HttpClientManager implements Closeable {
    private static final Logger logger = LogManager.getLogger(HttpClientManager.class);
    /**
     * From googling around the connection pools maxTotal value should be close to the number of available threads.
     *
     * https://stackoverflow.com/questions/30989637/how-to-decide-optimal-settings-for-setmaxtotal-and-setdefaultmaxperroute
     */
    static final Setting<Integer> MAX_CONNECTIONS = Setting.intSetting(
        "xpack.inference.http.max_connections",
        // TODO pick a reasonable values here
        20,
        1,
        1000,
        Setting.Property.NodeScope,
        Setting.Property.Dynamic
    );

    private static final TimeValue DEFAULT_CONNECTION_EVICTION_THREAD_INTERVAL_TIME = TimeValue.timeValueSeconds(10);
    static final Setting<TimeValue> CONNECTION_EVICTION_THREAD_INTERVAL_SETTING = Setting.timeSetting(
        "xpack.inference.http.connection_eviction_interval",
        DEFAULT_CONNECTION_EVICTION_THREAD_INTERVAL_TIME,
        Setting.Property.NodeScope,
        Setting.Property.Dynamic
    );

    private static final TimeValue DEFAULT_CONNECTION_EVICTION_MAX_IDLE_TIME_SETTING = DEFAULT_CONNECTION_EVICTION_THREAD_INTERVAL_TIME;
    static final Setting<TimeValue> CONNECTION_EVICTION_MAX_IDLE_TIME_SETTING = Setting.timeSetting(
        "xpack.inference.http.connection_eviction_max_idle_time",
        DEFAULT_CONNECTION_EVICTION_MAX_IDLE_TIME_SETTING,
        Setting.Property.NodeScope,
        Setting.Property.Dynamic
    );

    private final ThreadPool threadPool;
    private final PoolingNHttpClientConnectionManager connectionManager;
    private EvictorSettings evictorSettings;
    private IdleConnectionEvictor connectionEvictor;
    private final HttpClient httpClient;

    public static HttpClientManager create(Settings settings, ThreadPool threadPool, ClusterService clusterService) {
        PoolingNHttpClientConnectionManager connectionManager = createConnectionManager();
        return new HttpClientManager(settings, connectionManager, threadPool, clusterService);
    }

    // Default for testing
    HttpClientManager(
        Settings settings,
        PoolingNHttpClientConnectionManager connectionManager,
        ThreadPool threadPool,
        ClusterService clusterService
    ) {
        this.threadPool = threadPool;

        this.connectionManager = connectionManager;
        setMaxConnections(MAX_CONNECTIONS.get(settings));

        this.httpClient = HttpClient.create(new HttpSettings(settings, clusterService), threadPool, connectionManager);

        evictorSettings = new EvictorSettings(settings);
        connectionEvictor = createConnectionEvictor();

        this.addSettingsUpdateConsumers(clusterService);
    }

    private static PoolingNHttpClientConnectionManager createConnectionManager() {
        ConnectingIOReactor ioReactor;
        try {
            ioReactor = new DefaultConnectingIOReactor();
        } catch (IOReactorException e) {
            var message = "Failed to initialize the inference http client manager";
            logger.error(message, e);
            throw new ElasticsearchException(message, e);
        }

        return new PoolingNHttpClientConnectionManager(ioReactor);
    }

    private void addSettingsUpdateConsumers(ClusterService clusterService) {
        clusterService.getClusterSettings().addSettingsUpdateConsumer(MAX_CONNECTIONS, this::setMaxConnections);
        clusterService.getClusterSettings()
            .addSettingsUpdateConsumer(CONNECTION_EVICTION_THREAD_INTERVAL_SETTING, this::setEvictionInterval);
        clusterService.getClusterSettings().addSettingsUpdateConsumer(CONNECTION_EVICTION_MAX_IDLE_TIME_SETTING, this::setEvictionMaxIdle);
    }

    private IdleConnectionEvictor createConnectionEvictor() {
        return new IdleConnectionEvictor(threadPool, connectionManager, evictorSettings.evictionInterval, evictorSettings.evictionMaxIdle);
    }

    public static List<Setting<?>> getSettings() {
        return List.of(MAX_CONNECTIONS, CONNECTION_EVICTION_THREAD_INTERVAL_SETTING, CONNECTION_EVICTION_MAX_IDLE_TIME_SETTING);
    }

    public void start() {
        httpClient.start();
        connectionEvictor.start();
    }

    public HttpClient getHttpClient() {
        return httpClient;
    }

    @Override
    public void close() throws IOException {
        httpClient.close();
        connectionEvictor.stop();
    }

    private void setMaxConnections(int maxConnections) {
        connectionManager.setMaxTotal(maxConnections);
        connectionManager.setDefaultMaxPerRoute(maxConnections);
    }

    // default for testing
    void setEvictionInterval(TimeValue evictionInterval) {
        evictorSettings = new EvictorSettings(evictionInterval, evictorSettings.evictionMaxIdle);

        connectionEvictor.stop();
        connectionEvictor = createConnectionEvictor();
        connectionEvictor.start();
    }

    void setEvictionMaxIdle(TimeValue evictionMaxIdle) {
        evictorSettings = new EvictorSettings(evictorSettings.evictionInterval, evictionMaxIdle);

        connectionEvictor.stop();
        connectionEvictor = createConnectionEvictor();
        connectionEvictor.start();
    }

    private static class EvictorSettings {
        private final TimeValue evictionInterval;
        private final TimeValue evictionMaxIdle;

        EvictorSettings(Settings settings) {
            this.evictionInterval = CONNECTION_EVICTION_THREAD_INTERVAL_SETTING.get(settings);
            this.evictionMaxIdle = CONNECTION_EVICTION_MAX_IDLE_TIME_SETTING.get(settings);
        }

        EvictorSettings(TimeValue evictionInterval, TimeValue evictionMaxIdle) {
            this.evictionInterval = evictionInterval;
            this.evictionMaxIdle = evictionMaxIdle;
        }
    }
}
