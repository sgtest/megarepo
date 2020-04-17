/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.analytics.boxplot;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.plugins.SearchPlugin;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.ParsedAggregation;
import org.elasticsearch.search.aggregations.metrics.TDigestState;
import org.elasticsearch.test.InternalAggregationTestCase;
import org.elasticsearch.xpack.analytics.AnalyticsPlugin;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class InternalBoxplotTests extends InternalAggregationTestCase<InternalBoxplot> {

    @Override
    protected SearchPlugin registerPlugin() {
        return new AnalyticsPlugin();
    }

    @Override
    protected InternalBoxplot createTestInstance(String name, Map<String, Object> metadata) {
        int numValues = frequently() ? randomInt(100) : 0;
        TDigestState state = new TDigestState(100);
        for (int i = 0; i < numValues; ++i) {
            state.add(randomDouble());
        }
        DocValueFormat formatter = randomNumericDocValueFormat();

        return new InternalBoxplot(name, state, formatter, metadata);
    }

    @Override
    protected void assertReduced(InternalBoxplot reduced, List<InternalBoxplot> inputs) {
        TDigestState expected = new TDigestState(reduced.state().compression());
        for (InternalBoxplot input : inputs) {
            expected.add(input.state());
        }
        assertNotNull(expected);
        assertEquals(expected.getMax(), reduced.getMax(), 0);
        assertEquals(expected.getMin(), reduced.getMin(), 0);
    }

    @Override
    protected void assertFromXContent(InternalBoxplot min, ParsedAggregation parsedAggregation) {
        // There is no ParsedBoxplot yet so we cannot test it here
    }

    @Override
    protected InternalBoxplot mutateInstance(InternalBoxplot instance) {
        String name = instance.getName();
        TDigestState state;
        try (BytesStreamOutput output = new BytesStreamOutput()) {
            TDigestState.write(instance.state(), output);
            try (StreamInput in = new NamedWriteableAwareStreamInput(output.bytes().streamInput(), getNamedWriteableRegistry())) {
                state = TDigestState.read(in);
            }
        } catch (IOException ex) {
            throw new IllegalStateException(ex);
        }
        DocValueFormat formatter = instance.format();
        Map<String, Object> metadata = instance.getMetadata();
        switch (between(0, 2)) {
            case 0:
                name += randomAlphaOfLength(5);
                break;
            case 1:
                state.add(randomDouble());
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
        return new InternalBoxplot(name, state, formatter, metadata);
    }

    @Override
    protected List<NamedXContentRegistry.Entry> getNamedXContents() {
        List<NamedXContentRegistry.Entry> extendedNamedXContents = new ArrayList<>(super.getNamedXContents());
        extendedNamedXContents.add(new NamedXContentRegistry.Entry(Aggregation.class,
            new ParseField(BoxplotAggregationBuilder.NAME),
            (p, c) -> {
                assumeTrue("There is no ParsedBoxlot yet", false);
                return null;
            }
        ));
        return extendedNamedXContents;
    }
}
