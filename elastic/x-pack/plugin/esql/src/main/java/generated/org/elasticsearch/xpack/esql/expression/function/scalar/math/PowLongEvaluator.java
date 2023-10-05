// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.math;

import java.lang.ArithmeticException;
import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.DoubleBlock;
import org.elasticsearch.compute.data.DoubleVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Pow}.
 * This class is generated. Do not edit it.
 */
public final class PowLongEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator base;

  private final EvalOperator.ExpressionEvaluator exponent;

  private final DriverContext driverContext;

  public PowLongEvaluator(Source source, EvalOperator.ExpressionEvaluator base,
      EvalOperator.ExpressionEvaluator exponent, DriverContext driverContext) {
    this.warnings = new Warnings(source);
    this.base = base;
    this.exponent = exponent;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref baseRef = base.eval(page)) {
      if (baseRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      DoubleBlock baseBlock = (DoubleBlock) baseRef.block();
      try (Block.Ref exponentRef = exponent.eval(page)) {
        if (exponentRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        DoubleBlock exponentBlock = (DoubleBlock) exponentRef.block();
        DoubleVector baseVector = baseBlock.asVector();
        if (baseVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), baseBlock, exponentBlock));
        }
        DoubleVector exponentVector = exponentBlock.asVector();
        if (exponentVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), baseBlock, exponentBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), baseVector, exponentVector));
      }
    }
  }

  public LongBlock eval(int positionCount, DoubleBlock baseBlock, DoubleBlock exponentBlock) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        if (baseBlock.isNull(p) || baseBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        if (exponentBlock.isNull(p) || exponentBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        try {
          result.appendLong(Pow.processLong(baseBlock.getDouble(baseBlock.getFirstValueIndex(p)), exponentBlock.getDouble(exponentBlock.getFirstValueIndex(p))));
        } catch (ArithmeticException e) {
          warnings.registerException(e);
          result.appendNull();
        }
      }
      return result.build();
    }
  }

  public LongBlock eval(int positionCount, DoubleVector baseVector, DoubleVector exponentVector) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        try {
          result.appendLong(Pow.processLong(baseVector.getDouble(p), exponentVector.getDouble(p)));
        } catch (ArithmeticException e) {
          warnings.registerException(e);
          result.appendNull();
        }
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "PowLongEvaluator[" + "base=" + base + ", exponent=" + exponent + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(base, exponent);
  }
}
