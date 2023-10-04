// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.convert;

import java.lang.Override;
import java.lang.String;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.Vector;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link ToLong}.
 * This class is generated. Do not edit it.
 */
public final class ToLongFromStringEvaluator extends AbstractConvertFunction.AbstractEvaluator {
  public ToLongFromStringEvaluator(EvalOperator.ExpressionEvaluator field, Source source,
      DriverContext driverContext) {
    super(driverContext, field, source);
  }

  @Override
  public String name() {
    return "ToLongFromString";
  }

  @Override
  public Block evalVector(Vector v) {
    BytesRefVector vector = (BytesRefVector) v;
    int positionCount = v.getPositionCount();
    BytesRef scratchPad = new BytesRef();
    if (vector.isConstant()) {
      try {
        return driverContext.blockFactory().newConstantLongBlockWith(evalValue(vector, 0, scratchPad), positionCount);
      } catch (Exception e) {
        registerException(e);
        return Block.constantNullBlock(positionCount, driverContext.blockFactory());
      }
    }
    LongBlock.Builder builder = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory());
    for (int p = 0; p < positionCount; p++) {
      try {
        builder.appendLong(evalValue(vector, p, scratchPad));
      } catch (Exception e) {
        registerException(e);
        builder.appendNull();
      }
    }
    return builder.build();
  }

  private static long evalValue(BytesRefVector container, int index, BytesRef scratchPad) {
    BytesRef value = container.getBytesRef(index, scratchPad);
    return ToLong.fromKeyword(value);
  }

  @Override
  public Block evalBlock(Block b) {
    BytesRefBlock block = (BytesRefBlock) b;
    int positionCount = block.getPositionCount();
    try (LongBlock.Builder builder = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef scratchPad = new BytesRef();
      for (int p = 0; p < positionCount; p++) {
        int valueCount = block.getValueCount(p);
        int start = block.getFirstValueIndex(p);
        int end = start + valueCount;
        boolean positionOpened = false;
        boolean valuesAppended = false;
        for (int i = start; i < end; i++) {
          try {
            long value = evalValue(block, i, scratchPad);
            if (positionOpened == false && valueCount > 1) {
              builder.beginPositionEntry();
              positionOpened = true;
            }
            builder.appendLong(value);
            valuesAppended = true;
          } catch (Exception e) {
            registerException(e);
          }
        }
        if (valuesAppended == false) {
          builder.appendNull();
        } else if (positionOpened) {
          builder.endPositionEntry();
        }
      }
      return builder.build();
    }
  }

  private static long evalValue(BytesRefBlock container, int index, BytesRef scratchPad) {
    BytesRef value = container.getBytesRef(index, scratchPad);
    return ToLong.fromKeyword(value);
  }
}
