// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.string;

import java.lang.Override;
import java.lang.String;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Length}.
 * This class is generated. Do not edit it.
 */
public final class LengthEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator val;

  private final DriverContext driverContext;

  public LengthEvaluator(EvalOperator.ExpressionEvaluator val, DriverContext driverContext) {
    this.val = val;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref valRef = val.eval(page)) {
      if (valRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      BytesRefBlock valBlock = (BytesRefBlock) valRef.block();
      BytesRefVector valVector = valBlock.asVector();
      if (valVector == null) {
        return Block.Ref.floating(eval(page.getPositionCount(), valBlock));
      }
      return Block.Ref.floating(eval(page.getPositionCount(), valVector).asBlock());
    }
  }

  public IntBlock eval(int positionCount, BytesRefBlock valBlock) {
    try(IntBlock.Builder result = IntBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef valScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        if (valBlock.isNull(p) || valBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        result.appendInt(Length.process(valBlock.getBytesRef(valBlock.getFirstValueIndex(p), valScratch)));
      }
      return result.build();
    }
  }

  public IntVector eval(int positionCount, BytesRefVector valVector) {
    try(IntVector.Builder result = IntVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef valScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        result.appendInt(Length.process(valVector.getBytesRef(p, valScratch)));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "LengthEvaluator[" + "val=" + val + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(val);
  }
}
