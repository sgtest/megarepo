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

package org.elasticsearch.search.aggregations.bucket.histogram;

import org.apache.lucene.document.Document;
import org.apache.lucene.document.LongPoint;
import org.apache.lucene.document.SortedNumericDocValuesField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.store.Directory;
import org.elasticsearch.common.time.CompoundDateTimeFormatter;
import org.elasticsearch.common.time.DateFormatters;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.aggregations.BaseAggregationTestCase;
import org.elasticsearch.search.aggregations.BucketOrder;
import org.joda.time.DateTimeZone;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

public class DateHistogramTests extends BaseAggregationTestCase<DateHistogramAggregationBuilder> {

    @Override
    protected DateHistogramAggregationBuilder createTestAggregatorBuilder() {
        DateHistogramAggregationBuilder factory = new DateHistogramAggregationBuilder("foo");
        factory.field(INT_FIELD_NAME);
        if (randomBoolean()) {
            factory.interval(randomIntBetween(1, 100000));
        } else {
            if (randomBoolean()) {
                factory.dateHistogramInterval(randomFrom(DateHistogramInterval.YEAR, DateHistogramInterval.QUARTER,
                        DateHistogramInterval.MONTH, DateHistogramInterval.WEEK, DateHistogramInterval.DAY, DateHistogramInterval.HOUR,
                        DateHistogramInterval.MINUTE, DateHistogramInterval.SECOND));
            } else {
                int branch = randomInt(4);
                switch (branch) {
                case 0:
                    factory.dateHistogramInterval(DateHistogramInterval.seconds(randomIntBetween(1, 1000)));
                    break;
                case 1:
                    factory.dateHistogramInterval(DateHistogramInterval.minutes(randomIntBetween(1, 1000)));
                    break;
                case 2:
                    factory.dateHistogramInterval(DateHistogramInterval.hours(randomIntBetween(1, 1000)));
                    break;
                case 3:
                    factory.dateHistogramInterval(DateHistogramInterval.days(randomIntBetween(1, 1000)));
                    break;
                case 4:
                    factory.dateHistogramInterval(DateHistogramInterval.weeks(randomIntBetween(1, 1000)));
                    break;
                default:
                    throw new IllegalStateException("invalid branch: " + branch);
                }
            }
        }
        if (randomBoolean()) {
            factory.extendedBounds(ExtendedBoundsTests.randomExtendedBounds());
        }
        if (randomBoolean()) {
            factory.format("###.##");
        }
        if (randomBoolean()) {
            factory.keyed(randomBoolean());
        }
        if (randomBoolean()) {
            factory.minDocCount(randomIntBetween(0, 100));
        }
        if (randomBoolean()) {
            factory.missing(randomIntBetween(0, 10));
        }
        if (randomBoolean()) {
            factory.offset(randomIntBetween(0, 100000));
        }
        if (randomBoolean()) {
            List<BucketOrder> order = randomOrder();
            if(order.size() == 1 && randomBoolean()) {
                factory.order(order.get(0));
            } else {
                factory.order(order);
            }
        }
        return factory;
    }

    private List<BucketOrder> randomOrder() {
        List<BucketOrder> orders = new ArrayList<>();
        switch (randomInt(4)) {
            case 0:
                orders.add(BucketOrder.key(randomBoolean()));
                break;
            case 1:
                orders.add(BucketOrder.count(randomBoolean()));
                break;
            case 2:
                orders.add(BucketOrder.aggregation(randomAlphaOfLengthBetween(3, 20), randomBoolean()));
                break;
            case 3:
                orders.add(BucketOrder.aggregation(randomAlphaOfLengthBetween(3, 20), randomAlphaOfLengthBetween(3, 20), randomBoolean()));
                break;
            case 4:
                int numOrders = randomIntBetween(1, 3);
                for (int i = 0; i < numOrders; i++) {
                    orders.addAll(randomOrder());
                }
                break;
            default:
                fail();
        }
        return orders;
    }

    private static Document documentForDate(String field, long millis) {
        Document doc = new Document();
        doc.add(new LongPoint(field, millis));
        doc.add(new SortedNumericDocValuesField(field, millis));
        return doc;
    }

    public void testRewriteTimeZone() throws IOException {
        CompoundDateTimeFormatter format = DateFormatters.forPattern("strict_date_optional_time");

        try (Directory dir = newDirectory();
                IndexWriter w = new IndexWriter(dir, newIndexWriterConfig())) {

            long millis1 = DateFormatters.toZonedDateTime(format.parse("2018-03-11T11:55:00")).toInstant().toEpochMilli();
            w.addDocument(documentForDate(DATE_FIELD_NAME, millis1));
            long millis2 = DateFormatters.toZonedDateTime(format.parse("2017-10-30T18:13:00")).toInstant().toEpochMilli();
            w.addDocument(documentForDate(DATE_FIELD_NAME, millis2));

            try (IndexReader readerThatDoesntCross = DirectoryReader.open(w)) {

                long millis3 = DateFormatters.toZonedDateTime(format.parse("2018-03-25T02:44:00")).toInstant().toEpochMilli();
                w.addDocument(documentForDate(DATE_FIELD_NAME, millis3));

                try (IndexReader readerThatCrosses = DirectoryReader.open(w)) {

                    QueryShardContext shardContextThatDoesntCross = createShardContext(readerThatDoesntCross);
                    QueryShardContext shardContextThatCrosses = createShardContext(readerThatCrosses);

                    DateHistogramAggregationBuilder builder = new DateHistogramAggregationBuilder("my_date_histo");
                    builder.field(DATE_FIELD_NAME);
                    builder.dateHistogramInterval(DateHistogramInterval.DAY);

                    // no timeZone => no rewrite
                    assertNull(builder.rewriteTimeZone(shardContextThatDoesntCross));
                    assertNull(builder.rewriteTimeZone(shardContextThatCrosses));

                    // fixed timeZone => no rewrite
                    DateTimeZone tz = DateTimeZone.forOffsetHours(1);
                    builder.timeZone(tz);
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatDoesntCross));
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatCrosses));

                    // daylight-saving-times => rewrite if doesn't cross
                    tz = DateTimeZone.forID("Europe/Paris");
                    builder.timeZone(tz);
                    assertEquals(DateTimeZone.forOffsetHours(1), builder.rewriteTimeZone(shardContextThatDoesntCross));
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatCrosses));

                    // Rounded values are no longer all within the same transitions => no rewrite
                    builder.dateHistogramInterval(DateHistogramInterval.MONTH);
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatDoesntCross));
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatCrosses));

                    builder = new DateHistogramAggregationBuilder("my_date_histo");
                    builder.field(DATE_FIELD_NAME);
                    builder.timeZone(tz);

                    builder.interval(1000L * 60 * 60 * 24); // ~ 1 day
                    assertEquals(DateTimeZone.forOffsetHours(1), builder.rewriteTimeZone(shardContextThatDoesntCross));
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatCrosses));

                    // Because the interval is large, rounded values are not
                    // within the same transitions as the values => no rewrite
                    builder.interval(1000L * 60 * 60 * 24 * 30); // ~ 1 month
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatDoesntCross));
                    assertSame(tz, builder.rewriteTimeZone(shardContextThatCrosses));
                }
            }
        }
    }

}
