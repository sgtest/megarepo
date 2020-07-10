/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.inference.results;

import org.elasticsearch.common.io.stream.NamedWriteable;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.ingest.IngestDocument;

import java.util.Map;

public interface InferenceResults extends NamedWriteable, ToXContentFragment {

    void writeResult(IngestDocument document, String parentResultField);

    Map<String, Object> asMap();

    Object predictedValue();
}
