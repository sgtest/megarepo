/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ilm;

import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.codec.CodecService;
import org.elasticsearch.index.engine.EngineConfig;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;

import java.io.IOException;
import java.util.List;

import static org.hamcrest.Matchers.equalTo;

public class ForceMergeActionTests extends AbstractActionTestCase<ForceMergeAction> {

    @Override
    protected ForceMergeAction doParseInstance(XContentParser parser) {
        return ForceMergeAction.parse(parser);
    }

    @Override
    protected ForceMergeAction createTestInstance() {
        return randomInstance();
    }

    static ForceMergeAction randomInstance() {
        return new ForceMergeAction(randomIntBetween(1, 100), createRandomCompressionSettings());
    }

    static String createRandomCompressionSettings() {
        if (randomBoolean()) {
            return null;
        }
        return CodecService.BEST_COMPRESSION_CODEC;
    }

    @Override
    protected ForceMergeAction mutateInstance(ForceMergeAction instance) {
        int maxNumSegments = instance.getMaxNumSegments();
        maxNumSegments = maxNumSegments + randomIntBetween(1, 10);
        return new ForceMergeAction(maxNumSegments, createRandomCompressionSettings());
    }

    @Override
    protected Reader<ForceMergeAction> instanceReader() {
        return ForceMergeAction::new;
    }

    private void assertNonBestCompression(ForceMergeAction instance) {
        String phase = randomAlphaOfLength(5);
        StepKey nextStepKey = new StepKey(randomAlphaOfLength(10), randomAlphaOfLength(10), randomAlphaOfLength(10));
        List<Step> steps = instance.toSteps(null, phase, nextStepKey);
        assertNotNull(steps);
        assertEquals(3, steps.size());
        UpdateSettingsStep firstStep = (UpdateSettingsStep) steps.get(0);
        ForceMergeStep secondStep = (ForceMergeStep) steps.get(1);
        SegmentCountStep thirdStep = (SegmentCountStep) steps.get(2);
        assertThat(firstStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, ReadOnlyAction.NAME)));
        assertThat(firstStep.getNextStepKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, ForceMergeStep.NAME)));
        assertTrue(IndexMetaData.INDEX_BLOCKS_WRITE_SETTING.get(firstStep.getSettings()));
        assertThat(secondStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, ForceMergeStep.NAME)));
        assertThat(secondStep.getNextStepKey(), equalTo(thirdStep.getKey()));
        assertThat(thirdStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, SegmentCountStep.NAME)));
        assertThat(thirdStep.getNextStepKey(), equalTo(nextStepKey));
    }

    private void assertBestCompression(ForceMergeAction instance) {
        String phase = randomAlphaOfLength(5);
        StepKey nextStepKey = new StepKey(randomAlphaOfLength(10), randomAlphaOfLength(10), randomAlphaOfLength(10));
        List<Step> steps = instance.toSteps(null, phase, nextStepKey);
        assertNotNull(steps);
        assertEquals(8, steps.size());
        UpdateSettingsStep firstStep = (UpdateSettingsStep) steps.get(0);
        ForceMergeStep secondStep = (ForceMergeStep) steps.get(1);
        CloseIndexStep thirdStep = (CloseIndexStep) steps.get(2);
        UpdateSettingsStep fourthStep = (UpdateSettingsStep) steps.get(3);
        OpenIndexStep fifthStep = (OpenIndexStep) steps.get(4);
        WaitForIndexColorStep sixthStep = (WaitForIndexColorStep) steps.get(5);
        ForceMergeStep seventhStep = (ForceMergeStep) steps.get(6);
        SegmentCountStep eighthStep = (SegmentCountStep) steps.get(7);
        assertThat(firstStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, ReadOnlyAction.NAME)));
        assertThat(firstStep.getNextStepKey(), equalTo(secondStep));
        assertTrue(IndexMetaData.INDEX_BLOCKS_WRITE_SETTING.get(firstStep.getSettings()));
        assertThat(secondStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, ForceMergeAction.NAME)));
        assertThat(secondStep.getNextStepKey(), equalTo(thirdStep));
        assertThat(thirdStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, CloseIndexStep.NAME)));
        assertThat(thirdStep.getNextStepKey(), equalTo(fourthStep.getKey()));
        assertThat(fourthStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, UpdateSettingsStep.NAME)));
        assertThat(fourthStep.getSettings().get(EngineConfig.INDEX_CODEC_SETTING.getKey()), equalTo(CodecService.BEST_COMPRESSION_CODEC));
        assertThat(fourthStep.getNextStepKey(), equalTo(fifthStep.getKey()));
        assertThat(fifthStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, OpenIndexStep.NAME)));
        assertThat(fifthStep.getNextStepKey(), equalTo(sixthStep));
        assertThat(sixthStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, WaitForIndexColorStep.NAME)));
        assertThat(sixthStep.getNextStepKey(), equalTo(seventhStep));
        assertThat(seventhStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, ForceMergeStep.NAME)));
        assertThat(seventhStep.getNextStepKey(), equalTo(nextStepKey));
        assertThat(eighthStep.getKey(), equalTo(new StepKey(phase, ForceMergeAction.NAME, SegmentCountStep.NAME)));
        assertThat(eighthStep.getNextStepKey(), equalTo(nextStepKey));
    }

    public void testMissingMaxNumSegments() throws IOException {
        BytesReference emptyObject = BytesReference.bytes(JsonXContent.contentBuilder().startObject().endObject());
        XContentParser parser = XContentHelper.createParser(null, DeprecationHandler.THROW_UNSUPPORTED_OPERATION,
            emptyObject, XContentType.JSON);
        Exception e = expectThrows(IllegalArgumentException.class, () -> ForceMergeAction.parse(parser));
        assertThat(e.getMessage(), equalTo("Required [max_num_segments]"));
    }

    public void testInvalidNegativeSegmentNumber() {
        Exception r = expectThrows(IllegalArgumentException.class, () -> new
            ForceMergeAction(randomIntBetween(-10, 0), null));
        assertThat(r.getMessage(), equalTo("[max_num_segments] must be a positive integer"));
    }

    public void testInvalidCodec() {
        Exception r = expectThrows(IllegalArgumentException.class, () -> new
            ForceMergeAction(randomIntBetween(1, 10), "DummyCompressingStoredFields"));
        assertThat(r.getMessage(), equalTo("unknown index codec: [DummyCompressingStoredFields]"));
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/51822")
    public void testToSteps() {
        ForceMergeAction instance = createTestInstance();
        if (instance.getCodec() != null && CodecService.BEST_COMPRESSION_CODEC.equals(instance.getCodec())) {
            assertBestCompression(instance);
        }
        else {
            assertNonBestCompression(instance);
        }
    }
}
