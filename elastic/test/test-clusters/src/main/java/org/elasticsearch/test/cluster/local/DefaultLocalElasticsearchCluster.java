/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.test.cluster.local;

import org.elasticsearch.test.cluster.ElasticsearchCluster;
import org.elasticsearch.test.cluster.LogType;
import org.elasticsearch.test.cluster.util.Version;
import org.junit.runner.Description;
import org.junit.runners.model.Statement;

import java.io.InputStream;
import java.util.function.Supplier;

public class DefaultLocalElasticsearchCluster<S extends LocalClusterSpec, H extends LocalClusterHandle> implements ElasticsearchCluster {
    private final Supplier<S> specProvider;
    private final LocalClusterFactory<S, H> clusterFactory;
    private H handle;

    public DefaultLocalElasticsearchCluster(Supplier<S> specProvider, LocalClusterFactory<S, H> clusterFactory) {
        this.specProvider = specProvider;
        this.clusterFactory = clusterFactory;
    }

    @Override
    public Statement apply(Statement base, Description description) {
        return new Statement() {
            @Override
            public void evaluate() throws Throwable {
                try {
                    S spec = specProvider.get();
                    handle = clusterFactory.create(spec);
                    handle.start();
                    base.evaluate();
                } finally {
                    close();
                }
            }
        };
    }

    @Override
    public void start() {
        checkHandle();
        handle.start();
    }

    @Override
    public void stop(boolean forcibly) {
        checkHandle();
        handle.stop(forcibly);
    }

    @Override
    public void stopNode(int index, boolean forcibly) {
        checkHandle();
        handle.stopNode(index, forcibly);
    }

    @Override
    public void restart(boolean forcibly) {
        checkHandle();
        handle.restart(forcibly);
    }

    @Override
    public boolean isStarted() {
        checkHandle();
        return handle.isStarted();
    }

    @Override
    public void close() {
        checkHandle();
        handle.close();
    }

    @Override
    public String getHttpAddresses() {
        checkHandle();
        return handle.getHttpAddresses();
    }

    @Override
    public String getHttpAddress(int index) {
        checkHandle();
        return handle.getHttpAddress(index);
    }

    @Override
    public String getName(int index) {
        checkHandle();
        return handle.getName(index);
    }

    @Override
    public long getPid(int index) {
        checkHandle();
        return handle.getPid(index);
    }

    @Override
    public String getTransportEndpoints() {
        checkHandle();
        return handle.getTransportEndpoints();
    }

    @Override
    public String getTransportEndpoint(int index) {
        checkHandle();
        return handle.getTransportEndpoint(index);
    }

    @Override
    public String getRemoteClusterServerEndpoints() {
        checkHandle();
        return handle.getRemoteClusterServerEndpoints();
    }

    @Override
    public String getRemoteClusterServerEndpoint(int index) {
        checkHandle();
        return handle.getRemoteClusterServerEndpoint(index);
    }

    @Override
    public void upgradeNodeToVersion(int index, Version version) {
        checkHandle();
        handle.upgradeNodeToVersion(index, version);
    }

    @Override
    public void upgradeToVersion(Version version) {
        checkHandle();
        handle.upgradeToVersion(version);
    }

    @Override
    public InputStream getNodeLog(int index, LogType logType) {
        checkHandle();
        return handle.getNodeLog(index, logType);
    }

    protected H getHandle() {
        return handle;
    }

    protected void checkHandle() {
        if (handle == null) {
            throw new IllegalStateException("Cluster handle has not been initialized. Did you forget the @ClassRule annotation?");
        }
    }
}
