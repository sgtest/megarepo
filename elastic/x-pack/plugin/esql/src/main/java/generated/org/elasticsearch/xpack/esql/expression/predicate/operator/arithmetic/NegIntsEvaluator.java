// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.predicate.operator.arithmetic;

import java.lang.ArithmeticException;
import java.lang.Override;
import java.lang.String;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link Neg}.
 * This class is generated. Do not edit it.
 */
public final class NegIntsEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator v;

  public NegIntsEvaluator(Source source, EvalOperator.ExpressionEvaluator v) {
    this.warnings = new Warnings(source);
    this.v = v;
  }

  @Override
  public Block eval(Page page) {
    Block vUncastBlock = v.eval(page);
    if (vUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    IntBlock vBlock = (IntBlock) vUncastBlock;
    IntVector vVector = vBlock.asVector();
    if (vVector == null) {
      return eval(page.getPositionCount(), vBlock);
    }
    return eval(page.getPositionCount(), vVector);
  }

  public IntBlock eval(int positionCount, IntBlock vBlock) {
    IntBlock.Builder result = IntBlock.newBlockBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      if (vBlock.isNull(p) || vBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      try {
        result.appendInt(Neg.processInts(vBlock.getInt(vBlock.getFirstValueIndex(p))));
      } catch (ArithmeticException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  public IntBlock eval(int positionCount, IntVector vVector) {
    IntBlock.Builder result = IntBlock.newBlockBuilder(positionCount);
    position: for (int p = 0; p < positionCount; p++) {
      try {
        result.appendInt(Neg.processInts(vVector.getInt(p)));
      } catch (ArithmeticException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "NegIntsEvaluator[" + "v=" + v + "]";
  }
}
