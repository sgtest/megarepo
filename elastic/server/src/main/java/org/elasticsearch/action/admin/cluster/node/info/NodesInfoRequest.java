/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.node.info;

import org.elasticsearch.action.support.nodes.BaseNodesRequest;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;

import java.io.IOException;
import java.util.Arrays;
import java.util.Set;
import java.util.SortedSet;
import java.util.TreeSet;
import java.util.stream.Collectors;

/**
 * A request to get node (cluster) level information.
 */
public class NodesInfoRequest extends BaseNodesRequest<NodesInfoRequest> {

    private Set<String> requestedMetrics = Metric.allMetrics();

    /**
     * Create a new NodeInfoRequest from a {@link StreamInput} object.
     *
     * @param in A stream input object.
     * @throws IOException if the stream cannot be deserialized.
     */
    public NodesInfoRequest(StreamInput in) throws IOException {
        super(in);
        requestedMetrics.clear();
        requestedMetrics.addAll(Arrays.asList(in.readStringArray()));
    }

    /**
     * Get information from nodes based on the nodes ids specified. If none are passed, information
     * for all nodes will be returned.
     */
    public NodesInfoRequest(String... nodesIds) {
        super(nodesIds);
        all();
    }

    /**
     * Clears all info flags.
     */
    public NodesInfoRequest clear() {
        requestedMetrics.clear();
        return this;
    }

    /**
     * Sets to return all the data.
     */
    public NodesInfoRequest all() {
        requestedMetrics.addAll(Metric.allMetrics());
        return this;
    }

    /**
     * Get the names of requested metrics
     */
    public Set<String> requestedMetrics() {
        return Set.copyOf(requestedMetrics);
    }

    /**
     * Add metric
     */
    public NodesInfoRequest addMetric(String metric) {
        if (Metric.allMetrics().contains(metric) == false) {
            throw new IllegalStateException("Used an illegal metric: " + metric);
        }
        requestedMetrics.add(metric);
        return this;
    }

    /**
     * Add multiple metrics
     */
    public NodesInfoRequest addMetrics(String... metrics) {
        SortedSet<String> metricsSet = new TreeSet<>(Set.of(metrics));
        if (Metric.allMetrics().containsAll(metricsSet) == false) {
            metricsSet.removeAll(Metric.allMetrics());
            String plural = metricsSet.size() == 1 ? "" : "s";
            throw new IllegalStateException("Used illegal metric" + plural + ": " + metricsSet);
        }
        requestedMetrics.addAll(metricsSet);
        return this;
    }

    /**
     * Remove metric
     */
    public NodesInfoRequest removeMetric(String metric) {
        if (Metric.allMetrics().contains(metric) == false) {
            throw new IllegalStateException("Used an illegal metric: " + metric);
        }
        requestedMetrics.remove(metric);
        return this;
    }

    /**
     * Helper method for adding and removing metrics. Used when deserializing
     * a NodesInfoRequest from an ordered list of booleans.
     *
     * @param addMetric Whether or not to include a metric.
     * @param metricName Name of the metric to include or remove.
     */
    private void optionallyAddMetric(boolean addMetric, String metricName) {
        if (addMetric) {
            requestedMetrics.add(metricName);
        }
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeStringArray(requestedMetrics.toArray(String[]::new));
    }

    /**
     * An enumeration of the "core" sections of metrics that may be requested
     * from the nodes information endpoint. Eventually this list list will be
     * pluggable.
     */
    public enum Metric {
        SETTINGS("settings"),
        OS("os"),
        PROCESS("process"),
        JVM("jvm"),
        THREAD_POOL("thread_pool"),
        TRANSPORT("transport"),
        HTTP("http"),
        PLUGINS("plugins"),
        INGEST("ingest"),
        AGGREGATIONS("aggregations"),
        INDICES("indices");

        private String metricName;

        Metric(String name) {
            this.metricName = name;
        }

        public String metricName() {
            return this.metricName;
        }

        boolean containedIn(Set<String> metricNames) {
            return metricNames.contains(this.metricName());
        }

        public static Set<String> allMetrics() {
            return Arrays.stream(values()).map(Metric::metricName).collect(Collectors.toSet());
        }
    }
}
