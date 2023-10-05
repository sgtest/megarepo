// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.multivalue;

import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link MvMax}.
 * This class is generated. Do not edit it.
 */
public final class MvMaxIntEvaluator extends AbstractMultivalueFunction.AbstractEvaluator {
  private final DriverContext driverContext;

  public MvMaxIntEvaluator(EvalOperator.ExpressionEvaluator field, DriverContext driverContext) {
    super(field);
    this.driverContext = driverContext;
  }

  @Override
  public String name() {
    return "MvMax";
  }

  /**
   * Evaluate blocks containing at least one multivalued field.
   */
  @Override
  public Block.Ref evalNullable(Block.Ref ref) {
    if (ref.block().mvSortedAscending()) {
      return evalAscendingNullable(ref);
    }
    try (ref) {
      IntBlock v = (IntBlock) ref.block();
      int positionCount = v.getPositionCount();
      try (IntBlock.Builder builder = IntBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
        for (int p = 0; p < positionCount; p++) {
          int valueCount = v.getValueCount(p);
          if (valueCount == 0) {
            builder.appendNull();
            continue;
          }
          int first = v.getFirstValueIndex(p);
          int end = first + valueCount;
          int value = v.getInt(first);
          for (int i = first + 1; i < end; i++) {
            int next = v.getInt(i);
            value = MvMax.process(value, next);
          }
          int result = value;
          builder.appendInt(result);
        }
        return Block.Ref.floating(builder.build());
      }
    }
  }

  /**
   * Evaluate blocks containing at least one multivalued field.
   */
  @Override
  public Block.Ref evalNotNullable(Block.Ref ref) {
    if (ref.block().mvSortedAscending()) {
      return evalAscendingNotNullable(ref);
    }
    try (ref) {
      IntBlock v = (IntBlock) ref.block();
      int positionCount = v.getPositionCount();
      try (IntVector.FixedBuilder builder = IntVector.newVectorFixedBuilder(positionCount, driverContext.blockFactory())) {
        for (int p = 0; p < positionCount; p++) {
          int valueCount = v.getValueCount(p);
          int first = v.getFirstValueIndex(p);
          int end = first + valueCount;
          int value = v.getInt(first);
          for (int i = first + 1; i < end; i++) {
            int next = v.getInt(i);
            value = MvMax.process(value, next);
          }
          int result = value;
          builder.appendInt(result);
        }
        return Block.Ref.floating(builder.build().asBlock());
      }
    }
  }

  /**
   * Evaluate blocks containing at least one multivalued field and all multivalued fields are in ascending order.
   */
  private Block.Ref evalAscendingNullable(Block.Ref ref) {
    try (ref) {
      IntBlock v = (IntBlock) ref.block();
      int positionCount = v.getPositionCount();
      try (IntBlock.Builder builder = IntBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
        for (int p = 0; p < positionCount; p++) {
          int valueCount = v.getValueCount(p);
          if (valueCount == 0) {
            builder.appendNull();
            continue;
          }
          int first = v.getFirstValueIndex(p);
          int idx = MvMax.ascendingIndex(valueCount);
          int result = v.getInt(first + idx);
          builder.appendInt(result);
        }
        return Block.Ref.floating(builder.build());
      }
    }
  }

  /**
   * Evaluate blocks containing at least one multivalued field and all multivalued fields are in ascending order.
   */
  private Block.Ref evalAscendingNotNullable(Block.Ref ref) {
    try (ref) {
      IntBlock v = (IntBlock) ref.block();
      int positionCount = v.getPositionCount();
      try (IntVector.FixedBuilder builder = IntVector.newVectorFixedBuilder(positionCount, driverContext.blockFactory())) {
        for (int p = 0; p < positionCount; p++) {
          int valueCount = v.getValueCount(p);
          int first = v.getFirstValueIndex(p);
          int idx = MvMax.ascendingIndex(valueCount);
          int result = v.getInt(first + idx);
          builder.appendInt(result);
        }
        return Block.Ref.floating(builder.build().asBlock());
      }
    }
  }
}
