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
package org.elasticsearch.gradle.testclusters;

import org.elasticsearch.GradleServicesAdapter;
import org.elasticsearch.gradle.Distribution;
import org.elasticsearch.gradle.FileSupplier;
import org.elasticsearch.gradle.Version;
import org.elasticsearch.gradle.http.WaitForHttpResource;
import org.gradle.api.NamedDomainObjectContainer;
import org.gradle.api.Project;
import org.gradle.api.logging.Logger;
import org.gradle.api.logging.Logging;

import java.io.File;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.GeneralSecurityException;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.stream.Collectors;

public class ElasticsearchCluster implements TestClusterConfiguration {

    private static final Logger LOGGER = Logging.getLogger(ElasticsearchNode.class);
    private static final int CLUSTER_UP_TIMEOUT = 40;
    private static final TimeUnit CLUSTER_UP_TIMEOUT_UNIT = TimeUnit.SECONDS;

    private final AtomicBoolean configurationFrozen = new AtomicBoolean(false);
    private final String path;
    private final String clusterName;
    private final NamedDomainObjectContainer<ElasticsearchNode> nodes;
    private final File workingDirBase;
    private final File artifactsExtractDir;
    private final LinkedHashMap<String, Predicate<TestClusterConfiguration>> waitConditions = new LinkedHashMap<>();
    private final GradleServicesAdapter services;

    public ElasticsearchCluster(String path, String clusterName, Project project, File artifactsExtractDir, File workingDirBase) {
        this.path = path;
        this.clusterName = clusterName;
        this.workingDirBase = workingDirBase;
        this.artifactsExtractDir = artifactsExtractDir;
        this.services = GradleServicesAdapter.getInstance(project);
        this.nodes = project.container(ElasticsearchNode.class);
        this.nodes.add(
            new ElasticsearchNode(
                path, clusterName + "-0",
                services, artifactsExtractDir, workingDirBase
            )
        );
        // configure the cluster name eagerly so nodes know about it
        this.nodes.all((node) -> node.defaultConfig.put("cluster.name", safeName(clusterName)));

        addWaitForClusterHealth();
    }

    public void setNumberOfNodes(int numberOfNodes) {
        checkFrozen();

        if (numberOfNodes < 1) {
            throw new IllegalArgumentException("Number of nodes should be >= 1 but was " + numberOfNodes + " for " + this);
        }

        if (numberOfNodes <= nodes.size()) {
            throw new IllegalArgumentException(
                "Cannot shrink " + this + " to have " + numberOfNodes + " nodes as it already has " + getNumberOfNodes()
            );
        }

        for (int i = nodes.size() ; i < numberOfNodes; i++) {
            this.nodes.add(new ElasticsearchNode(
                path, clusterName + "-" + i, services, artifactsExtractDir, workingDirBase
            ));
        }
    }

    private ElasticsearchNode getFirstNode() {
        return nodes.getAt(clusterName + "-0");
    }

    public int getNumberOfNodes() {
        return nodes.size();
    }

    public String getName() {
        return clusterName;
    }

    @Override
    public void setVersion(String version) {
        nodes.all(each -> each.setVersion(version));
    }

    @Override
    public void setDistribution(Distribution distribution) {
        nodes.all(each -> each.setDistribution(distribution));
    }

    @Override
    public void plugin(URI plugin) {
        nodes.all(each -> each.plugin(plugin));
    }

    @Override
    public void plugin(File plugin) {
        nodes.all(each -> each.plugin(plugin));
    }

    @Override
    public void module(File module) {
        nodes.all(each -> each.module(module));
    }

    @Override
    public void keystore(String key, String value) {
        nodes.all(each -> each.keystore(key, value));
    }

    @Override
    public void keystore(String key, Supplier<CharSequence> valueSupplier) {
        nodes.all(each -> each.keystore(key, valueSupplier));
    }

    @Override
    public void keystore(String key, File value) {
        nodes.all(each -> each.keystore(key, value));
    }

    @Override
    public void keystore(String key, FileSupplier valueSupplier) {
        nodes.all(each -> each.keystore(key, valueSupplier));
    }

    @Override
    public void setting(String key, String value) {
        nodes.all(each -> each.setting(key, value));
    }

    @Override
    public void setting(String key, Supplier<CharSequence> valueSupplier) {
        nodes.all(each -> each.setting(key, valueSupplier));
    }

    @Override
    public void systemProperty(String key, String value) {
        nodes.all(each -> each.systemProperty(key, value));
    }

    @Override
    public void systemProperty(String key, Supplier<CharSequence> valueSupplier) {
        nodes.all(each -> each.systemProperty(key, valueSupplier));
    }

    @Override
    public void environment(String key, String value) {
        nodes.all(each -> each.environment(key, value));
    }

    @Override
    public void environment(String key, Supplier<CharSequence> valueSupplier) {
        nodes.all(each -> each.environment(key, valueSupplier));
    }

    @Override
    public void jvmArgs(String... values) {
        nodes.all(each -> each.jvmArgs(values));
    }

    @Override
    public void jvmArgs(Supplier<String[]> valueSupplier) {
        nodes.all(each -> each.jvmArgs(valueSupplier));
    }

    @Override
    public void freeze() {
        nodes.forEach(ElasticsearchNode::freeze);
        configurationFrozen.set(true);
    }

