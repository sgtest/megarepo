/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.data;

import org.elasticsearch.common.Randomness;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.test.ESTestCase;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;

public class DocVectorTests extends ESTestCase {
    public void testNonDecreasingSetTrue() {
        int length = between(1, 100);
        DocVector docs = new DocVector(intRange(0, length), intRange(0, length), intRange(0, length), true);
        assertTrue(docs.singleSegmentNonDecreasing());
    }

    public void testNonDecreasingSetFalse() {
        DocVector docs = new DocVector(intRange(0, 2), intRange(0, 2), new IntArrayVector(new int[] { 1, 0 }, 2), false);
        assertFalse(docs.singleSegmentNonDecreasing());
    }

    public void testNonDecreasingNonConstantShard() {
        DocVector docs = new DocVector(intRange(0, 2), IntBlock.newConstantBlockWith(0, 2).asVector(), intRange(0, 2), null);
        assertFalse(docs.singleSegmentNonDecreasing());
    }

    public void testNonDecreasingNonConstantSegment() {
        DocVector docs = new DocVector(IntBlock.newConstantBlockWith(0, 2).asVector(), intRange(0, 2), intRange(0, 2), null);
        assertFalse(docs.singleSegmentNonDecreasing());
    }

    public void testNonDecreasingDescendingDocs() {
        DocVector docs = new DocVector(
            IntBlock.newConstantBlockWith(0, 2).asVector(),
            IntBlock.newConstantBlockWith(0, 2).asVector(),
            new IntArrayVector(new int[] { 1, 0 }, 2),
            null
        );
        assertFalse(docs.singleSegmentNonDecreasing());
    }

    public void testShardSegmentDocMap() {
        assertShardSegmentDocMap(
            new int[][] {
                new int[] { 1, 0, 0 },
                new int[] { 1, 1, 1 },
                new int[] { 1, 1, 0 },
                new int[] { 0, 0, 2 },
                new int[] { 0, 1, 1 },
                new int[] { 0, 1, 0 },
                new int[] { 0, 2, 1 },
                new int[] { 0, 2, 0 },
                new int[] { 0, 2, 2 },
                new int[] { 0, 2, 3 }, },
            new int[][] {
                new int[] { 0, 0, 2 },
                new int[] { 0, 1, 0 },
                new int[] { 0, 1, 1 },
                new int[] { 0, 2, 0 },
                new int[] { 0, 2, 1 },
                new int[] { 0, 2, 2 },
                new int[] { 0, 2, 3 },
                new int[] { 1, 0, 0 },
                new int[] { 1, 1, 0 },
                new int[] { 1, 1, 1 }, }
        );
    }

    public void testRandomShardSegmentDocMap() {
        int[][] tracker = new int[5][];
        for (int shard = 0; shard < 5; shard++) {
            tracker[shard] = new int[] { 0, 0, 0, 0, 0 };
        }
        List<int[]> data = new ArrayList<>();
        for (int r = 0; r < 10000; r++) {
            int shard = between(0, 4);
            int segment = between(0, 4);
            data.add(new int[] { shard, segment, tracker[shard][segment]++ });
        }
        Randomness.shuffle(data);

        List<int[]> sorted = new ArrayList<>(data);
        Collections.sort(sorted, Comparator.comparing((int[] r) -> r[0]).thenComparing(r -> r[1]).thenComparing(r -> r[2]));
        assertShardSegmentDocMap(data.toArray(int[][]::new), sorted.toArray(int[][]::new));
    }

    private void assertShardSegmentDocMap(int[][] data, int[][] expected) {
        BlockFactory blockFactory = BlockFactoryTests.blockFactory(ByteSizeValue.ofGb(1));
        try (DocBlock.Builder builder = DocBlock.newBlockBuilder(data.length, blockFactory)) {
            for (int r = 0; r < data.length; r++) {
                builder.appendShard(data[r][0]);
                builder.appendSegment(data[r][1]);
                builder.appendDoc(data[r][2]);
            }
            try (DocVector docVector = builder.build().asVector()) {
                int[] forwards = docVector.shardSegmentDocMapForwards();

                int[][] result = new int[docVector.getPositionCount()][];
                for (int p = 0; p < result.length; p++) {
                    result[p] = new int[] {
                        docVector.shards().getInt(forwards[p]),
                        docVector.segments().getInt(forwards[p]),
                        docVector.docs().getInt(forwards[p]) };
                }
                assertThat(result, equalTo(expected));

                int[] backwards = docVector.shardSegmentDocMapBackwards();
                for (int p = 0; p < result.length; p++) {
                    result[p] = new int[] {
                        docVector.shards().getInt(backwards[forwards[p]]),
                        docVector.segments().getInt(backwards[forwards[p]]),
                        docVector.docs().getInt(backwards[forwards[p]]) };
                }

                assertThat(result, equalTo(data));
            }
        }
        assertThat(blockFactory.breaker().getUsed(), equalTo(0L));
    }

    public void testCannotDoubleRelease() {
        var block = new DocVector(intRange(0, 2), IntBlock.newConstantBlockWith(0, 2).asVector(), intRange(0, 2), null).asBlock();
        assertThat(block.isReleased(), is(false));
        Page page = new Page(block);

        Releasables.closeExpectNoException(block);
        assertThat(block.isReleased(), is(true));

        Exception e = expectThrows(IllegalStateException.class, () -> block.close());
        assertThat(e.getMessage(), containsString("can't release already released block"));

        e = expectThrows(IllegalStateException.class, () -> page.getBlock(0));
        assertThat(e.getMessage(), containsString("can't read released block"));

        e = expectThrows(IllegalArgumentException.class, () -> new Page(block));
        assertThat(e.getMessage(), containsString("can't build page out of released blocks"));
    }

    public void testRamBytesUsedWithout() {
        DocVector docs = new DocVector(
            IntBlock.newConstantBlockWith(0, 1).asVector(),
            IntBlock.newConstantBlockWith(0, 1).asVector(),
            IntBlock.newConstantBlockWith(0, 1).asVector(),
            false
        );
        assertThat(docs.singleSegmentNonDecreasing(), is(false));
        docs.ramBytesUsed(); // ensure non-singleSegmentNonDecreasing handles nulls in ramByteUsed
    }

    IntVector intRange(int startInclusive, int endExclusive) {
        return IntVector.range(startInclusive, endExclusive, BlockFactory.getNonBreakingInstance());
    }
}
