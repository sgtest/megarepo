/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.monitoring.collector.ccr;

import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.xpack.core.XPackClient;
import org.elasticsearch.xpack.core.ccr.action.FollowStatsAction;
import org.elasticsearch.xpack.core.ccr.client.CcrClient;
import org.elasticsearch.xpack.core.monitoring.exporter.MonitoringDoc;

import java.util.Collection;
import java.util.stream.Collectors;

public final class FollowStatsCollector extends AbstractCcrCollector {

    public static final Setting<TimeValue> CCR_STATS_TIMEOUT = collectionTimeoutSetting("ccr.stats.timeout");

    public FollowStatsCollector(
            final Settings settings,
            final ClusterService clusterService,
            final XPackLicenseState licenseState,
            final Client client) {
        super(settings, clusterService, CCR_STATS_TIMEOUT, licenseState, new XPackClient(client).ccr(),
            client.threadPool().getThreadContext());
    }

    FollowStatsCollector(
            final Settings settings,
            final ClusterService clusterService,
            final XPackLicenseState licenseState,
            final CcrClient ccrClient,
            final ThreadContext threadContext) {
        super(settings, clusterService, CCR_STATS_TIMEOUT, licenseState, ccrClient, threadContext);
    }

    @Override
    Collection<MonitoringDoc> innerDoCollect(
        long timestamp,
        String clusterUuid,
        long interval,
        MonitoringDoc.Node node) throws Exception {


            final FollowStatsAction.StatsRequest request = new FollowStatsAction.StatsRequest();
            request.setIndices(getCollectionIndices());
            final FollowStatsAction.StatsResponses responses = ccrClient.stats(request).actionGet(getCollectionTimeout());

        return responses
            .getStatsResponses()
            .stream()
            .map(stats -> new FollowStatsMonitoringDoc(clusterUuid, timestamp, interval, node, stats.status()))
            .collect(Collectors.toList());
    }

}
