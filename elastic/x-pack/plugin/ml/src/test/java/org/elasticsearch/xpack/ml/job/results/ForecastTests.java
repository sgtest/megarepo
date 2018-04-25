/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.results;

import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.job.results.Forecast;

import java.io.IOException;
import java.util.Date;
import java.util.Objects;

import static org.hamcrest.Matchers.containsString;

public class ForecastTests extends AbstractSerializingTestCase<Forecast> {

    @Override
    protected Forecast createTestInstance() {
        return createTestInstance("ForecastTest");
    }

    public Forecast createTestInstance(String jobId) {
        Forecast forecast =
                new Forecast(jobId, randomAlphaOfLength(20), new Date(randomLong()),
                        randomNonNegativeLong(), randomInt());

        if (randomBoolean()) {
            forecast.setByFieldName(randomAlphaOfLengthBetween(1, 20));
        }
        if (randomBoolean()) {
            forecast.setByFieldValue(randomAlphaOfLengthBetween(1, 20));
        }
        if (randomBoolean()) {
            forecast.setPartitionFieldName(randomAlphaOfLengthBetween(1, 20));
        }
        if (randomBoolean()) {
            forecast.setPartitionFieldValue(randomAlphaOfLengthBetween(1, 20));
        }
        if (randomBoolean()) {
            forecast.setModelFeature(randomAlphaOfLengthBetween(1, 20));
        }
        if (randomBoolean()) {
            forecast.setForecastLower(randomDouble());
        }
        if (randomBoolean()) {
            forecast.setForecastUpper(randomDouble());
        }
        if (randomBoolean()) {
            forecast.setForecastPrediction(randomDouble());
        }

        return forecast;
    }

    @Override
    protected Reader<Forecast> instanceReader() {
        return Forecast::new;
    }

    @Override
    protected Forecast doParseInstance(XContentParser parser) {
        return Forecast.STRICT_PARSER.apply(parser, null);
    }

    public void testId() {
        Forecast forecast = new Forecast("job-foo", "222", new Date(100L), 60L, 2);
        String byFieldValue = null;
        String partitionFieldValue = null;

        int valuesHash = Objects.hash(byFieldValue, partitionFieldValue);
        assertEquals("job-foo_model_forecast_222_100_60_2_" + valuesHash + "_0", forecast.getId());

        int length = 0;
        if (randomBoolean()) {
            byFieldValue = randomAlphaOfLength(10);
            length += byFieldValue.length();
            forecast.setByFieldValue(byFieldValue);
        }
        if (randomBoolean()) {
            partitionFieldValue = randomAlphaOfLength(10);
            length += partitionFieldValue.length();
            forecast.setPartitionFieldValue(partitionFieldValue);
        }

        valuesHash = Objects.hash(byFieldValue, partitionFieldValue);
        assertEquals("job-foo_model_forecast_222_100_60_2_" + valuesHash + "_" + length, forecast.getId());
    }

    public void testStrictParser() throws IOException {
        String json = "{\"job_id\":\"job_1\", \"forecast_id\":\"forecast_1\", \"timestamp\":12354667, \"bucket_span\": 3600," +
                "\"detector_index\":3, \"foo\":\"bar\"}";
        try (XContentParser parser = createParser(JsonXContent.jsonXContent, json)) {
            IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                    () -> Forecast.STRICT_PARSER.apply(parser, null));

            assertThat(e.getMessage(), containsString("unknown field [foo]"));
        }
    }
}
