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
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Mod}.
 * This class is generated. Do not edit it.
 */
public final class ModUnsignedLongsEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator lhs;

  private final EvalOperator.ExpressionEvaluator rhs;

  private final DriverContext driverContext;

  public ModUnsignedLongsEvaluator(Source source, EvalOperator.ExpressionEvaluator lhs,
      EvalOperator.ExpressionEvaluator rhs, DriverContext driverContext) {
    this.warnings = new Warnings(source);
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
      LongBlock lhsBlock = (LongBlock) lhsRef.block();
      try (Block.Ref rhsRef = rhs.eval(page)) {
        if (rhsRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        LongBlock rhsBlock = (LongBlock) rhsRef.block();
        LongVector lhsVector = lhsBlock.asVector();
        if (lhsVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), lhsBlock, rhsBlock));
        }
        LongVector rhsVector = rhsBlock.asVector();
        if (rhsVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), lhsBlock, rhsBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), lhsVector, rhsVector));
      }
    }
  }

  public LongBlock eval(int positionCount, LongBlock lhsBlock, LongBlock rhsBlock) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
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
          result.appendLong(Mod.processUnsignedLongs(lhsBlock.getLong(lhsBlock.getFirstValueIndex(p)), rhsBlock.getLong(rhsBlock.getFirstValueIndex(p))));
        } catch (ArithmeticException e) {
          warnings.registerException(e);
          result.appendNull();
        }
      }
      return result.build();
    }
  }

  public LongBlock eval(int positionCount, LongVector lhsVector, LongVector rhsVector) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      position: for (int p = 0; p < positionCount; p++) {
        try {
          result.appendLong(Mod.processUnsignedLongs(lhsVector.getLong(p), rhsVector.getLong(p)));
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
    return "ModUnsignedLongsEvaluator[" + "lhs=" + lhs + ", rhs=" + rhs + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(lhs, rhs);
  }
}
