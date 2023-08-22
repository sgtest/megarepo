// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.conditional;

import java.lang.Override;
import java.lang.String;
import java.util.Arrays;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.DoubleBlock;
import org.elasticsearch.compute.data.DoubleVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Greatest}.
 * This class is generated. Do not edit it.
 */
public final class GreatestDoubleEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator[] values;

  public GreatestDoubleEvaluator(EvalOperator.ExpressionEvaluator[] values) {
    this.values = values;
  }

  @Override
  public Block eval(Page page) {
    DoubleBlock[] valuesBlocks = new DoubleBlock[values.length];
    for (int i = 0; i < valuesBlocks.length; i++) {
      Block block = values[i].eval(page);
      if (block.areAllValuesNull()) {
        return Block.constantNullBlock(page.getPositionCount());
      }
      valuesBlocks[i] = (DoubleBlock) block;
    }
    DoubleVector[] valuesVectors = new DoubleVector[values.length];
    for (int i = 0; i < valuesBlocks.length; i++) {
      valuesVectors[i] = valuesBlocks[i].asVector();
      if (valuesVectors[i] == null) {
        return eval(page.getPositionCount(), valuesBlocks);
      }
    }
    return eval(page.getPositionCount(), valuesVectors).asBlock();
  }

  public DoubleBlock eval(int positionCount, DoubleBlock[] valuesBlocks) {
    DoubleBlock.Builder result = DoubleBlock.newBlockBuilder(positionCount);
    double[] valuesValues = new double[values.length];
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
        valuesValues[i] = valuesBlocks[i].getDouble(o);
      }
      result.appendDouble(Greatest.process(valuesValues));
    }
    return result.build();
  }

  public DoubleVector eval(int positionCount, DoubleVector[] valuesVectors) {
    DoubleVector.Builder result = DoubleVector.newVectorBuilder(positionCount);
    double[] valuesValues = new double[values.length];
    position: for (int p = 0; p < positionCount; p++) {
      // unpack valuesVectors into valuesValues
      for (int i = 0; i < valuesVectors.length; i++) {
        valuesValues[i] = valuesVectors[i].getDouble(p);
      }
      result.appendDouble(Greatest.process(valuesValues));
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "GreatestDoubleEvaluator[" + "values=" + Arrays.toString(values) + "]";
  }
}
