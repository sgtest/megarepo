// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.multivalue;

import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.LongArrayVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.Vector;
import org.elasticsearch.compute.operator.EvalOperator;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link MvMax}.
 * This class is generated. Do not edit it.
 */
public final class MvMaxLongEvaluator extends AbstractMultivalueFunction.AbstractEvaluator {
  public MvMaxLongEvaluator(EvalOperator.ExpressionEvaluator field) {
    super(field);
  }

  @Override
  public String name() {
    return "MvMax";
  }

  /**
   * Evaluate blocks containing at least one multivalued field.
   */
  @Override
  public Block evalNullable(Block fieldVal) {
    if (fieldVal.mvOrdering() == Block.MvOrdering.ASCENDING) {
      return evalAscendingNullable(fieldVal);
    }
    LongBlock v = (LongBlock) fieldVal;
    int positionCount = v.getPositionCount();
    LongBlock.Builder builder = LongBlock.newBlockBuilder(positionCount);
    for (int p = 0; p < positionCount; p++) {
      int valueCount = v.getValueCount(p);
      if (valueCount == 0) {
        builder.appendNull();
        continue;
      }
      int first = v.getFirstValueIndex(p);
      int end = first + valueCount;
      long value = v.getLong(first);
      for (int i = first + 1; i < end; i++) {
        long next = v.getLong(i);
        value = MvMax.process(value, next);
      }
      long result = value;
      builder.appendLong(result);
    }
    return builder.build();
  }

  /**
   * Evaluate blocks containing at least one multivalued field.
   */
  @Override
  public Vector evalNotNullable(Block fieldVal) {
    if (fieldVal.mvOrdering() == Block.MvOrdering.ASCENDING) {
      return evalAscendingNotNullable(fieldVal);
    }
    LongBlock v = (LongBlock) fieldVal;
    int positionCount = v.getPositionCount();
    long[] values = new long[positionCount];
    for (int p = 0; p < positionCount; p++) {
      int valueCount = v.getValueCount(p);
      int first = v.getFirstValueIndex(p);
      int end = first + valueCount;
      long value = v.getLong(first);
      for (int i = first + 1; i < end; i++) {
        long next = v.getLong(i);
        value = MvMax.process(value, next);
      }
      long result = value;
      values[p] = result;
    }
    return new LongArrayVector(values, positionCount);
  }

  /**
   * Evaluate blocks containing at least one multivalued field and all multivalued fields are in ascending order.
   */
  private Block evalAscendingNullable(Block fieldVal) {
    LongBlock v = (LongBlock) fieldVal;
    int positionCount = v.getPositionCount();
    LongBlock.Builder builder = LongBlock.newBlockBuilder(positionCount);
    for (int p = 0; p < positionCount; p++) {
      int valueCount = v.getValueCount(p);
      if (valueCount == 0) {
        builder.appendNull();
        continue;
      }
      int first = v.getFirstValueIndex(p);
      int idx = MvMax.ascendingIndex(valueCount);
      long result = v.getLong(first + idx);
      builder.appendLong(result);
    }
    return builder.build();
  }

  /**
   * Evaluate blocks containing at least one multivalued field and all multivalued fields are in ascending order.
   */
  private Vector evalAscendingNotNullable(Block fieldVal) {
    LongBlock v = (LongBlock) fieldVal;
    int positionCount = v.getPositionCount();
    long[] values = new long[positionCount];
    for (int p = 0; p < positionCount; p++) {
      int valueCount = v.getValueCount(p);
      int first = v.getFirstValueIndex(p);
      int idx = MvMax.ascendingIndex(valueCount);
      long result = v.getLong(first + idx);
      values[p] = result;
    }
    return new LongArrayVector(values, positionCount);
  }
}
