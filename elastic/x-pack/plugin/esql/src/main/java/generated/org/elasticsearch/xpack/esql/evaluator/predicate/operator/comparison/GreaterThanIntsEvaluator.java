// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.evaluator.predicate.operator.comparison;

import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BooleanBlock;
import org.elasticsearch.compute.data.BooleanVector;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link GreaterThan}.
 * This class is generated. Do not edit it.
 */
public final class GreaterThanIntsEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator lhs;

  private final EvalOperator.ExpressionEvaluator rhs;

  private final DriverContext driverContext;

  public GreaterThanIntsEvaluator(EvalOperator.ExpressionEvaluator lhs,
      EvalOperator.ExpressionEvaluator rhs, DriverContext driverContext) {
    this.lhs = lhs;
    this.rhs = rhs;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref lhsRef = lhs.eval(page)) {
      if (lhsRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      IntBlock lhsBlock = (IntBlock) lhsRef.block();
      try (Block.Ref rhsRef = rhs.eval(page)) {
        if (rhsRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        IntBlock rhsBlock = (IntBlock) rhsRef.block();
        IntVector lhsVector = lhsBlock.asVector();
        if (lhsVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), lhsBlock, rhsBlock));
        }
        IntVector rhsVector = rhsBlock.asVector();
        if (rhsVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), lhsBlock, rhsBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), lhsVector, rhsVector).asBlock());
      }
    }
  }

  public BooleanBlock eval(int positionCount, IntBlock lhsBlock, IntBlock rhsBlock) {
    try(BooleanBlock.Builder result = BooleanBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        if (lhsBlock.isNull(p) || lhsBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        if (rhsBlock.isNull(p) || rhsBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        result.appendBoolean(GreaterThan.processInts(lhsBlock.getInt(lhsBlock.getFirstValueIndex(p)), rhsBlock.getInt(rhsBlock.getFirstValueIndex(p))));
      }
      return result.build();
    }
  }

  public BooleanVector eval(int positionCount, IntVector lhsVector, IntVector rhsVector) {
    try(BooleanVector.Builder result = BooleanVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        result.appendBoolean(GreaterThan.processInts(lhsVector.getInt(p), rhsVector.getInt(p)));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "GreaterThanIntsEvaluator[" + "lhs=" + lhs + ", rhs=" + rhs + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(lhs, rhs);
  }
}
