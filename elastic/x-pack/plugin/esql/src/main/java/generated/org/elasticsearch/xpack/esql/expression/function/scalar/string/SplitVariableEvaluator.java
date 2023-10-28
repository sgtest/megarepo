// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.string;

import java.lang.Override;
import java.lang.String;
import java.util.function.Function;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Split}.
 * This class is generated. Do not edit it.
 */
public final class SplitVariableEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator str;

  private final EvalOperator.ExpressionEvaluator delim;

  private final BytesRef scratch;

  private final DriverContext driverContext;

  public SplitVariableEvaluator(EvalOperator.ExpressionEvaluator str,
      EvalOperator.ExpressionEvaluator delim, BytesRef scratch, DriverContext driverContext) {
    this.str = str;
    this.delim = delim;
    this.scratch = scratch;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref strRef = str.eval(page)) {
      BytesRefBlock strBlock = (BytesRefBlock) strRef.block();
      try (Block.Ref delimRef = delim.eval(page)) {
        BytesRefBlock delimBlock = (BytesRefBlock) delimRef.block();
        BytesRefVector strVector = strBlock.asVector();
        if (strVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), strBlock, delimBlock));
        }
        BytesRefVector delimVector = delimBlock.asVector();
        if (delimVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), strBlock, delimBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), strVector, delimVector));
      }
    }
  }

  public BytesRefBlock eval(int positionCount, BytesRefBlock strBlock, BytesRefBlock delimBlock) {
    try(BytesRefBlock.Builder result = BytesRefBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef strScratch = new BytesRef();
      BytesRef delimScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        if (strBlock.isNull(p) || strBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        if (delimBlock.isNull(p) || delimBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        Split.process(result, strBlock.getBytesRef(strBlock.getFirstValueIndex(p), strScratch), delimBlock.getBytesRef(delimBlock.getFirstValueIndex(p), delimScratch), scratch);
      }
      return result.build();
    }
  }

  public BytesRefBlock eval(int positionCount, BytesRefVector strVector,
      BytesRefVector delimVector) {
    try(BytesRefBlock.Builder result = BytesRefBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef strScratch = new BytesRef();
      BytesRef delimScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        Split.process(result, strVector.getBytesRef(p, strScratch), delimVector.getBytesRef(p, delimScratch), scratch);
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "SplitVariableEvaluator[" + "str=" + str + ", delim=" + delim + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(str, delim);
  }

  static class Factory implements EvalOperator.ExpressionEvaluator.Factory {
    private final EvalOperator.ExpressionEvaluator.Factory str;

    private final EvalOperator.ExpressionEvaluator.Factory delim;

    private final Function<DriverContext, BytesRef> scratch;

    public Factory(EvalOperator.ExpressionEvaluator.Factory str,
        EvalOperator.ExpressionEvaluator.Factory delim, Function<DriverContext, BytesRef> scratch) {
      this.str = str;
      this.delim = delim;
      this.scratch = scratch;
    }

    @Override
    public SplitVariableEvaluator get(DriverContext context) {
      return new SplitVariableEvaluator(str.get(context), delim.get(context), scratch.apply(context), context);
    }

    @Override
    public String toString() {
      return "SplitVariableEvaluator[" + "str=" + str + ", delim=" + delim + "]";
    }
  }
}
