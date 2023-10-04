// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.math;

import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.LongVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Cast}.
 * This class is generated. Do not edit it.
 */
public final class CastIntToLongEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator v;

  private final DriverContext driverContext;

  public CastIntToLongEvaluator(EvalOperator.ExpressionEvaluator v, DriverContext driverContext) {
    this.v = v;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref vRef = v.eval(page)) {
      if (vRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      IntBlock vBlock = (IntBlock) vRef.block();
      IntVector vVector = vBlock.asVector();
      if (vVector == null) {
        return Block.Ref.floating(eval(page.getPositionCount(), vBlock));
      }
      return Block.Ref.floating(eval(page.getPositionCount(), vVector).asBlock());
    }
  }

  public LongBlock eval(int positionCount, IntBlock vBlock) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        if (vBlock.isNull(p) || vBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        result.appendLong(Cast.castIntToLong(vBlock.getInt(vBlock.getFirstValueIndex(p))));
      }
      return result.build();
    }
  }

  public LongVector eval(int positionCount, IntVector vVector) {
    try(LongVector.Builder result = LongVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        result.appendLong(Cast.castIntToLong(vVector.getInt(p)));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "CastIntToLongEvaluator[" + "v=" + v + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(v);
  }
}
