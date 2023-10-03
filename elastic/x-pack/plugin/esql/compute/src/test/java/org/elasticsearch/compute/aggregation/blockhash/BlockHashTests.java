/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.aggregation.blockhash;

import com.carrotsearch.randomizedtesting.annotations.ParametersFactory;

import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.breaker.CircuitBreaker;
import org.elasticsearch.common.inject.name.Named;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.MockBigArrays;
import org.elasticsearch.common.util.PageCacheRecycler;
import org.elasticsearch.compute.aggregation.GroupingAggregatorFunction;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BlockFactory;
import org.elasticsearch.compute.data.BooleanBlock;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.DoubleBlock;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.HashAggregationOperator;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.test.ESTestCase;
import org.junit.After;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Consumer;
import java.util.stream.IntStream;
import java.util.stream.LongStream;

import static org.hamcrest.Matchers.arrayWithSize;
import static org.hamcrest.Matchers.endsWith;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.startsWith;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class BlockHashTests extends ESTestCase {

    final CircuitBreaker breaker = new MockBigArrays.LimitedBreaker("esql-test-breaker", ByteSizeValue.ofGb(1));
    final BigArrays bigArrays = new MockBigArrays(PageCacheRecycler.NON_RECYCLING_INSTANCE, mockBreakerService(breaker));
    final BlockFactory blockFactory = BlockFactory.getInstance(breaker, bigArrays);

    @ParametersFactory
    public static List<Object[]> params() {
        List<Object[]> params = new ArrayList<>();
        params.add(new Object[] { false });
        params.add(new Object[] { true });
        return params;
    }

    @After
    public void checkBreaker() {
        assertThat(breaker.getUsed(), is(0L));
    }

    private final boolean forcePackedHash;

    public BlockHashTests(@Named("forcePackedHash") boolean forcePackedHash) {
        this.forcePackedHash = forcePackedHash;
    }

    public void testIntHash() {
        int[] values = new int[] { 1, 2, 3, 1, 2, 3, 1, 2, 3 };
        IntBlock block = BlockFactory.getNonBreakingInstance().newIntArrayVector(values, values.length).asBlock();

        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:INT], entries=3, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 0, 1, 2, 0, 1, 2);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("IntBlockHash{channel=0, entries=3, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 1, 2, 3, 1, 2, 3, 1, 2, 3);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(1, 4)));
        }
        assertKeys(ordsAndKeys.keys, 1, 2, 3);
        // we close these explicitly in the test. In common operation the operator is in charge of closing these.
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testIntHashWithNulls() {
        IntBlock.Builder builder = BlockFactory.getNonBreakingInstance().newIntBlockBuilder(4);
        builder.appendInt(0);
        builder.appendNull();
        builder.appendInt(2);
        builder.appendNull();

        OrdsAndKeys ordsAndKeys = hash(builder.build());
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:INT], entries=3, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 1);
            assertKeys(ordsAndKeys.keys, 0, null, 2);
        } else {
            assertThat(ordsAndKeys.description, equalTo("IntBlockHash{channel=0, entries=2, seenNull=true}"));
            assertOrds(ordsAndKeys.ords, 1, 0, 2, 0);
            assertKeys(ordsAndKeys.keys, null, 0, 2);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testIntHashWithMultiValuedFields() {
        var builder = BlockFactory.getNonBreakingInstance().newIntBlockBuilder(8);
        builder.appendInt(1);
        builder.beginPositionEntry();
        builder.appendInt(1);
        builder.appendInt(2);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendInt(3);
        builder.appendInt(1);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendInt(3);
        builder.appendInt(3);
        builder.endPositionEntry();
        builder.appendNull();
        builder.beginPositionEntry();
        builder.appendInt(3);
        builder.appendInt(2);
        builder.appendInt(1);
        builder.endPositionEntry();

        OrdsAndKeys ordsAndKeys = hash(builder.build());
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:INT], entries=4, size="));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0 },
                new int[] { 0, 1 },
                new int[] { 2, 0 },
                new int[] { 2 },
                new int[] { 3 },
                new int[] { 2, 1, 0 }
            );
            assertKeys(ordsAndKeys.keys, 1, 2, 3, null);
        } else {
            assertThat(ordsAndKeys.description, equalTo("IntBlockHash{channel=0, entries=3, seenNull=true}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 1 },
                new int[] { 1, 2 },
                new int[] { 3, 1 },
                new int[] { 3 },
                new int[] { 0 },
                new int[] { 3, 2, 1 }
            );
            assertKeys(ordsAndKeys.keys, null, 1, 2, 3);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongHash() {
        long[] values = new long[] { 2, 1, 4, 2, 4, 1, 3, 4 };
        LongBlock block = BlockFactory.getNonBreakingInstance().newLongArrayVector(values, values.length).asBlock();

        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG], entries=4, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 0, 2, 1, 3, 2);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("LongBlockHash{channel=0, entries=4, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 1, 2, 3, 1, 3, 2, 4, 3);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(1, 5)));
        }
        assertKeys(ordsAndKeys.keys, 2L, 1L, 4L, 3L);
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongHashWithNulls() {
        LongBlock.Builder builder = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(4);
        builder.appendLong(0);
        builder.appendNull();
        builder.appendLong(2);
        builder.appendNull();

        OrdsAndKeys ordsAndKeys = hash(builder.build());
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG], entries=3, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 1);
            assertKeys(ordsAndKeys.keys, 0L, null, 2L);
        } else {
            assertThat(ordsAndKeys.description, equalTo("LongBlockHash{channel=0, entries=2, seenNull=true}"));
            assertOrds(ordsAndKeys.ords, 1, 0, 2, 0);
            assertKeys(ordsAndKeys.keys, null, 0L, 2L);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongHashWithMultiValuedFields() {
        var builder = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(8);
        builder.appendLong(1);
        builder.beginPositionEntry();
        builder.appendLong(1);
        builder.appendLong(2);
        builder.appendLong(3);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendLong(1);
        builder.appendLong(1);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendLong(3);
        builder.endPositionEntry();
        builder.appendNull();
        builder.beginPositionEntry();
        builder.appendLong(3);
        builder.appendLong(2);
        builder.appendLong(1);
        builder.endPositionEntry();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG], entries=4, size="));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0 },
                new int[] { 0, 1, 2 },
                new int[] { 0 },
                new int[] { 2 },
                new int[] { 3 },
                new int[] { 2, 1, 0 }
            );
            assertKeys(ordsAndKeys.keys, 1L, 2L, 3L, null);
        } else {
            assertThat(ordsAndKeys.description, equalTo("LongBlockHash{channel=0, entries=3, seenNull=true}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 1 },
                new int[] { 1, 2, 3 },
                new int[] { 1 },
                new int[] { 3 },
                new int[] { 0 },
                new int[] { 3, 2, 1 }
            );
            assertKeys(ordsAndKeys.keys, null, 1L, 2L, 3L);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testDoubleHash() {
        double[] values = new double[] { 2.0, 1.0, 4.0, 2.0, 4.0, 1.0, 3.0, 4.0 };
        DoubleBlock block = BlockFactory.getNonBreakingInstance().newDoubleArrayVector(values, values.length).asBlock();
        OrdsAndKeys ordsAndKeys = hash(block);

        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:DOUBLE], entries=4, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 0, 2, 1, 3, 2);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("DoubleBlockHash{channel=0, entries=4, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 1, 2, 3, 1, 3, 2, 4, 3);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(1, 5)));
        }
        assertKeys(ordsAndKeys.keys, 2.0, 1.0, 4.0, 3.0);
        // we close these explicitly in the test. In common operation the operator is in charge of closing these.
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testDoubleHashWithNulls() {
        DoubleBlock.Builder builder = BlockFactory.getNonBreakingInstance().newDoubleBlockBuilder(4);
        builder.appendDouble(0);
        builder.appendNull();
        builder.appendDouble(2);
        builder.appendNull();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:DOUBLE], entries=3, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 1);
            assertKeys(ordsAndKeys.keys, 0.0, null, 2.0);
        } else {
            assertThat(ordsAndKeys.description, equalTo("DoubleBlockHash{channel=0, entries=2, seenNull=true}"));
            assertOrds(ordsAndKeys.ords, 1, 0, 2, 0);
            assertKeys(ordsAndKeys.keys, null, 0.0, 2.0);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testDoubleHashWithMultiValuedFields() {
        var builder = BlockFactory.getNonBreakingInstance().newDoubleBlockBuilder(8);
        builder.appendDouble(1);
        builder.beginPositionEntry();
        builder.appendDouble(2);
        builder.appendDouble(3);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendDouble(3);
        builder.appendDouble(2);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendDouble(1);
        builder.endPositionEntry();
        builder.appendNull();
        builder.beginPositionEntry();
        builder.appendDouble(1);
        builder.appendDouble(1);
        builder.appendDouble(2);
        builder.endPositionEntry();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:DOUBLE], entries=4, size="));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0 },
                new int[] { 1, 2 },
                new int[] { 2, 1 },
                new int[] { 0 },
                new int[] { 3 },
                new int[] { 0, 1 }
            );
            assertKeys(ordsAndKeys.keys, 1.0, 2.0, 3.0, null);
        } else {
            assertThat(ordsAndKeys.description, equalTo("DoubleBlockHash{channel=0, entries=3, seenNull=true}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 1 },
                new int[] { 2, 3 },
                new int[] { 3, 2 },
                new int[] { 1 },
                new int[] { 0 },
                new int[] { 1, 2 }
            );
            assertKeys(ordsAndKeys.keys, null, 1.0, 2.0, 3.0);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBasicBytesRefHash() {
        var builder = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(8);
        builder.appendBytesRef(new BytesRef("item-2"));
        builder.appendBytesRef(new BytesRef("item-1"));
        builder.appendBytesRef(new BytesRef("item-4"));
        builder.appendBytesRef(new BytesRef("item-2"));
        builder.appendBytesRef(new BytesRef("item-4"));
        builder.appendBytesRef(new BytesRef("item-1"));
        builder.appendBytesRef(new BytesRef("item-3"));
        builder.appendBytesRef(new BytesRef("item-4"));

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BYTES_REF], entries=4, size="));
            assertThat(ordsAndKeys.description, endsWith("b}"));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 0, 2, 1, 3, 2);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        } else {
            assertThat(ordsAndKeys.description, startsWith("BytesRefBlockHash{channel=0, entries=4, size="));
            assertThat(ordsAndKeys.description, endsWith("b, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 1, 2, 3, 1, 3, 2, 4, 3);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(1, 5)));
        }
        assertKeys(ordsAndKeys.keys, "item-2", "item-1", "item-4", "item-3");
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBytesRefHashWithNulls() {
        BytesRefBlock.Builder builder = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(4);
        builder.appendBytesRef(new BytesRef("cat"));
        builder.appendNull();
        builder.appendBytesRef(new BytesRef("dog"));
        builder.appendNull();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BYTES_REF], entries=3, size="));
            assertThat(ordsAndKeys.description, endsWith("b}"));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 1);
            assertKeys(ordsAndKeys.keys, "cat", null, "dog");
        } else {
            assertThat(ordsAndKeys.description, startsWith("BytesRefBlockHash{channel=0, entries=2, size="));
            assertThat(ordsAndKeys.description, endsWith("b, seenNull=true}"));
            assertOrds(ordsAndKeys.ords, 1, 0, 2, 0);
            assertKeys(ordsAndKeys.keys, null, "cat", "dog");
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBytesRefHashWithMultiValuedFields() {
        var builder = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(8);
        builder.appendBytesRef(new BytesRef("foo"));
        builder.beginPositionEntry();
        builder.appendBytesRef(new BytesRef("foo"));
        builder.appendBytesRef(new BytesRef("bar"));
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendBytesRef(new BytesRef("bar"));
        builder.appendBytesRef(new BytesRef("bort"));
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendBytesRef(new BytesRef("bort"));
        builder.appendBytesRef(new BytesRef("bar"));
        builder.endPositionEntry();
        builder.appendNull();
        builder.beginPositionEntry();
        builder.appendBytesRef(new BytesRef("bort"));
        builder.appendBytesRef(new BytesRef("bort"));
        builder.appendBytesRef(new BytesRef("bar"));
        builder.endPositionEntry();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BYTES_REF], entries=4, size="));
            assertThat(ordsAndKeys.description, endsWith("b}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0 },
                new int[] { 0, 1 },
                new int[] { 1, 2 },
                new int[] { 2, 1 },
                new int[] { 3 },
                new int[] { 2, 1 }
            );
            assertKeys(ordsAndKeys.keys, "foo", "bar", "bort", null);
        } else {
            assertThat(ordsAndKeys.description, startsWith("BytesRefBlockHash{channel=0, entries=3, size="));
            assertThat(ordsAndKeys.description, endsWith("b, seenNull=true}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 1 },
                new int[] { 1, 2 },
                new int[] { 2, 3 },
                new int[] { 3, 2 },
                new int[] { 0 },
                new int[] { 3, 2 }
            );
            assertKeys(ordsAndKeys.keys, null, "foo", "bar", "bort");
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBooleanHashFalseFirst() {
        boolean[] values = new boolean[] { false, true, true, true, true };
        BooleanBlock block = BlockFactory.getNonBreakingInstance().newBooleanArrayVector(values, values.length).asBlock();

        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BOOLEAN], entries=2, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 1, 1, 1);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 2)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("BooleanBlockHash{channel=0, seenFalse=true, seenTrue=true, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 1, 2, 2, 2, 2);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(1, 3)));
        }
        assertKeys(ordsAndKeys.keys, false, true);
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBooleanHashTrueFirst() {
        boolean[] values = new boolean[] { true, false, false, true, true };
        BooleanBlock block = BlockFactory.getNonBreakingInstance().newBooleanArrayVector(values, values.length).asBlock();

        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BOOLEAN], entries=2, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 1, 0, 0);
            assertKeys(ordsAndKeys.keys, true, false);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 2)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("BooleanBlockHash{channel=0, seenFalse=true, seenTrue=true, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 2, 1, 1, 2, 2);
            assertKeys(ordsAndKeys.keys, false, true);
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(1, 3)));
        }
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBooleanHashTrueOnly() {
        boolean[] values = new boolean[] { true, true, true, true };
        BooleanBlock block = BlockFactory.getNonBreakingInstance().newBooleanArrayVector(values, values.length).asBlock();

        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BOOLEAN], entries=1, size="));
            assertOrds(ordsAndKeys.ords, 0, 0, 0, 0);
            assertKeys(ordsAndKeys.keys, true);
            assertThat(ordsAndKeys.nonEmpty, equalTo(IntVector.newVectorBuilder(1).appendInt(0).build()));
        } else {
            assertThat(ordsAndKeys.description, equalTo("BooleanBlockHash{channel=0, seenFalse=false, seenTrue=true, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 2, 2, 2, 2);
            assertKeys(ordsAndKeys.keys, true);
            assertThat(ordsAndKeys.nonEmpty, equalTo(IntVector.newVectorBuilder(1).appendInt(2).build()));
        }
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBooleanHashFalseOnly() {
        boolean[] values = new boolean[] { false, false, false, false };
        BooleanBlock block = BlockFactory.getNonBreakingInstance().newBooleanArrayVector(values, values.length).asBlock();

        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BOOLEAN], entries=1, size="));
            assertOrds(ordsAndKeys.ords, 0, 0, 0, 0);
            assertThat(ordsAndKeys.nonEmpty, equalTo(IntVector.newVectorBuilder(1).appendInt(0).build()));
        } else {
            assertThat(ordsAndKeys.description, equalTo("BooleanBlockHash{channel=0, seenFalse=true, seenTrue=false, seenNull=false}"));
            assertOrds(ordsAndKeys.ords, 1, 1, 1, 1);
            assertThat(ordsAndKeys.nonEmpty, equalTo(IntVector.newVectorBuilder(1).appendInt(1).build()));
        }
        assertKeys(ordsAndKeys.keys, false);
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBooleanHashWithNulls() {
        BooleanBlock.Builder builder = BlockFactory.getNonBreakingInstance().newBooleanBlockBuilder(4);
        builder.appendBoolean(false);
        builder.appendNull();
        builder.appendBoolean(true);
        builder.appendNull();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BOOLEAN], entries=3, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 1);
            assertKeys(ordsAndKeys.keys, false, null, true);
        } else {
            assertThat(ordsAndKeys.description, equalTo("BooleanBlockHash{channel=0, seenFalse=true, seenTrue=true, seenNull=true}"));
            assertOrds(ordsAndKeys.ords, 1, 0, 2, 0);
            assertKeys(ordsAndKeys.keys, null, false, true);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBooleanHashWithMultiValuedFields() {
        var builder = BlockFactory.getNonBreakingInstance().newBooleanBlockBuilder(8);
        builder.appendBoolean(false);
        builder.beginPositionEntry();
        builder.appendBoolean(false);
        builder.appendBoolean(true);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendBoolean(true);
        builder.appendBoolean(false);
        builder.endPositionEntry();
        builder.beginPositionEntry();
        builder.appendBoolean(true);
        builder.endPositionEntry();
        builder.appendNull();
        builder.beginPositionEntry();
        builder.appendBoolean(true);
        builder.appendBoolean(true);
        builder.appendBoolean(false);
        builder.endPositionEntry();

        Block block = builder.build();
        OrdsAndKeys ordsAndKeys = hash(block);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:BOOLEAN], entries=3, size="));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0 },
                new int[] { 0, 1 },
                new int[] { 0, 1 },  // Order is not preserved
                new int[] { 1 },
                new int[] { 2 },
                new int[] { 0, 1 }
            );
            assertKeys(ordsAndKeys.keys, false, true, null);
        } else {
            assertThat(ordsAndKeys.description, equalTo("BooleanBlockHash{channel=0, seenFalse=true, seenTrue=true, seenNull=true}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 1 },
                new int[] { 1, 2 },
                new int[] { 1, 2 },  // Order is not preserved
                new int[] { 2 },
                new int[] { 0 },
                new int[] { 1, 2 }
            );
            assertKeys(ordsAndKeys.keys, null, false, true);
        }
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 3)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongLongHash() {
        long[] values1 = new long[] { 0, 1, 0, 1, 0, 1 };
        LongBlock block1 = BlockFactory.getNonBreakingInstance().newLongArrayVector(values1, values1.length).asBlock();
        long[] values2 = new long[] { 0, 0, 0, 1, 1, 1 };
        LongBlock block2 = BlockFactory.getNonBreakingInstance().newLongArrayVector(values2, values2.length).asBlock();
        Object[][] expectedKeys = { new Object[] { 0L, 0L }, new Object[] { 1L, 0L }, new Object[] { 1L, 1L }, new Object[] { 0L, 1L } };

        OrdsAndKeys ordsAndKeys = hash(block1, block2);
        assertThat(
            ordsAndKeys.description,
            forcePackedHash
                ? startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:LONG], entries=4, size=")
                : equalTo("LongLongBlockHash{channels=[0,1], entries=4}")
        );
        assertOrds(ordsAndKeys.ords, 0, 1, 0, 2, 3, 2);
        assertKeys(ordsAndKeys.keys, expectedKeys);
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    private void append(LongBlock.Builder b1, LongBlock.Builder b2, long[] v1, long[] v2) {
        if (v1 == null) {
            b1.appendNull();
        } else if (v1.length == 1) {
            b1.appendLong(v1[0]);
        } else {
            b1.beginPositionEntry();
            for (long v : v1) {
                b1.appendLong(v);
            }
            b1.endPositionEntry();
        }
        if (v2 == null) {
            b2.appendNull();
        } else if (v2.length == 1) {
            b2.appendLong(v2[0]);
        } else {
            b2.beginPositionEntry();
            for (long v : v2) {
                b2.appendLong(v);
            }
            b2.endPositionEntry();
        }
    }

    public void testLongLongHashWithMultiValuedFields() {
        var b1 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(8);
        var b2 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(8);
        append(b1, b2, new long[] { 1, 2 }, new long[] { 10, 20 });
        append(b1, b2, new long[] { 1, 2 }, new long[] { 10 });
        append(b1, b2, new long[] { 1 }, new long[] { 10, 20 });
        append(b1, b2, new long[] { 1 }, new long[] { 10 });
        append(b1, b2, null, new long[] { 10 });
        append(b1, b2, new long[] { 1 }, null);
        append(b1, b2, new long[] { 1, 1, 1 }, new long[] { 10, 10, 10 });
        append(b1, b2, new long[] { 1, 1, 2, 2 }, new long[] { 10, 20, 20 });
        append(b1, b2, new long[] { 1, 2, 3 }, new long[] { 30, 30, 10 });

        Block block1 = b1.build();
        Block block2 = b2.build();
        OrdsAndKeys ordsAndKeys = hash(block1, block2);
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:LONG], entries=10, size="));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0, 1, 2, 3 },
                new int[] { 0, 2 },
                new int[] { 0, 1 },
                new int[] { 0 },
                new int[] { 4 },
                new int[] { 5 },
                new int[] { 0 },
                new int[] { 0, 1, 2, 3 },
                new int[] { 6, 0, 7, 2, 8, 9 }
            );
            assertKeys(
                ordsAndKeys.keys,
                new Object[][] {
                    new Object[] { 1L, 10L },
                    new Object[] { 1L, 20L },
                    new Object[] { 2L, 10L },
                    new Object[] { 2L, 20L },
                    new Object[] { null, 10L },
                    new Object[] { 1L, null },
                    new Object[] { 1L, 30L },
                    new Object[] { 2L, 30L },
                    new Object[] { 3L, 30L },
                    new Object[] { 3L, 10L }, }
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 10)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("LongLongBlockHash{channels=[0,1], entries=8}"));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0, 1, 2, 3 },
                new int[] { 0, 2 },
                new int[] { 0, 1 },
                new int[] { 0 },
                null,
                null,
                new int[] { 0 },
                new int[] { 0, 1, 2, 3 },
                new int[] { 4, 0, 5, 2, 6, 7 }
            );
            assertKeys(
                ordsAndKeys.keys,
                new Object[][] {
                    new Object[] { 1L, 10L },
                    new Object[] { 1L, 20L },
                    new Object[] { 2L, 10L },
                    new Object[] { 2L, 20L },
                    new Object[] { 1L, 30L },
                    new Object[] { 2L, 30L },
                    new Object[] { 3L, 30L },
                    new Object[] { 3L, 10L }, }
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 8)));
        }
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongLongHashHugeCombinatorialExplosion() {
        long[] v1 = LongStream.range(0, 5000).toArray();
        long[] v2 = LongStream.range(100, 200).toArray();

        var b1 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(v1.length);
        var b2 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(v2.length);
        append(b1, b2, v1, v2);
        var block1 = b1.build();
        var block2 = b2.build();

        int[] expectedEntries = new int[1];
        int pageSize = between(1000, 16 * 1024);
        hash(ordsAndKeys -> {
            int start = expectedEntries[0];
            expectedEntries[0] = Math.min(expectedEntries[0] + pageSize, v1.length * v2.length);
            assertThat(
                ordsAndKeys.description,
                forcePackedHash
                    ? startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:LONG], entries=" + expectedEntries[0] + ", size=")
                    : equalTo("LongLongBlockHash{channels=[0,1], entries=" + expectedEntries[0] + "}")
            );
            assertOrds(ordsAndKeys.ords, IntStream.range(start, expectedEntries[0]).toArray());
            assertKeys(
                ordsAndKeys.keys,
                IntStream.range(0, expectedEntries[0])
                    .mapToObj(i -> new Object[] { v1[i / v2.length], v2[i % v2.length] })
                    .toArray(l -> new Object[l][])
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, expectedEntries[0])));
            Releasables.closeExpectNoException(ordsAndKeys.keys);
            Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        }, pageSize, block1, block2);

        assertThat("misconfigured test", expectedEntries[0], greaterThan(0));
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testIntLongHash() {
        int[] values1 = new int[] { 0, 1, 0, 1, 0, 1 };
        IntBlock block1 = BlockFactory.getNonBreakingInstance().newIntArrayVector(values1, values1.length).asBlock();
        long[] values2 = new long[] { 0, 0, 0, 1, 1, 1 };
        LongBlock block2 = BlockFactory.getNonBreakingInstance().newLongArrayVector(values2, values2.length).asBlock();
        Object[][] expectedKeys = { new Object[] { 0, 0L }, new Object[] { 1, 0L }, new Object[] { 1, 1L }, new Object[] { 0, 1L } };

        OrdsAndKeys ordsAndKeys = hash(block1, block2);
        assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:INT, 1:LONG], entries=4, size="));
        assertThat(ordsAndKeys.description, endsWith("b}"));
        assertOrds(ordsAndKeys.ords, 0, 1, 0, 2, 3, 2);
        assertKeys(ordsAndKeys.keys, expectedKeys);
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongDoubleHash() {
        long[] values1 = new long[] { 0, 1, 0, 1, 0, 1 };
        LongBlock block1 = BlockFactory.getNonBreakingInstance().newLongArrayVector(values1, values1.length).asBlock();
        double[] values2 = new double[] { 0, 0, 0, 1, 1, 1 };
        DoubleBlock block2 = BlockFactory.getNonBreakingInstance().newDoubleArrayVector(values2, values2.length).asBlock();
        Object[][] expectedKeys = { new Object[] { 0L, 0d }, new Object[] { 1L, 0d }, new Object[] { 1L, 1d }, new Object[] { 0L, 1d } };
        OrdsAndKeys ordsAndKeys = hash(block1, block2);
        assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:DOUBLE], entries=4, size="));
        assertThat(ordsAndKeys.description, endsWith("b}"));
        assertOrds(ordsAndKeys.ords, 0, 1, 0, 2, 3, 2);
        assertKeys(ordsAndKeys.keys, expectedKeys);
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testIntBooleanHash() {
        int[] values1 = new int[] { 0, 1, 0, 1, 0, 1 };
        IntBlock block1 = BlockFactory.getNonBreakingInstance().newIntArrayVector(values1, values1.length).asBlock();
        boolean[] values2 = new boolean[] { false, false, false, true, true, true };
        BooleanBlock block2 = BlockFactory.getNonBreakingInstance().newBooleanArrayVector(values2, values2.length).asBlock();
        Object[][] expectedKeys = {
            new Object[] { 0, false },
            new Object[] { 1, false },
            new Object[] { 1, true },
            new Object[] { 0, true } };

        OrdsAndKeys ordsAndKeys = hash(block1, block2);
        assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:INT, 1:BOOLEAN], entries=4, size="));
        assertThat(ordsAndKeys.description, endsWith("b}"));
        assertOrds(ordsAndKeys.ords, 0, 1, 0, 2, 3, 2);
        assertKeys(ordsAndKeys.keys, expectedKeys);
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongLongHashWithNull() {
        LongBlock.Builder b1 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(2);
        LongBlock.Builder b2 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(2);
        b1.appendLong(1);
        b2.appendLong(0);
        b1.appendNull();
        b2.appendNull();
        b1.appendLong(0);
        b2.appendLong(1);
        b1.appendLong(0);
        b2.appendNull();
        b1.appendNull();
        b2.appendLong(0);

        OrdsAndKeys ordsAndKeys = hash(b1.build(), b2.build());
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:LONG], entries=5, size="));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 3, 4);
            assertKeys(
                ordsAndKeys.keys,
                new Object[][] {
                    new Object[] { 1L, 0L },
                    new Object[] { null, null },
                    new Object[] { 0L, 1L },
                    new Object[] { 0L, null },
                    new Object[] { null, 0L }, }
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 5)));
        } else {
            assertThat(ordsAndKeys.description, equalTo("LongLongBlockHash{channels=[0,1], entries=2}"));
            assertOrds(ordsAndKeys.ords, 0, null, 1, null, null);
            assertKeys(ordsAndKeys.keys, new Object[][] { new Object[] { 1L, 0L }, new Object[] { 0L, 1L } });
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 2)));
        }
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
    }

    public void testLongBytesRefHash() {
        long[] values1 = new long[] { 0, 1, 0, 1, 0, 1 };
        LongBlock block1 = BlockFactory.getNonBreakingInstance().newLongArrayVector(values1, values1.length).asBlock();
        BytesRefBlock.Builder builder = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(8);
        builder.appendBytesRef(new BytesRef("cat"));
        builder.appendBytesRef(new BytesRef("cat"));
        builder.appendBytesRef(new BytesRef("cat"));
        builder.appendBytesRef(new BytesRef("dog"));
        builder.appendBytesRef(new BytesRef("dog"));
        builder.appendBytesRef(new BytesRef("dog"));
        BytesRefBlock block2 = builder.build();
        Object[][] expectedKeys = {
            new Object[] { 0L, "cat" },
            new Object[] { 1L, "cat" },
            new Object[] { 1L, "dog" },
            new Object[] { 0L, "dog" } };

        OrdsAndKeys ordsAndKeys = hash(block1, block2);
        assertThat(
            ordsAndKeys.description,
            startsWith(
                forcePackedHash
                    ? "PackedValuesBlockHash{groups=[0:LONG, 1:BYTES_REF], entries=4, size="
                    : "BytesRefLongBlockHash{keys=[BytesRefKey[channel=1], LongKey[channel=0]], entries=4, size="
            )
        );
        assertThat(ordsAndKeys.description, endsWith("b}"));
        assertOrds(ordsAndKeys.ords, 0, 1, 0, 2, 3, 2);
        assertKeys(ordsAndKeys.keys, expectedKeys);
        assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 4)));
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testLongBytesRefHashWithNull() {
        LongBlock.Builder b1 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(2);
        BytesRefBlock.Builder b2 = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(2);
        b1.appendLong(1);
        b2.appendBytesRef(new BytesRef("cat"));
        b1.appendNull();
        b2.appendNull();
        b1.appendLong(0);
        b2.appendBytesRef(new BytesRef("dog"));
        b1.appendLong(0);
        b2.appendNull();
        b1.appendNull();
        b2.appendBytesRef(new BytesRef("vanish"));

        OrdsAndKeys ordsAndKeys = hash(b1.build(), b2.build());
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:BYTES_REF], entries=5, size="));
            assertThat(ordsAndKeys.description, endsWith("b}"));
            assertOrds(ordsAndKeys.ords, 0, 1, 2, 3, 4);
            assertKeys(
                ordsAndKeys.keys,
                new Object[][] {
                    new Object[] { 1L, "cat" },
                    new Object[] { null, null },
                    new Object[] { 0L, "dog" },
                    new Object[] { 1L, null },
                    new Object[] { null, "vanish" } }
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 5)));
        } else {
            assertThat(
                ordsAndKeys.description,
                startsWith("BytesRefLongBlockHash{keys=[BytesRefKey[channel=1], LongKey[channel=0]], entries=2, size=")
            );
            assertThat(ordsAndKeys.description, endsWith("b}"));
            assertOrds(ordsAndKeys.ords, 0, null, 1, null, null);
            assertKeys(ordsAndKeys.keys, new Object[][] { new Object[] { 1L, "cat" }, new Object[] { 0L, "dog" } });
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 2)));
        }
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    private void append(LongBlock.Builder b1, BytesRefBlock.Builder b2, long[] v1, String[] v2) {
        if (v1 == null) {
            b1.appendNull();
        } else if (v1.length == 1) {
            b1.appendLong(v1[0]);
        } else {
            b1.beginPositionEntry();
            for (long v : v1) {
                b1.appendLong(v);
            }
            b1.endPositionEntry();
        }
        if (v2 == null) {
            b2.appendNull();
        } else if (v2.length == 1) {
            b2.appendBytesRef(new BytesRef(v2[0]));
        } else {
            b2.beginPositionEntry();
            for (String v : v2) {
                b2.appendBytesRef(new BytesRef(v));
            }
            b2.endPositionEntry();
        }
    }

    public void testLongBytesRefHashWithMultiValuedFields() {
        var b1 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(8);
        var b2 = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(8);
        append(b1, b2, new long[] { 1, 2 }, new String[] { "a", "b" });
        append(b1, b2, new long[] { 1, 2 }, new String[] { "a" });
        append(b1, b2, new long[] { 1 }, new String[] { "a", "b" });
        append(b1, b2, new long[] { 1 }, new String[] { "a" });
        append(b1, b2, null, new String[] { "a" });
        append(b1, b2, new long[] { 1 }, null);
        append(b1, b2, new long[] { 1, 1, 1 }, new String[] { "a", "a", "a" });
        append(b1, b2, new long[] { 1, 1, 2, 2 }, new String[] { "a", "b", "b" });
        append(b1, b2, new long[] { 1, 2, 3 }, new String[] { "c", "c", "a" });

        OrdsAndKeys ordsAndKeys = hash(b1.build(), b2.build());
        if (forcePackedHash) {
            assertThat(ordsAndKeys.description, startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:BYTES_REF], entries=10, size="));
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0, 1, 2, 3 },
                new int[] { 0, 2 },
                new int[] { 0, 1 },
                new int[] { 0 },
                new int[] { 4 },
                new int[] { 5 },
                new int[] { 0 },
                new int[] { 0, 1, 2, 3 },
                new int[] { 6, 0, 7, 2, 8, 9 }
            );
            assertKeys(
                ordsAndKeys.keys,
                new Object[][] {
                    new Object[] { 1L, "a" },
                    new Object[] { 1L, "b" },
                    new Object[] { 2L, "a" },
                    new Object[] { 2L, "b" },
                    new Object[] { null, "a" },
                    new Object[] { 1L, null },
                    new Object[] { 1L, "c" },
                    new Object[] { 2L, "c" },
                    new Object[] { 3L, "c" },
                    new Object[] { 3L, "a" }, }
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 10)));
        } else {
            assertThat(
                ordsAndKeys.description,
                equalTo("BytesRefLongBlockHash{keys=[BytesRefKey[channel=1], LongKey[channel=0]], entries=8, size=491b}")
            );
            assertOrds(
                ordsAndKeys.ords,
                new int[] { 0, 1, 2, 3 },
                new int[] { 0, 1 },
                new int[] { 0, 2 },
                new int[] { 0 },
                null,
                null,
                new int[] { 0 },
                new int[] { 0, 1, 2, 3 },
                new int[] { 4, 5, 6, 0, 1, 7 }
            );
            assertKeys(
                ordsAndKeys.keys,
                new Object[][] {
                    new Object[] { 1L, "a" },
                    new Object[] { 2L, "a" },
                    new Object[] { 1L, "b" },
                    new Object[] { 2L, "b" },
                    new Object[] { 1L, "c" },
                    new Object[] { 2L, "c" },
                    new Object[] { 3L, "c" },
                    new Object[] { 3L, "a" }, }
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, 8)));
        }
        Releasables.closeExpectNoException(ordsAndKeys.keys);
        Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        assertThat(breaker.getUsed(), is(0L));
    }

    public void testBytesRefLongHashHugeCombinatorialExplosion() {
        long[] v1 = LongStream.range(0, 3000).toArray();
        String[] v2 = LongStream.range(100, 200).mapToObj(l -> "a" + l).toArray(String[]::new);

        var b1 = BlockFactory.getNonBreakingInstance().newLongBlockBuilder(v1.length);
        var b2 = BlockFactory.getNonBreakingInstance().newBytesRefBlockBuilder(v2.length);
        append(b1, b2, v1, v2);

        int[] expectedEntries = new int[1];
        int pageSize = between(1000, 16 * 1024);
        hash(ordsAndKeys -> {
            int start = expectedEntries[0];
            expectedEntries[0] = Math.min(expectedEntries[0] + pageSize, v1.length * v2.length);
            assertThat(
                ordsAndKeys.description,
                forcePackedHash
                    ? startsWith("PackedValuesBlockHash{groups=[0:LONG, 1:BYTES_REF], entries=" + expectedEntries[0] + ", size=")
                    : startsWith(
                        "BytesRefLongBlockHash{keys=[BytesRefKey[channel=1], LongKey[channel=0]], entries=" + expectedEntries[0] + ", size="
                    )
            );
            assertOrds(ordsAndKeys.ords, IntStream.range(start, expectedEntries[0]).toArray());
            assertKeys(
                ordsAndKeys.keys,
                IntStream.range(0, expectedEntries[0])
                    .mapToObj(
                        i -> forcePackedHash
                            ? new Object[] { v1[i / v2.length], v2[i % v2.length] }
                            : new Object[] { v1[i % v1.length], v2[i / v1.length] }
                    )
                    .toArray(l -> new Object[l][])
            );
            assertThat(ordsAndKeys.nonEmpty, equalTo(intRange(0, expectedEntries[0])));
            Releasables.closeExpectNoException(ordsAndKeys.keys);
            Releasables.closeExpectNoException(ordsAndKeys.nonEmpty);
        }, pageSize, b1.build(), b2.build());

        assertThat("misconfigured test", expectedEntries[0], greaterThan(0));
        assertThat(breaker.getUsed(), is(0L));
    }

    record OrdsAndKeys(String description, int positionOffset, IntBlock ords, Block[] keys, IntVector nonEmpty) {}

    /**
     * Hash some values into a single block of group ids. If the hash produces
     * more than one block of group ids this will fail.
     */
    private OrdsAndKeys hash(Block... values) {
        OrdsAndKeys[] result = new OrdsAndKeys[1];
        hash(ordsAndKeys -> {
            if (result[0] != null) {
                throw new IllegalStateException("hash produced more than one block");
            }
            result[0] = ordsAndKeys;
        }, 16 * 1024, values);
        return result[0];
    }

    private void hash(Consumer<OrdsAndKeys> callback, int emitBatchSize, Block... values) {
        List<HashAggregationOperator.GroupSpec> specs = new ArrayList<>(values.length);
        for (int c = 0; c < values.length; c++) {
            specs.add(new HashAggregationOperator.GroupSpec(c, values[c].elementType()));
        }
        DriverContext driverContext = new DriverContext(bigArrays, blockFactory);
        try (
            BlockHash blockHash = forcePackedHash
                ? new PackedValuesBlockHash(specs, driverContext, emitBatchSize)
                : BlockHash.build(specs, driverContext, emitBatchSize, true)
        ) {
            hash(true, blockHash, callback, values);
        }
    }

    static void hash(boolean collectKeys, BlockHash blockHash, Consumer<OrdsAndKeys> callback, Block... values) {
        blockHash.add(new Page(values), new GroupingAggregatorFunction.AddInput() {
            @Override
            public void add(int positionOffset, IntBlock groupIds) {
                OrdsAndKeys result = new OrdsAndKeys(
                    blockHash.toString(),
                    positionOffset,
                    groupIds,
                    collectKeys ? blockHash.getKeys() : null,
                    blockHash.nonEmpty()
                );

                Set<Integer> allowedOrds = new HashSet<>();
                for (int p = 0; p < result.nonEmpty.getPositionCount(); p++) {
                    allowedOrds.add(result.nonEmpty.getInt(p));
                }
                for (int p = 0; p < result.ords.getPositionCount(); p++) {
                    if (result.ords.isNull(p)) {
                        continue;
                    }
                    int start = result.ords.getFirstValueIndex(p);
                    int end = start + result.ords.getValueCount(p);
                    for (int i = start; i < end; i++) {
                        int ord = result.ords.getInt(i);
                        if (false == allowedOrds.contains(ord)) {
                            fail("ord is not allowed " + ord);
                        }
                    }
                }
                callback.accept(result);
            }

            @Override
            public void add(int positionOffset, IntVector groupIds) {
                add(positionOffset, groupIds.asBlock());
            }
        });
    }

    private void assertOrds(IntBlock ordsBlock, Integer... expectedOrds) {
        assertOrds(ordsBlock, Arrays.stream(expectedOrds).map(l -> l == null ? null : new int[] { l }).toArray(int[][]::new));
    }

    private void assertOrds(IntBlock ordsBlock, int[]... expectedOrds) {
        assertEquals(expectedOrds.length, ordsBlock.getPositionCount());
        for (int p = 0; p < expectedOrds.length; p++) {
            int start = ordsBlock.getFirstValueIndex(p);
            int count = ordsBlock.getValueCount(p);
            if (expectedOrds[p] == null) {
                if (false == ordsBlock.isNull(p)) {
                    StringBuilder error = new StringBuilder();
                    error.append(p);
                    error.append(": expected null but was [");
                    for (int i = 0; i < count; i++) {
                        if (i != 0) {
                            error.append(", ");
                        }
                        error.append(ordsBlock.getInt(start + i));
                    }
                    fail(error.append("]").toString());
                }
                continue;
            }
            assertFalse(p + ": expected not null", ordsBlock.isNull(p));
            int[] actual = new int[count];
            for (int i = 0; i < count; i++) {
                actual[i] = ordsBlock.getInt(start + i);
            }
            assertThat("position " + p, actual, equalTo(expectedOrds[p]));
        }
    }

    private void assertKeys(Block[] actualKeys, Object... expectedKeys) {
        Object[][] flipped = new Object[expectedKeys.length][];
        for (int r = 0; r < flipped.length; r++) {
            flipped[r] = new Object[] { expectedKeys[r] };
        }
        assertKeys(actualKeys, flipped);
    }

    private void assertKeys(Block[] actualKeys, Object[][] expectedKeys) {
        for (int r = 0; r < expectedKeys.length; r++) {
            assertThat(actualKeys, arrayWithSize(expectedKeys[r].length));
        }
        for (int c = 0; c < actualKeys.length; c++) {
            assertThat("block " + c, actualKeys[c].getPositionCount(), equalTo(expectedKeys.length));
        }
        for (int r = 0; r < expectedKeys.length; r++) {
            for (int c = 0; c < actualKeys.length; c++) {
                if (expectedKeys[r][c] == null) {
                    assertThat("expected null", actualKeys[c].isNull(r), equalTo(true));
                    return;
                }
                assertThat(actualKeys[c].isNull(r), equalTo(false));
                if (expectedKeys[r][c] instanceof Integer v) {
                    assertThat(((IntBlock) actualKeys[c]).getInt(r), equalTo(v));
                } else if (expectedKeys[r][c] instanceof Long v) {
                    assertThat(((LongBlock) actualKeys[c]).getLong(r), equalTo(v));
                } else if (expectedKeys[r][c] instanceof Double v) {
                    assertThat(((DoubleBlock) actualKeys[c]).getDouble(r), equalTo(v));
                } else if (expectedKeys[r][c] instanceof String v) {
                    assertThat(((BytesRefBlock) actualKeys[c]).getBytesRef(r, new BytesRef()), equalTo(new BytesRef(v)));
                } else if (expectedKeys[r][c] instanceof Boolean v) {
                    assertThat(((BooleanBlock) actualKeys[c]).getBoolean(r), equalTo(v));
                } else {
                    throw new IllegalArgumentException("unsupported type " + expectedKeys[r][c].getClass());
                }
            }
        }
    }

    // A breaker service that always returns the given breaker for getBreaker(CircuitBreaker.REQUEST)
    static CircuitBreakerService mockBreakerService(CircuitBreaker breaker) {
        CircuitBreakerService breakerService = mock(CircuitBreakerService.class);
        when(breakerService.getBreaker(CircuitBreaker.REQUEST)).thenReturn(breaker);
        return breakerService;
    }

    IntVector intRange(int startInclusive, int endExclusive) {
        return IntVector.range(startInclusive, endExclusive, BlockFactory.getNonBreakingInstance());
    }
}
