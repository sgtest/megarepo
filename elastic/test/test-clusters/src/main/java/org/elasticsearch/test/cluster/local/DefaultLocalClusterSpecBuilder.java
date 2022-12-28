/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.test.cluster.local;

import org.elasticsearch.test.cluster.ElasticsearchCluster;
import org.elasticsearch.test.cluster.local.LocalClusterSpec.LocalNodeSpec;
import org.elasticsearch.test.cluster.local.distribution.DistributionType;
import org.elasticsearch.test.cluster.local.model.User;
import org.elasticsearch.test.cluster.util.Version;
import org.elasticsearch.test.cluster.util.resource.Resource;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.function.Consumer;

public class DefaultLocalClusterSpecBuilder extends AbstractLocalSpecBuilder<LocalClusterSpecBuilder> implements LocalClusterSpecBuilder {
    private String name = "test-cluster";
    private final List<DefaultLocalNodeSpecBuilder> nodeBuilders = new ArrayList<>();
    private final List<User> users = new ArrayList<>();
    private final List<Resource> roleFiles = new ArrayList<>();

    public DefaultLocalClusterSpecBuilder() {
        super(null);
        this.settings(new DefaultSettingsProvider());
        this.environment(new DefaultEnvironmentProvider());
        this.rolesFile(Resource.fromClasspath("default_test_roles.yml"));
    }

    @Override
    public DefaultLocalClusterSpecBuilder name(String name) {
        this.name = name;
        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder apply(LocalClusterConfigProvider configProvider) {
        configProvider.apply(this);
        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder nodes(int nodes) {
        if (nodes < nodeBuilders.size()) {
            throw new IllegalArgumentException(
                "Cannot shrink cluster to " + nodes + ". " + nodeBuilders.size() + " nodes already configured"
            );
        }

        int newNodes = nodes - nodeBuilders.size();
        for (int i = 0; i < newNodes; i++) {
            nodeBuilders.add(new DefaultLocalNodeSpecBuilder(this));
        }

        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder withNode(Consumer<? super LocalNodeSpecBuilder> config) {
        DefaultLocalNodeSpecBuilder builder = new DefaultLocalNodeSpecBuilder(this);
        config.accept(builder);
        nodeBuilders.add(builder);
        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder node(int index, Consumer<? super LocalNodeSpecBuilder> config) {
        try {
            DefaultLocalNodeSpecBuilder builder = nodeBuilders.get(index);
            config.accept(builder);
        } catch (IndexOutOfBoundsException e) {
            throw new IllegalArgumentException(
                "No node at index + " + index + " exists. Only " + nodeBuilders.size() + " nodes have been configured"
            );
        }
        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder user(String username, String password) {
        this.users.add(new User(username, password));
        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder user(String username, String password, String role) {
        this.users.add(new User(username, password, role));
        return this;
    }

    @Override
    public DefaultLocalClusterSpecBuilder rolesFile(Resource rolesFile) {
        this.roleFiles.add(rolesFile);
        return this;
    }

    @Override
    public ElasticsearchCluster build() {
        List<User> clusterUsers = users.isEmpty() ? List.of(User.DEFAULT_USER) : users;
        LocalClusterSpec clusterSpec = new LocalClusterSpec(name, clusterUsers, roleFiles);
        List<LocalNodeSpec> nodeSpecs;

        if (nodeBuilders.isEmpty()) {
            // No node-specific configuration so assume a single-node cluster
            nodeSpecs = List.of(new DefaultLocalNodeSpecBuilder(this).build(clusterSpec));
        } else {
            nodeSpecs = nodeBuilders.stream().map(node -> node.build(clusterSpec)).toList();
        }

        clusterSpec.setNodes(nodeSpecs);
        clusterSpec.validate();

        return new LocalElasticsearchCluster(clusterSpec);
    }

    public static class DefaultLocalNodeSpecBuilder extends AbstractLocalSpecBuilder<LocalNodeSpecBuilder> implements LocalNodeSpecBuilder {
        private String name;

        protected DefaultLocalNodeSpecBuilder(AbstractLocalSpecBuilder<?> parent) {
            super(parent);
        }

        @Override
        public DefaultLocalNodeSpecBuilder name(String name) {
            this.name = name;
            return this;
        }

        private LocalNodeSpec build(LocalClusterSpec cluster) {

            return new LocalNodeSpec(
                cluster,
                name,
                Version.CURRENT,
                getSettingsProviders(),
                getSettings(),
                getEnvironmentProviders(),
                getEnvironment(),
                getModules(),
                getPlugins(),
                Optional.ofNullable(getDistributionType()).orElse(DistributionType.INTEG_TEST),
                getFeatures(),
                getKeystoreSettings(),
                getExtraConfigFiles()
            );
        }
    }
}
