/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.persistence;

import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.xpack.core.action.util.PageParams;
import org.elasticsearch.xpack.core.dataframe.DataFrameMessages;
import org.elasticsearch.xpack.core.dataframe.transforms.DataFrameTransformCheckpoint;
import org.elasticsearch.xpack.core.dataframe.transforms.DataFrameTransformCheckpointTests;
import org.elasticsearch.xpack.core.dataframe.transforms.DataFrameTransformConfig;
import org.elasticsearch.xpack.core.dataframe.transforms.DataFrameTransformConfigTests;
import org.elasticsearch.xpack.dataframe.DataFrameSingleNodeTestCase;
import org.junit.Before;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;

public class DataFrameTransformsConfigManagerTests extends DataFrameSingleNodeTestCase {

    private DataFrameTransformsConfigManager transformsConfigManager;

    @Before
    public void createComponents() {
        transformsConfigManager = new DataFrameTransformsConfigManager(client(), xContentRegistry());
    }

    public void testGetMissingTransform() throws InterruptedException {
        // the index does not exist yet
        assertAsync(listener -> transformsConfigManager.getTransformConfiguration("not_there", listener), (DataFrameTransformConfig) null,
                null, e -> {
                    assertEquals(ResourceNotFoundException.class, e.getClass());
                    assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, "not_there"),
                            e.getMessage());
                });

        // create one transform and test with an existing index
        assertAsync(
                listener -> transformsConfigManager
                        .putTransformConfiguration(DataFrameTransformConfigTests.randomDataFrameTransformConfig(), listener),
                true, null, null);

        // same test, but different code path
        assertAsync(listener -> transformsConfigManager.getTransformConfiguration("not_there", listener), (DataFrameTransformConfig) null,
                null, e -> {
                    assertEquals(ResourceNotFoundException.class, e.getClass());
                    assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, "not_there"),
                            e.getMessage());
                });
    }

    public void testDeleteMissingTransform() throws InterruptedException {
        // the index does not exist yet
        assertAsync(listener -> transformsConfigManager.deleteTransform("not_there", listener), (Boolean) null, null, e -> {
            assertEquals(ResourceNotFoundException.class, e.getClass());
            assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, "not_there"), e.getMessage());
        });

        // create one transform and test with an existing index
        assertAsync(
                listener -> transformsConfigManager
                        .putTransformConfiguration(DataFrameTransformConfigTests.randomDataFrameTransformConfig(), listener),
                true, null, null);

        // same test, but different code path
        assertAsync(listener -> transformsConfigManager.deleteTransform("not_there", listener), (Boolean) null, null, e -> {
            assertEquals(ResourceNotFoundException.class, e.getClass());
            assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, "not_there"), e.getMessage());
        });
    }

    public void testCreateReadDeleteTransform() throws InterruptedException {
        DataFrameTransformConfig transformConfig = DataFrameTransformConfigTests.randomDataFrameTransformConfig();

        // create transform
        assertAsync(listener -> transformsConfigManager.putTransformConfiguration(transformConfig, listener), true, null, null);

        // read transform
        assertAsync(listener -> transformsConfigManager.getTransformConfiguration(transformConfig.getId(), listener), transformConfig, null,
                null);

        // try to create again
        assertAsync(listener -> transformsConfigManager.putTransformConfiguration(transformConfig, listener), (Boolean) null, null, e -> {
            assertEquals(ResourceAlreadyExistsException.class, e.getClass());
            assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_PUT_DATA_FRAME_TRANSFORM_EXISTS, transformConfig.getId()),
                    e.getMessage());
        });

        // delete transform
        assertAsync(listener -> transformsConfigManager.deleteTransform(transformConfig.getId(), listener), true, null, null);

        // delete again
        assertAsync(listener -> transformsConfigManager.deleteTransform(transformConfig.getId(), listener), (Boolean) null, null, e -> {
            assertEquals(ResourceNotFoundException.class, e.getClass());
            assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, transformConfig.getId()),
                    e.getMessage());
        });

        // try to get deleted transform
        assertAsync(listener -> transformsConfigManager.getTransformConfiguration(transformConfig.getId(), listener),
                (DataFrameTransformConfig) null, null, e -> {
                    assertEquals(ResourceNotFoundException.class, e.getClass());
                    assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, transformConfig.getId()),
                            e.getMessage());
                });
    }

    public void testCreateReadDeleteCheckPoint() throws InterruptedException {
        DataFrameTransformCheckpoint checkpoint = DataFrameTransformCheckpointTests.randomDataFrameTransformCheckpoints();

        // create
        assertAsync(listener -> transformsConfigManager.putTransformCheckpoint(checkpoint, listener), true, null, null);

        // read
        assertAsync(listener -> transformsConfigManager.getTransformCheckpoint(checkpoint.getTransformId(), checkpoint.getCheckpoint(),
                listener), checkpoint, null, null);

        // delete
        assertAsync(listener -> transformsConfigManager.deleteTransform(checkpoint.getTransformId(), listener), true, null, null);

        // delete again
        assertAsync(listener -> transformsConfigManager.deleteTransform(checkpoint.getTransformId(), listener), (Boolean) null, null, e -> {
            assertEquals(ResourceNotFoundException.class, e.getClass());
            assertEquals(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, checkpoint.getTransformId()),
                    e.getMessage());
        });

        // getting a non-existing checkpoint returns null
        assertAsync(listener -> transformsConfigManager.getTransformCheckpoint(checkpoint.getTransformId(), checkpoint.getCheckpoint(),
                listener), DataFrameTransformCheckpoint.EMPTY, null, null);
    }

    public void testExpandIds() throws Exception {
        DataFrameTransformConfig transformConfig1 = DataFrameTransformConfigTests.randomDataFrameTransformConfig("transform1_expand");
        DataFrameTransformConfig transformConfig2 = DataFrameTransformConfigTests.randomDataFrameTransformConfig("transform2_expand");
        DataFrameTransformConfig transformConfig3 = DataFrameTransformConfigTests.randomDataFrameTransformConfig("transform3_expand");

        // create transform
        assertAsync(listener -> transformsConfigManager.putTransformConfiguration(transformConfig1, listener), true, null, null);
        assertAsync(listener -> transformsConfigManager.putTransformConfiguration(transformConfig2, listener), true, null, null);
        assertAsync(listener -> transformsConfigManager.putTransformConfiguration(transformConfig3, listener), true, null, null);


        // expand 1 id
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds(transformConfig1.getId(),
                    PageParams.defaultParams(),
                    listener),
            Collections.singletonList("transform1_expand"),
            null,
            null);

        // expand 2 ids explicitly
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds("transform1_expand,transform2_expand",
                    PageParams.defaultParams(),
                    listener),
            Arrays.asList("transform1_expand", "transform2_expand"),
            null,
            null);

        // expand 3 ids wildcard and explicit
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds("transform1*,transform2_expand,transform3_expand",
                    PageParams.defaultParams(),
                    listener),
            Arrays.asList("transform1_expand", "transform2_expand", "transform3_expand"),
            null,
            null);

        // expand 3 ids _all
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds("_all",
                    PageParams.defaultParams(),
                    listener),
            Arrays.asList("transform1_expand", "transform2_expand", "transform3_expand"),
            null,
            null);

        // expand 1 id _all with pagination
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds("_all",
                    new PageParams(0, 1),
                    listener),
            Collections.singletonList("transform1_expand"),
            null,
            null);

        // expand 2 later ids _all with pagination
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds("_all",
                    new PageParams(1, 2),
                    listener),
            Arrays.asList("transform2_expand", "transform3_expand"),
            null,
            null);

        // expand 1 id explicitly that does not exist
        assertAsync(listener ->
                transformsConfigManager.expandTransformIds("unknown,unknown2",
                    new PageParams(1, 2),
                    listener),
            (List<String>)null,
            null,
            e -> {
                assertThat(e, instanceOf(ResourceNotFoundException.class));
                assertThat(e.getMessage(),
                    equalTo(DataFrameMessages.getMessage(DataFrameMessages.REST_DATA_FRAME_UNKNOWN_TRANSFORM, "unknown,unknown2")));
            });

    }
}
