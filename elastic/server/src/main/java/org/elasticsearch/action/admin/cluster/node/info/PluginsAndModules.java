/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.node.info;

import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.node.ReportingService;
import org.elasticsearch.plugins.PluginInfo;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;

/**
 * Information about plugins and modules
 */
public class PluginsAndModules implements ReportingService.Info {
    private final List<PluginInfo> plugins;
    private final List<PluginInfo> modules;

    public PluginsAndModules(List<PluginInfo> plugins, List<PluginInfo> modules) {
        this.plugins = Collections.unmodifiableList(plugins);
        this.modules = Collections.unmodifiableList(modules);
    }

    public PluginsAndModules(StreamInput in) throws IOException {
        this.plugins = Collections.unmodifiableList(in.readList(PluginInfo::new));
        this.modules = Collections.unmodifiableList(in.readList(PluginInfo::new));
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeList(plugins);
        out.writeList(modules);
    }

    /**
     * Returns an ordered list based on plugins name
     */
    public List<PluginInfo> getPluginInfos() {
        List<PluginInfo> plugins = new ArrayList<>(this.plugins);
        Collections.sort(plugins, Comparator.comparing(PluginInfo::getName));
        return plugins;
    }

    /**
     * Returns an ordered list based on modules name
     */
    public List<PluginInfo> getModuleInfos() {
        List<PluginInfo> modules = new ArrayList<>(this.modules);
        Collections.sort(modules, Comparator.comparing(PluginInfo::getName));
        return modules;
    }

    public void addPlugin(PluginInfo info) {
        plugins.add(info);
    }

    public void addModule(PluginInfo info) {
        modules.add(info);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startArray("plugins");
        for (PluginInfo pluginInfo : getPluginInfos()) {
            pluginInfo.toXContent(builder, params);
        }
        builder.endArray();
        // TODO: not ideal, make a better api for this (e.g. with jar metadata, and so on)
        builder.startArray("modules");
        for (PluginInfo moduleInfo : getModuleInfos()) {
            moduleInfo.toXContent(builder, params);
        }
        builder.endArray();

        return builder;
    }
}
