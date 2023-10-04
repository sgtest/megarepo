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
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Abs}.
 * This class is generated. Do not edit it.
 */
public final class AbsDoubleEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator fieldVal;

  private final DriverContext driverContext;

  public AbsDoubleEvaluator(EvalOperator.ExpressionEvaluator fieldVal,
      DriverContext driverContext) {
    this.fieldVal = fieldVal;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref fieldValRef = fieldVal.eval(page)) {
      if (fieldValRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      DoubleBlock fieldValBlock = (DoubleBlock) fieldValRef.block();
      DoubleVector fieldValVector = fieldValBlock.asVector();
      if (fieldValVector == null) {
        return Block.Ref.floating(eval(page.getPositionCount(), fieldValBlock));
      }
      return Block.Ref.floating(eval(page.getPositionCount(), fieldValVector).asBlock());
    }
  }

  public DoubleBlock eval(int positionCount, DoubleBlock fieldValBlock) {
    try(DoubleBlock.Builder result = DoubleBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        if (fieldValBlock.isNull(p) || fieldValBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        result.appendDouble(Abs.process(fieldValBlock.getDouble(fieldValBlock.getFirstValueIndex(p))));
      }
      return result.build();
    }
  }

  public DoubleVector eval(int positionCount, DoubleVector fieldValVector) {
    try(DoubleVector.Builder result = DoubleVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        result.appendDouble(Abs.process(fieldValVector.getDouble(p)));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "AbsDoubleEvaluator[" + "fieldVal=" + fieldVal + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(fieldVal);
  }
}