    private void checkFrozen() {
        if (configurationFrozen.get()) {
            throw new IllegalStateException("Configuration for " + this + " can not be altered, already locked");
        }
    }

    @Override
    public void setJavaHome(File javaHome) {
        nodes.all(each -> each.setJavaHome(javaHome));
    }

    @Override
    public void start() {
        final String nodeNames;
        if (nodes.stream().map(ElasticsearchNode::getName).anyMatch( name -> name == null)) {
            nodeNames = null;
        } else {
            nodeNames = nodes.stream().map(ElasticsearchNode::getName).collect(Collectors.joining(","));
        };
        for (ElasticsearchNode node : nodes) {
            if (nodeNames != null) {
                // Can only configure master nodes if we have node names defined
                if (Version.fromString(node.getVersion()).getMajor() >= 7) {
                    node.defaultConfig.put("cluster.initial_master_nodes", "[" + nodeNames + "]");
                    node.defaultConfig.put("discovery.seed_providers", "file");
                    node.defaultConfig.put("discovery.seed_hosts", "[]");
                }
            }
            node.start();
        }
    }

    @Override
    public void restart() {
        nodes.forEach(ElasticsearchNode::restart);
    }

    @Override
    public void extraConfigFile(String destination, File from) {
        nodes.all(node -> node.extraConfigFile(destination, from));
    }

    @Override
    public void user(Map<String, String> userSpec) {
        nodes.all(node -> node.user(userSpec));
    }

    private void writeUnicastHostsFiles() {
        String unicastUris = nodes.stream().flatMap(node -> node.getAllTransportPortURI().stream()).collect(Collectors.joining("\n"));
        nodes.forEach(node -> {
            try {
                Files.write(node.getConfigDir().resolve("unicast_hosts.txt"), unicastUris.getBytes(StandardCharsets.UTF_8));
            } catch (IOException e) {
                throw new UncheckedIOException("Failed to write unicast_hosts for " + this, e);
            }
        });
    }

    @Override
    public String getHttpSocketURI() {
        waitForAllConditions();
        return getFirstNode().getHttpSocketURI();
    }

    @Override
    public String getTransportPortURI() {
        waitForAllConditions();
        return getFirstNode().getTransportPortURI();
    }

    @Override
    public List<String> getAllHttpSocketURI() {
        waitForAllConditions();
        return nodes.stream().flatMap(each -> each.getAllHttpSocketURI().stream()).collect(Collectors.toList());
    }

    @Override
    public List<String> getAllTransportPortURI() {
        waitForAllConditions();
        return nodes.stream().flatMap(each -> each.getAllTransportPortURI().stream()).collect(Collectors.toList());
    }

    public void waitForAllConditions() {
        LOGGER.info("Waiting for nodes");
        nodes.forEach(ElasticsearchNode::waitForAllConditions);

        writeUnicastHostsFiles();

        LOGGER.info("Starting to wait for cluster to form");
        waitForConditions(waitConditions, System.currentTimeMillis(), CLUSTER_UP_TIMEOUT, CLUSTER_UP_TIMEOUT_UNIT, this);
    }

    @Override
    public void stop(boolean tailLogs) {
        nodes.forEach(each -> each.stop(tailLogs));
    }

    @Override
    public void setNameCustomization(Function<String, String> nameCustomization) {
        nodes.all(each -> each.setNameCustomization(nameCustomization));
    }

    @Override
    public boolean isProcessAlive() {
        return nodes.stream().noneMatch(node -> node.isProcessAlive() == false);
    }

    void eachVersionedDistribution(BiConsumer<String, Distribution> consumer) {
        nodes.forEach(each -> {
            consumer.accept(each.getVersion(), each.getDistribution());
        });
    }

    public ElasticsearchNode singleNode() {
        if (nodes.size() != 1) {
            throw new IllegalStateException(
                "Can't treat " + this + " as single node as it has " + nodes.size() + " nodes"
            );
        }
        return getFirstNode();
    }

    private void addWaitForClusterHealth() {
        waitConditions.put("cluster health yellow", (node) -> {
            try {
                boolean httpSslEnabled = getFirstNode().isHttpSslEnabled();
                WaitForHttpResource wait = new WaitForHttpResource(
                    httpSslEnabled ? "https" : "http",
                    getFirstNode().getHttpSocketURI(),
                    nodes.size()
                );
                if (httpSslEnabled) {

                    getFirstNode().configureHttpWait(wait);
                }
                List<Map<String, String>> credentials = getFirstNode().getCredentials();
                if (getFirstNode().getCredentials().isEmpty() == false) {
                    wait.setUsername(credentials.get(0).get("useradd"));
                    wait.setPassword(credentials.get(0).get("-p"));
                }
                return wait.wait(500);
            } catch (IOException e) {
                throw new UncheckedIOException("IO error while waiting cluster", e);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new TestClustersException("Interrupted while waiting for " + this, e);
            } catch (GeneralSecurityException e) {
                throw new RuntimeException("security exception", e);
            }
        });
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        ElasticsearchCluster that = (ElasticsearchCluster) o;
        return Objects.equals(clusterName, that.clusterName) &&
            Objects.equals(path, that.path);
    }

    @Override
    public int hashCode() {
        return Objects.hash(clusterName, path);
    }

    @Override
    public String toString() {
        return "cluster{" + path + ":" + clusterName + "}";
    }
}
