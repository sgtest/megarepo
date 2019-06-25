/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.core.dataframe.transforms;

import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.dataframe.transforms.TimeSyncConfig;

import java.io.IOException;

public class TimeSyncConfigTests extends AbstractSerializingTestCase<TimeSyncConfig> {

    public static TimeSyncConfig randomTimeSyncConfig() {
        return new TimeSyncConfig(randomAlphaOfLengthBetween(1, 10), new TimeValue(randomNonNegativeLong()));
    }

    @Override
    protected TimeSyncConfig doParseInstance(XContentParser parser) throws IOException {
        return TimeSyncConfig.fromXContent(parser, false);
    }

    @Override
    protected TimeSyncConfig createTestInstance() {
        return randomTimeSyncConfig();
    }

    @Override
    protected Reader<TimeSyncConfig> instanceReader() {
        return TimeSyncConfig::new;
    }

}
