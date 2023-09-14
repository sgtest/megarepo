/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.lucene;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;
import org.elasticsearch.test.ESTestCase;

import static org.hamcrest.Matchers.equalTo;

public class LuceneSourceOperatorStatusTests extends AbstractWireSerializingTestCase<LuceneSourceOperator.Status> {
    public static LuceneSourceOperator.Status simple() {
        return new LuceneSourceOperator.Status(0, 1, 5, 123, 99990);
    }

    public static String simpleToJson() {
        return """
            {"processed_sliced":0,"total_slices":1,"slice_position":123,"slice_size":99990,"pages_emitted":5}""";
    }

    public void testToXContent() {
        assertThat(Strings.toString(simple()), equalTo(simpleToJson()));
    }

    @Override
    protected Writeable.Reader<LuceneSourceOperator.Status> instanceReader() {
        return LuceneSourceOperator.Status::new;
    }

    @Override
    public LuceneSourceOperator.Status createTestInstance() {
        return new LuceneSourceOperator.Status(
            randomNonNegativeInt(),
            randomNonNegativeInt(),
            randomNonNegativeInt(),
            randomNonNegativeInt(),
            randomNonNegativeInt()
        );
    }

    @Override
    protected LuceneSourceOperator.Status mutateInstance(LuceneSourceOperator.Status instance) {
        return switch (between(0, 4)) {
            case 0 -> new LuceneSourceOperator.Status(
                randomValueOtherThan(instance.currentLeaf(), ESTestCase::randomNonNegativeInt),
                instance.totalLeaves(),
                instance.pagesEmitted(),
                instance.slicePosition(),
                instance.sliceSize()
            );
            case 1 -> new LuceneSourceOperator.Status(
                instance.currentLeaf(),
                randomValueOtherThan(instance.totalLeaves(), ESTestCase::randomNonNegativeInt),
                instance.pagesEmitted(),
                instance.slicePosition(),
                instance.sliceSize()
            );
            case 2 -> new LuceneSourceOperator.Status(
                instance.currentLeaf(),
                instance.totalLeaves(),
                randomValueOtherThan(instance.pagesEmitted(), ESTestCase::randomNonNegativeInt),
                instance.slicePosition(),
                instance.sliceSize()
            );
            case 3 -> new LuceneSourceOperator.Status(
                instance.currentLeaf(),
                instance.totalLeaves(),
                instance.pagesEmitted(),
                randomValueOtherThan(instance.slicePosition(), ESTestCase::randomNonNegativeInt),
                instance.sliceSize()
            );
            case 4 -> new LuceneSourceOperator.Status(
                instance.currentLeaf(),
                instance.totalLeaves(),
                instance.pagesEmitted(),
                instance.slicePosition(),
                randomValueOtherThan(instance.sliceSize(), ESTestCase::randomNonNegativeInt)
            );
            default -> throw new UnsupportedOperationException();
        };
    }
}
