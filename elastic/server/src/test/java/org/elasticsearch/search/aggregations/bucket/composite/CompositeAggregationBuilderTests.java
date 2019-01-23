/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.search.aggregations.bucket.composite;

import org.elasticsearch.script.Script;
import org.elasticsearch.search.aggregations.BaseAggregationTestCase;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramInterval;
import org.elasticsearch.search.sort.SortOrder;

import java.util.ArrayList;
import java.util.List;

public class CompositeAggregationBuilderTests extends BaseAggregationTestCase<CompositeAggregationBuilder> {
    private DateHistogramValuesSourceBuilder randomDateHistogramSourceBuilder() {
        DateHistogramValuesSourceBuilder histo = new DateHistogramValuesSourceBuilder(randomAlphaOfLengthBetween(5, 10));
        if (randomBoolean()) {
            histo.field(randomAlphaOfLengthBetween(1, 20));
        } else {
            histo.script(new Script(randomAlphaOfLengthBetween(10, 20)));
        }
        if (randomBoolean()) {
            histo.dateHistogramInterval(randomFrom(DateHistogramInterval.days(10),
                DateHistogramInterval.minutes(1), DateHistogramInterval.weeks(1)));
        } else {
            histo.interval(randomNonNegativeLong());
        }
        if (randomBoolean()) {
            histo.timeZone(randomZone());
        }
        if (randomBoolean()) {
            histo.missingBucket(true);
        }
        return histo;
    }

    private TermsValuesSourceBuilder randomTermsSourceBuilder() {
        TermsValuesSourceBuilder terms = new TermsValuesSourceBuilder(randomAlphaOfLengthBetween(5, 10));
        if (randomBoolean()) {
            terms.field(randomAlphaOfLengthBetween(1, 20));
        } else {
            terms.script(new Script(randomAlphaOfLengthBetween(10, 20)));
        }
        terms.order(randomFrom(SortOrder.values()));
        if (randomBoolean()) {
            terms.missingBucket(true);
        }
        return terms;
    }

    private HistogramValuesSourceBuilder randomHistogramSourceBuilder() {
        HistogramValuesSourceBuilder histo = new HistogramValuesSourceBuilder(randomAlphaOfLengthBetween(5, 10));
        if (randomBoolean()) {
            histo.field(randomAlphaOfLengthBetween(1, 20));
        } else {
            histo.script(new Script(randomAlphaOfLengthBetween(10, 20)));
        }
        if (randomBoolean()) {
            histo.missingBucket(true);
        }
        histo.interval(randomDoubleBetween(Math.nextUp(0), Double.MAX_VALUE, false));
        return histo;
    }

    @Override
    protected CompositeAggregationBuilder createTestAggregatorBuilder() {
        int numSources = randomIntBetween(1, 10);
        List<CompositeValuesSourceBuilder<?>> sources = new ArrayList<>();
        for (int i = 0; i < numSources; i++) {
            int type = randomIntBetween(0, 2);
            switch (type) {
                case 0:
                    sources.add(randomTermsSourceBuilder());
                    break;
                case 1:
                    sources.add(randomDateHistogramSourceBuilder());
                    break;
                case 2:
                    sources.add(randomHistogramSourceBuilder());
                    break;
                default:
                    throw new AssertionError("wrong branch");
            }
        }
        return new CompositeAggregationBuilder(randomAlphaOfLength(10), sources);
    }
}
