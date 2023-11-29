/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.inference;

import org.elasticsearch.common.io.stream.NamedWriteable;
import org.elasticsearch.xcontent.ToXContentFragment;

import java.util.List;
import java.util.Map;

public interface InferenceServiceResults extends NamedWriteable, ToXContentFragment {

    /**
     * Transform the result to match the format required for versions prior to
     * {@link org.elasticsearch.TransportVersions#INFERENCE_SERVICE_RESULTS_ADDED}
     */
    List<? extends InferenceResults> transformToLegacyFormat();

    /**
     * Convert the result to a map to aid with test assertions
     */
    Map<String, Object> asMap();
}
