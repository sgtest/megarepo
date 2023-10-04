// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.string;

import java.lang.Override;
import java.lang.String;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BooleanBlock;
import org.elasticsearch.compute.data.BooleanVector;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link StartsWith}.
 * This class is generated. Do not edit it.
 */
public final class StartsWithEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator str;

  private final EvalOperator.ExpressionEvaluator prefix;

  private final DriverContext driverContext;

  public StartsWithEvaluator(EvalOperator.ExpressionEvaluator str,
      EvalOperator.ExpressionEvaluator prefix, DriverContext driverContext) {
    this.str = str;
    this.prefix = prefix;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref strRef = str.eval(page)) {
      if (strRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      BytesRefBlock strBlock = (BytesRefBlock) strRef.block();
      try (Block.Ref prefixRef = prefix.eval(page)) {
        if (prefixRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        BytesRefBlock prefixBlock = (BytesRefBlock) prefixRef.block();
        BytesRefVector strVector = strBlock.asVector();
        if (strVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), strBlock, prefixBlock));
        }
        BytesRefVector prefixVector = prefixBlock.asVector();
        if (prefixVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), strBlock, prefixBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), strVector, prefixVector).asBlock());
      }
    }
  }

  public BooleanBlock eval(int positionCount, BytesRefBlock strBlock, BytesRefBlock prefixBlock) {
    try(BooleanBlock.Builder result = BooleanBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef strScratch = new BytesRef();
      BytesRef prefixScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        if (strBlock.isNull(p) || strBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        if (prefixBlock.isNull(p) || prefixBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        result.appendBoolean(StartsWith.process(strBlock.getBytesRef(strBlock.getFirstValueIndex(p), strScratch), prefixBlock.getBytesRef(prefixBlock.getFirstValueIndex(p), prefixScratch)));
      }
      return result.build();
    }
  }

  public BooleanVector eval(int positionCount, BytesRefVector strVector,
      BytesRefVector prefixVector) {
    try(BooleanVector.Builder result = BooleanVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef strScratch = new BytesRef();
      BytesRef prefixScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        result.appendBoolean(StartsWith.process(strVector.getBytesRef(p, strScratch), prefixVector.getBytesRef(p, prefixScratch)));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "StartsWithEvaluator[" + "str=" + str + ", prefix=" + prefix + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(str, prefix);
  }
}
