// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.conditional;

import java.lang.Override;
import java.lang.String;
import java.util.Arrays;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.LongVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Least}.
 * This class is generated. Do not edit it.
 */
public final class LeastLongEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator[] values;

  public LeastLongEvaluator(EvalOperator.ExpressionEvaluator[] values) {
    this.values = values;
  }

  @Override
  public Block eval(Page page) {
    LongBlock[] valuesBlocks = new LongBlock[values.length];
    for (int i = 0; i < valuesBlocks.length; i++) {
      Block block = values[i].eval(page);
      if (block.areAllValuesNull()) {
        return Block.constantNullBlock(page.getPositionCount());
      }
      valuesBlocks[i] = (LongBlock) block;
    }
    LongVector[] valuesVectors = new LongVector[values.length];
    for (int i = 0; i < valuesBlocks.length; i++) {
      valuesVectors[i] = valuesBlocks[i].asVector();
      if (valuesVectors[i] == null) {
        return eval(page.getPositionCount(), valuesBlocks);
      }
    }
    return eval(page.getPositionCount(), valuesVectors).asBlock();
  }

  public LongBlock eval(int positionCount, LongBlock[] valuesBlocks) {
    LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount);
    long[] valuesValues = new long[values.length];
    position: for (int p = 0; p < positionCount; p++) {
      for (int i = 0; i < valuesBlocks.length; i++) {
        if (valuesBlocks[i].isNull(p) || valuesBlocks[i].getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
      }
      // unpack valuesBlocks into valuesValues
      for (int i = 0; i < valuesBlocks.length; i++) {
        int o = valuesBlocks[i].getFirstValueIndex(p);
        valuesValues[i] = valuesBlocks[i].getLong(o);
      }
      result.appendLong(Least.process(valuesValues));
    }
    return result.build();
  }

  public LongVector eval(int positionCount, LongVector[] valuesVectors) {
    LongVector.Builder result = LongVector.newVectorBuilder(positionCount);
    long[] valuesValues = new long[values.length];
    position: for (int p = 0; p < positionCount; p++) {
      // unpack valuesVectors into valuesValues
      for (int i = 0; i < valuesVectors.length; i++) {
        valuesValues[i] = valuesVectors[i].getLong(p);
      }
      result.appendLong(Least.process(valuesValues));
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "LeastLongEvaluator[" + "values=" + Arrays.toString(values) + "]";
  }
}
