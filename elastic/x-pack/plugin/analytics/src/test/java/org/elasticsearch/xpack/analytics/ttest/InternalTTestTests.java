/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.analytics.ttest;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.ParsedAggregation;
import org.elasticsearch.search.aggregations.pipeline.PipelineAggregator;
import org.elasticsearch.test.InternalAggregationTestCase;
import org.elasticsearch.xpack.analytics.AnalyticsPlugin;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.emptyList;

public class InternalTTestTests extends InternalAggregationTestCase<InternalTTest> {

    private TTestType type = randomFrom(TTestType.values());
    private int tails = randomIntBetween(1, 2);

    @Override
    protected InternalTTest createTestInstance(String name, Map<String, Object> metadata) {
        TTestState state = randomState();
        DocValueFormat formatter = randomNumericDocValueFormat();
        return new InternalTTest(name, state, formatter, metadata);
    }

    private TTestState randomState() {
        if (type == TTestType.PAIRED) {
            return new PairedTTestState(randomStats(), tails);
        } else {
            return new UnpairedTTestState(randomStats(), randomStats(), type == TTestType.HOMOSCEDASTIC, tails);
        }
    }

    private TTestStats randomStats() {
        return new TTestStats(randomNonNegativeLong(), randomDouble(), randomDouble());
    }

    @Override
    protected Writeable.Reader<InternalTTest> instanceReader() {
        return InternalTTest::new;
    }

    @Override
    protected void assertReduced(InternalTTest reduced, List<InternalTTest> inputs) {
        TTestState expected = reduced.state.reduce(inputs.stream().map(a -> a.state));
        assertNotNull(expected);
        assertEquals(expected.getValue(), reduced.getValue(), 0.00001);
    }

    @Override
    protected void assertFromXContent(InternalTTest min, ParsedAggregation parsedAggregation) {
        // There is no ParsedTTest yet so we cannot test it here
    }

    @Override
    protected InternalTTest mutateInstance(InternalTTest instance) {
        String name = instance.getName();
        TTestState state;
        try (BytesStreamOutput output = new BytesStreamOutput()) {
            output.writeNamedWriteable(instance.state);
            try (StreamInput in = new NamedWriteableAwareStreamInput(output.bytes().streamInput(), getNamedWriteableRegistry())) {
                state = in.readNamedWriteable(TTestState.class);
            }
        } catch (IOException ex) {
            throw new IllegalStateException(ex);
        }
        DocValueFormat formatter = instance.format();
        List<PipelineAggregator> pipelineAggregators = instance.pipelineAggregators();
        Map<String, Object> metadata = instance.getMetadata();
        switch (between(0, 2)) {
            case 0:
                name += randomAlphaOfLength(5);
                break;
            case 1:
                state = randomState();
                break;
            case 2:
                if (metadata == null) {
                    metadata = new HashMap<>(1);
                } else {
                    metadata = new HashMap<>(instance.getMetadata());
                }
                metadata.put(randomAlphaOfLength(15), randomInt());
                break;
            default:
                throw new AssertionError("Illegal randomisation branch");
        }
        return new InternalTTest(name, state, formatter, metadata);
    }

    @Override
    protected List<NamedXContentRegistry.Entry> getNamedXContents() {
        List<NamedXContentRegistry.Entry> extendedNamedXContents = new ArrayList<>(super.getNamedXContents());
        extendedNamedXContents.add(new NamedXContentRegistry.Entry(Aggregation.class,
            new ParseField(TTestAggregationBuilder.NAME),
            (p, c) -> {
                assumeTrue("There is no ParsedTTest yet", false);
                return null;
            }
        ));
        return extendedNamedXContents;
    }

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        List<NamedWriteableRegistry.Entry> entries = new ArrayList<>();
        entries.addAll(new SearchModule(Settings.EMPTY, emptyList()).getNamedWriteables());
        entries.addAll(new AnalyticsPlugin().getNamedWriteables());
        return new NamedWriteableRegistry(entries);
    }

}
