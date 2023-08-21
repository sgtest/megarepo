// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.math;

import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.DoubleBlock;
import org.elasticsearch.compute.data.DoubleVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Atan2}.
 * This class is generated. Do not edit it.
 */
public final class Atan2Evaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator y;

  private final EvalOperator.ExpressionEvaluator x;

  public Atan2Evaluator(EvalOperator.ExpressionEvaluator y, EvalOperator.ExpressionEvaluator x) {
    this.y = y;
    this.x = x;
  }

  @Override
  public Block eval(Page page) {
    Block yUncastBlock = y.eval(page);
    if (yUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    DoubleBlock yBlock = (DoubleBlock) yUncastBlock;
    Block xUncastBlock = x.eval(page);
    if (xUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    DoubleBlock xBlock = (DoubleBlock) xUncastBlock;
    DoubleVector yVector = yBlock.asVector();
    if (yVector == null) {
      return eval(page.getPositionCount(), yBlock, xBlock);
    }
    DoubleVector xVector = xBlock.asVector();
    if (xVector == null) {
      return eval(page.getPositionCount(), yBlock, xBlock);
    }
    return eval(page.getPositionCount(), yVector, xVector).asBlock();
  }

  public DoubleBlock eval(int positionCount, DoubleBlock yBlock, DoubleBlock xBlock) {
    DoubleBlock.Builder result = DoubleBlock.newBlockBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      if (yBlock.isNull(p) || yBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      if (xBlock.isNull(p) || xBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      result.appendDouble(Atan2.process(yBlock.getDouble(yBlock.getFirstValueIndex(p)), xBlock.getDouble(xBlock.getFirstValueIndex(p))));
    }
    return result.build();
  }

  public DoubleVector eval(int positionCount, DoubleVector yVector, DoubleVector xVector) {
    DoubleVector.Builder result = DoubleVector.newVectorBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      result.appendDouble(Atan2.process(yVector.getDouble(p), xVector.getDouble(p)));
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "Atan2Evaluator[" + "y=" + y + ", x=" + x + "]";
  }
}
