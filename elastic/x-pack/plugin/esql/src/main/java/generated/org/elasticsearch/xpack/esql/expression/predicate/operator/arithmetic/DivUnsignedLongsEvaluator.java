// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.predicate.operator.arithmetic;

import java.lang.ArithmeticException;
import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.LongVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Div}.
 * This class is generated. Do not edit it.
 */
public final class DivUnsignedLongsEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator lhs;

  private final EvalOperator.ExpressionEvaluator rhs;

  public DivUnsignedLongsEvaluator(Source source, EvalOperator.ExpressionEvaluator lhs,
      EvalOperator.ExpressionEvaluator rhs) {
    this.warnings = new Warnings(source);
    this.lhs = lhs;
    this.rhs = rhs;
  }

  @Override
  public Block eval(Page page) {
    Block lhsUncastBlock = lhs.eval(page);
    if (lhsUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    LongBlock lhsBlock = (LongBlock) lhsUncastBlock;
    Block rhsUncastBlock = rhs.eval(page);
    if (rhsUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    LongBlock rhsBlock = (LongBlock) rhsUncastBlock;
    LongVector lhsVector = lhsBlock.asVector();
    if (lhsVector == null) {
      return eval(page.getPositionCount(), lhsBlock, rhsBlock);
    }
    LongVector rhsVector = rhsBlock.asVector();
    if (rhsVector == null) {
      return eval(page.getPositionCount(), lhsBlock, rhsBlock);
    }
    return eval(page.getPositionCount(), lhsVector, rhsVector);
  }

  public LongBlock eval(int positionCount, LongBlock lhsBlock, LongBlock rhsBlock) {
    LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      if (lhsBlock.isNull(p) || lhsBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      if (rhsBlock.isNull(p) || rhsBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      try {
        result.appendLong(Div.processUnsignedLongs(lhsBlock.getLong(lhsBlock.getFirstValueIndex(p)), rhsBlock.getLong(rhsBlock.getFirstValueIndex(p))));
      } catch (ArithmeticException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  public LongBlock eval(int positionCount, LongVector lhsVector, LongVector rhsVector) {
    LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      try {
        result.appendLong(Div.processUnsignedLongs(lhsVector.getLong(p), rhsVector.getLong(p)));
      } catch (ArithmeticException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "DivUnsignedLongsEvaluator[" + "lhs=" + lhs + ", rhs=" + rhs + "]";
  }
}
