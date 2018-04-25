/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.datafeed.extractor.scroll;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesAction;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesRequest;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.xpack.core.ml.MlClientHelper;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig;
import org.elasticsearch.xpack.core.ml.datafeed.extractor.DataExtractor;
import org.elasticsearch.xpack.ml.datafeed.extractor.DataExtractorFactory;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.utils.MlStrings;

import java.util.Objects;

public class ScrollDataExtractorFactory implements DataExtractorFactory {

    private final Client client;
    private final DatafeedConfig datafeedConfig;
    private final Job job;
    private final ExtractedFields extractedFields;

    private ScrollDataExtractorFactory(Client client, DatafeedConfig datafeedConfig, Job job, ExtractedFields extractedFields) {
        this.client = Objects.requireNonNull(client);
        this.datafeedConfig = Objects.requireNonNull(datafeedConfig);
        this.job = Objects.requireNonNull(job);
        this.extractedFields = Objects.requireNonNull(extractedFields);
    }

    @Override
    public DataExtractor newExtractor(long start, long end) {
        ScrollDataExtractorContext dataExtractorContext = new ScrollDataExtractorContext(
                job.getId(),
                extractedFields,
                datafeedConfig.getIndices(),
                datafeedConfig.getTypes(),
                datafeedConfig.getQuery(),
                datafeedConfig.getScriptFields(),
                datafeedConfig.getScrollSize(),
                start,
                end,
                datafeedConfig.getHeaders());
        return new ScrollDataExtractor(client, dataExtractorContext);
    }

    public static void create(Client client, DatafeedConfig datafeed, Job job, ActionListener<DataExtractorFactory> listener) {

        // Step 2. Contruct the factory and notify listener
        ActionListener<FieldCapabilitiesResponse> fieldCapabilitiesHandler = ActionListener.wrap(
                fieldCapabilitiesResponse -> {
                    ExtractedFields extractedFields = ExtractedFields.build(job, datafeed, fieldCapabilitiesResponse);
                    listener.onResponse(new ScrollDataExtractorFactory(client, datafeed, job, extractedFields));
                }, e -> {
                    if (e instanceof IndexNotFoundException) {
                        listener.onFailure(new ResourceNotFoundException("datafeed [" + datafeed.getId()
                                + "] cannot retrieve data because index " + ((IndexNotFoundException) e).getIndex() + " does not exist"));
                    } else {
                        listener.onFailure(e);
                    }
                }
        );

        // Step 1. Get field capabilities necessary to build the information of how to extract fields
        FieldCapabilitiesRequest fieldCapabilitiesRequest = new FieldCapabilitiesRequest();
        fieldCapabilitiesRequest.indices(datafeed.getIndices().toArray(new String[datafeed.getIndices().size()]));
        // We need capabilities for all fields matching the requested fields' parents so that we can work around
        // multi-fields that are not in source.
        String[] requestFields = job.allInputFields().stream().map(f -> MlStrings.getParentField(f) + "*")
                .toArray(size -> new String[size]);
        fieldCapabilitiesRequest.fields(requestFields);
        MlClientHelper.<FieldCapabilitiesResponse>execute(datafeed, client, () -> {
            client.execute(FieldCapabilitiesAction.INSTANCE, fieldCapabilitiesRequest, fieldCapabilitiesHandler);
            // This response gets discarded - the listener handles the real response
            return null;
        });
    }
}
