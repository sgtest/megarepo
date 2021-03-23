/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.ingest.geoip.stats;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;

import java.util.Set;

public class GeoIpDownloaderStatsActionNodeResponseSerializingTests extends
    AbstractWireSerializingTestCase<GeoIpDownloaderStatsAction.NodeResponse> {

    @Override
    protected Writeable.Reader<GeoIpDownloaderStatsAction.NodeResponse> instanceReader() {
        return GeoIpDownloaderStatsAction.NodeResponse::new;
    }

    @Override
    protected GeoIpDownloaderStatsAction.NodeResponse createTestInstance() {
        return createRandomInstance();
    }

    static GeoIpDownloaderStatsAction.NodeResponse createRandomInstance() {
        DiscoveryNode node = new DiscoveryNode("id", buildNewFakeTransportAddress(), Version.CURRENT);
        Set<String> databases = Set.copyOf(randomList(10, () -> randomAlphaOfLengthBetween(5, 10)));
        Set<String> files = Set.copyOf(randomList(10, () -> randomAlphaOfLengthBetween(5, 10)));
        return new GeoIpDownloaderStatsAction.NodeResponse(node, GeoIpDownloaderStatsSerializingTests.createRandomInstance(), databases,
            files);
    }
}
