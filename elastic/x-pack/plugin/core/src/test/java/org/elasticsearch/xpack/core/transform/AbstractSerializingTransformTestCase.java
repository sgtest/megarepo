/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.transform;

import org.elasticsearch.Version;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.ToXContent.Params;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.BaseAggregationBuilder;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.transform.transforms.RetentionPolicyConfig;
import org.elasticsearch.xpack.core.transform.transforms.SyncConfig;
import org.elasticsearch.xpack.core.transform.transforms.TimeRetentionPolicyConfig;
import org.elasticsearch.xpack.core.transform.transforms.TimeSyncConfig;
import org.junit.Before;

import java.io.IOException;
import java.util.Collections;
import java.util.List;

import static java.util.Collections.emptyList;

public abstract class AbstractSerializingTransformTestCase<T extends ToXContent & Writeable> extends AbstractSerializingTestCase<T> {

    protected static Params TO_XCONTENT_PARAMS = new ToXContent.MapParams(
        Collections.singletonMap(TransformField.FOR_INTERNAL_STORAGE, "true")
    );

    private NamedWriteableRegistry namedWriteableRegistry;
    private NamedXContentRegistry namedXContentRegistry;

    @Before
    public void registerNamedObjects() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, emptyList());

        List<NamedWriteableRegistry.Entry> namedWriteables = searchModule.getNamedWriteables();
        namedWriteables.add(
            new NamedWriteableRegistry.Entry(QueryBuilder.class, MockDeprecatedQueryBuilder.NAME, MockDeprecatedQueryBuilder::new)
        );
        namedWriteables.add(
            new NamedWriteableRegistry.Entry(
                AggregationBuilder.class,
                MockDeprecatedAggregationBuilder.NAME,
                MockDeprecatedAggregationBuilder::new
            )
        );
        namedWriteables.add(
            new NamedWriteableRegistry.Entry(SyncConfig.class, TransformField.TIME.getPreferredName(), TimeSyncConfig::new)
        );
        namedWriteables.add(
            new NamedWriteableRegistry.Entry(
                RetentionPolicyConfig.class,
                TransformField.TIME.getPreferredName(),
                TimeRetentionPolicyConfig::new
            )
        );

        List<NamedXContentRegistry.Entry> namedXContents = searchModule.getNamedXContents();
        namedXContents.add(
            new NamedXContentRegistry.Entry(
                QueryBuilder.class,
                new ParseField(MockDeprecatedQueryBuilder.NAME),
                (p, c) -> MockDeprecatedQueryBuilder.fromXContent(p)
            )
        );
        namedXContents.add(
            new NamedXContentRegistry.Entry(
                BaseAggregationBuilder.class,
                new ParseField(MockDeprecatedAggregationBuilder.NAME),
                (p, c) -> MockDeprecatedAggregationBuilder.fromXContent(p)
            )
        );

        namedXContents.addAll(new TransformNamedXContentProvider().getNamedXContentParsers());

        namedWriteableRegistry = new NamedWriteableRegistry(namedWriteables);
        namedXContentRegistry = new NamedXContentRegistry(namedXContents);
    }

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return namedWriteableRegistry;
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        return namedXContentRegistry;
    }

    protected <X extends Writeable, Y extends Writeable> Y writeAndReadBWCObject(
        X original,
        NamedWriteableRegistry namedWriteableRegistry,
        Writeable.Writer<X> writer,
        Writeable.Reader<Y> reader,
        Version version
    ) throws IOException {
        try (BytesStreamOutput output = new BytesStreamOutput()) {
            output.setVersion(version);
            original.writeTo(output);

            try (StreamInput in = new NamedWriteableAwareStreamInput(output.bytes().streamInput(), getNamedWriteableRegistry())) {
                in.setVersion(version);
                return reader.read(in);
            }
        }
    }

}
