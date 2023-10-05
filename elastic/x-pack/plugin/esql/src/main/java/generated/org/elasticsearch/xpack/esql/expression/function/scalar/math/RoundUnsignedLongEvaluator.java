// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.math;

import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.LongVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Round}.
 * This class is generated. Do not edit it.
 */
public final class RoundUnsignedLongEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator val;

  private final EvalOperator.ExpressionEvaluator decimals;

  private final DriverContext driverContext;

  public RoundUnsignedLongEvaluator(EvalOperator.ExpressionEvaluator val,
      EvalOperator.ExpressionEvaluator decimals, DriverContext driverContext) {
    this.val = val;
    this.decimals = decimals;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref valRef = val.eval(page)) {
      if (valRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      LongBlock valBlock = (LongBlock) valRef.block();
      try (Block.Ref decimalsRef = decimals.eval(page)) {
        if (decimalsRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        LongBlock decimalsBlock = (LongBlock) decimalsRef.block();
        LongVector valVector = valBlock.asVector();
        if (valVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), valBlock, decimalsBlock));
        }
        LongVector decimalsVector = decimalsBlock.asVector();
        if (decimalsVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), valBlock, decimalsBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), valVector, decimalsVector).asBlock());
      }
    }
  }

  public LongBlock eval(int positionCount, LongBlock valBlock, LongBlock decimalsBlock) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        if (valBlock.isNull(p) || valBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        if (decimalsBlock.isNull(p) || decimalsBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        result.appendLong(Round.processUnsignedLong(valBlock.getLong(valBlock.getFirstValueIndex(p)), decimalsBlock.getLong(decimalsBlock.getFirstValueIndex(p))));
      }
      return result.build();
    }
  }

  public LongVector eval(int positionCount, LongVector valVector, LongVector decimalsVector) {
    try(LongVector.Builder result = LongVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        result.appendLong(Round.processUnsignedLong(valVector.getLong(p), decimalsVector.getLong(p)));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "RoundUnsignedLongEvaluator[" + "val=" + val + ", decimals=" + decimals + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(val, decimals);
  }
}
