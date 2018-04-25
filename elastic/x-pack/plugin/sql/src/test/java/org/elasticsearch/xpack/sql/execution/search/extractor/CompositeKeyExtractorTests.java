/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.execution.search.extractor;

import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.bucket.MultiBucketsAggregation.Bucket;
import org.elasticsearch.test.AbstractWireSerializingTestCase;
import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.querydsl.container.GroupByRef.Property;
import org.joda.time.DateTime;
import org.joda.time.DateTimeZone;

import java.io.IOException;

import static java.util.Arrays.asList;
import static java.util.Collections.emptyList;
import static java.util.Collections.emptyMap;
import static java.util.Collections.singletonMap;

public class CompositeKeyExtractorTests extends AbstractWireSerializingTestCase<CompositeKeyExtractor> {

    public static CompositeKeyExtractor randomCompositeKeyExtractor() {
        return new CompositeKeyExtractor(randomAlphaOfLength(16), randomFrom(asList(Property.values())), randomTimeZone());
    }

    @Override
    protected CompositeKeyExtractor createTestInstance() {
        return randomCompositeKeyExtractor();
    }

    @Override
    protected Reader<CompositeKeyExtractor> instanceReader() {
        return CompositeKeyExtractor::new;
    }

    @Override
    protected CompositeKeyExtractor mutateInstance(CompositeKeyExtractor instance) throws IOException {
        return new CompositeKeyExtractor(instance.key() + "mutated", instance.property(), instance.timeZone());
    }

    public void testExtractBucketCount() {
        Bucket bucket = new TestBucket(emptyMap(), randomLong(), new Aggregations(emptyList()));
        CompositeKeyExtractor extractor = new CompositeKeyExtractor(randomAlphaOfLength(16), Property.COUNT,
                randomTimeZone());
        assertEquals(bucket.getDocCount(), extractor.extract(bucket));
    }

    public void testExtractKey() {
        CompositeKeyExtractor extractor = new CompositeKeyExtractor(randomAlphaOfLength(16), Property.VALUE, null);

        Object value = new Object();
        Bucket bucket = new TestBucket(singletonMap(extractor.key(), value), randomLong(), new Aggregations(emptyList()));
        assertEquals(value, extractor.extract(bucket));
    }

    public void testExtractDate() {
        CompositeKeyExtractor extractor = new CompositeKeyExtractor(randomAlphaOfLength(16), Property.VALUE, randomTimeZone());

        long millis = System.currentTimeMillis();
        Bucket bucket = new TestBucket(singletonMap(extractor.key(), millis), randomLong(), new Aggregations(emptyList()));
        assertEquals(new DateTime(millis, DateTimeZone.forTimeZone(extractor.timeZone())), extractor.extract(bucket));
    }

    public void testExtractIncorrectDateKey() {
        CompositeKeyExtractor extractor = new CompositeKeyExtractor(randomAlphaOfLength(16), Property.VALUE, randomTimeZone());

        Object value = new Object();
        Bucket bucket = new TestBucket(singletonMap(extractor.key(), value), randomLong(), new Aggregations(emptyList()));
        SqlIllegalArgumentException exception = expectThrows(SqlIllegalArgumentException.class, () -> extractor.extract(bucket));
        assertEquals("Invalid date key returned: " + value, exception.getMessage());
    }
}