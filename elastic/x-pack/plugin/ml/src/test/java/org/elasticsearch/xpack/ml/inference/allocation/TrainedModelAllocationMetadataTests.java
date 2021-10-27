/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.allocation;

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.inference.allocation.TrainedModelAllocation;
import org.elasticsearch.xpack.core.ml.inference.allocation.TrainedModelAllocationTests;

import java.io.IOException;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.function.Function;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.hamcrest.Matchers.is;

public class TrainedModelAllocationMetadataTests extends AbstractSerializingTestCase<TrainedModelAllocationMetadata> {

    public static TrainedModelAllocationMetadata randomInstance() {
        LinkedHashMap<String, TrainedModelAllocation> map = Stream.generate(() -> randomAlphaOfLength(10))
            .limit(randomInt(5))
            .collect(
                Collectors.toMap(Function.identity(), (k) -> TrainedModelAllocationTests.randomInstance(), (k, k1) -> k, LinkedHashMap::new)
            );
        return new TrainedModelAllocationMetadata(map);
    }

    @Override
    protected TrainedModelAllocationMetadata doParseInstance(XContentParser parser) throws IOException {
        return TrainedModelAllocationMetadata.fromXContent(parser);
    }

    @Override
    protected Writeable.Reader<TrainedModelAllocationMetadata> instanceReader() {
        return TrainedModelAllocationMetadata::new;
    }

    @Override
    protected TrainedModelAllocationMetadata createTestInstance() {
        return new TrainedModelAllocationMetadata(new HashMap<>());
    }

    public void testBuilderChanged_WhenAddingRemovingModel() {
        TrainedModelAllocationMetadata original = randomInstance();
        String newModel = "foo_model";

        TrainedModelAllocationMetadata.Builder builder = TrainedModelAllocationMetadata.Builder.fromMetadata(original);
        assertThat(builder.isChanged(), is(false));

        assertUnchanged(builder, b -> b.removeAllocation(newModel));

        builder.addNewAllocation(newModel, TrainedModelAllocation.Builder.empty(randomParams(newModel)));
        assertThat(builder.isChanged(), is(true));
    }

    public void testBuilderChangedWhenAllocationChanged() {
        String allocatedModelId = "test_model_id";
        TrainedModelAllocationMetadata.Builder builder = TrainedModelAllocationMetadata.Builder.fromMetadata(
            TrainedModelAllocationMetadata.Builder.empty()
                .addNewAllocation(allocatedModelId, TrainedModelAllocation.Builder.empty(randomParams(allocatedModelId)))
                .build()
        );
        assertThat(builder.isChanged(), is(false));

        builder.getAllocation(allocatedModelId).addNewRoutingEntry("new-node");
        assertThat(builder.isChanged(), is(true));
    }

    public void testIsAllocated() {
        String allocatedModelId = "test_model_id";
        TrainedModelAllocationMetadata metadata = TrainedModelAllocationMetadata.Builder.empty()
            .addNewAllocation(allocatedModelId, TrainedModelAllocation.Builder.empty(randomParams(allocatedModelId)))
            .build();
        assertThat(metadata.isAllocated(allocatedModelId), is(true));
        assertThat(metadata.isAllocated("unknown_model_id"), is(false));
    }

    private static TrainedModelAllocationMetadata.Builder assertUnchanged(
        TrainedModelAllocationMetadata.Builder builder,
        Function<TrainedModelAllocationMetadata.Builder, TrainedModelAllocationMetadata.Builder> function
    ) {
        function.apply(builder);
        assertThat(builder.isChanged(), is(false));
        return builder;
    }

    private static StartTrainedModelDeploymentAction.TaskParams randomParams(String modelId) {
        return new StartTrainedModelDeploymentAction.TaskParams(
            modelId,
            randomNonNegativeLong(),
            randomIntBetween(1, 8),
            randomIntBetween(1, 8),
            randomIntBetween(1, 10000)
        );
    }

}
