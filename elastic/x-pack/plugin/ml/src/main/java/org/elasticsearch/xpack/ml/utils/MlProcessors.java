/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.utils;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.unit.Processors;
import org.elasticsearch.xpack.ml.MachineLearning;

public final class MlProcessors {

    private MlProcessors() {}

    public static Processors get(DiscoveryNode node) {
        String allocatedProcessorsString = node.getVersion().onOrAfter(Version.V_8_5_0)
            ? node.getAttributes().get(MachineLearning.ALLOCATED_PROCESSORS_NODE_ATTR)
            : node.getAttributes().get(MachineLearning.PRE_V_8_5_ALLOCATED_PROCESSORS_NODE_ATTR);
        if (allocatedProcessorsString == null) {
            return Processors.ZERO;
        }
        try {
            double processorsAsDouble = Double.parseDouble(allocatedProcessorsString);
            return processorsAsDouble > 0 ? Processors.of(processorsAsDouble) : Processors.ZERO;
        } catch (NumberFormatException e) {
            assert e == null
                : MachineLearning.ALLOCATED_PROCESSORS_NODE_ATTR
                    + " should parse because we set it internally: invalid value was ["
                    + allocatedProcessorsString
                    + "]";
            return Processors.ZERO;
        }
    }
}
