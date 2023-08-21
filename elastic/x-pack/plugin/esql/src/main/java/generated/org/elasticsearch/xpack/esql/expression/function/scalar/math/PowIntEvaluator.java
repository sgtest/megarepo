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
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Pow}.
 * This class is generated. Do not edit it.
 */
public final class PowIntEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator base;

  private final EvalOperator.ExpressionEvaluator exponent;

  public PowIntEvaluator(Source source, EvalOperator.ExpressionEvaluator base,
      EvalOperator.ExpressionEvaluator exponent) {
    this.warnings = new Warnings(source);
    this.base = base;
    this.exponent = exponent;
  }

  @Override
  public Block eval(Page page) {
    Block baseUncastBlock = base.eval(page);
    if (baseUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    DoubleBlock baseBlock = (DoubleBlock) baseUncastBlock;
    Block exponentUncastBlock = exponent.eval(page);
    if (exponentUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    DoubleBlock exponentBlock = (DoubleBlock) exponentUncastBlock;
    DoubleVector baseVector = baseBlock.asVector();
    if (baseVector == null) {
      return eval(page.getPositionCount(), baseBlock, exponentBlock);
    }
    DoubleVector exponentVector = exponentBlock.asVector();
    if (exponentVector == null) {
      return eval(page.getPositionCount(), baseBlock, exponentBlock);
    }
    return eval(page.getPositionCount(), baseVector, exponentVector);
  }

  public IntBlock eval(int positionCount, DoubleBlock baseBlock, DoubleBlock exponentBlock) {
    IntBlock.Builder result = IntBlock.newBlockBuilder(positionCount);
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
        result.appendInt(Pow.processInt(baseBlock.getDouble(baseBlock.getFirstValueIndex(p)), exponentBlock.getDouble(exponentBlock.getFirstValueIndex(p))));
      } catch (ArithmeticException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  public IntBlock eval(int positionCount, DoubleVector baseVector, DoubleVector exponentVector) {
    IntBlock.Builder result = IntBlock.newBlockBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      try {
        result.appendInt(Pow.processInt(baseVector.getDouble(p), exponentVector.getDouble(p)));
      } catch (ArithmeticException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "PowIntEvaluator[" + "base=" + base + ", exponent=" + exponent + "]";
  }
}
