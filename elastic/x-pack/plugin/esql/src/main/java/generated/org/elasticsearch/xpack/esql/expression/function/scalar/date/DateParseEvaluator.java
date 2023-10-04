// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.date;

import java.lang.IllegalArgumentException;
import java.lang.Override;
import java.lang.String;
import java.time.ZoneId;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link DateParse}.
 * This class is generated. Do not edit it.
 */
public final class DateParseEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator val;

  private final EvalOperator.ExpressionEvaluator formatter;

  private final ZoneId zoneId;

  private final DriverContext driverContext;

  public DateParseEvaluator(Source source, EvalOperator.ExpressionEvaluator val,
      EvalOperator.ExpressionEvaluator formatter, ZoneId zoneId, DriverContext driverContext) {
    this.warnings = new Warnings(source);
    this.val = val;
    this.formatter = formatter;
    this.zoneId = zoneId;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref valRef = val.eval(page)) {
      if (valRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      BytesRefBlock valBlock = (BytesRefBlock) valRef.block();
      try (Block.Ref formatterRef = formatter.eval(page)) {
        if (formatterRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        BytesRefBlock formatterBlock = (BytesRefBlock) formatterRef.block();
        BytesRefVector valVector = valBlock.asVector();
        if (valVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), valBlock, formatterBlock));
        }
        BytesRefVector formatterVector = formatterBlock.asVector();
        if (formatterVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), valBlock, formatterBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), valVector, formatterVector));
      }
    }
  }

  public LongBlock eval(int positionCount, BytesRefBlock valBlock, BytesRefBlock formatterBlock) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef valScratch = new BytesRef();
      BytesRef formatterScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        if (valBlock.isNull(p) || valBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        if (formatterBlock.isNull(p) || formatterBlock.getValueCount(p) != 1) {
          result.appendNull();
          continue position;
        }
        try {
          result.appendLong(DateParse.process(valBlock.getBytesRef(valBlock.getFirstValueIndex(p), valScratch), formatterBlock.getBytesRef(formatterBlock.getFirstValueIndex(p), formatterScratch), zoneId));
        } catch (IllegalArgumentException e) {
          warnings.registerException(e);
          result.appendNull();
        }
      }
      return result.build();
    }
  }

  public LongBlock eval(int positionCount, BytesRefVector valVector,
      BytesRefVector formatterVector) {
    try(LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef valScratch = new BytesRef();
      BytesRef formatterScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        try {
          result.appendLong(DateParse.process(valVector.getBytesRef(p, valScratch), formatterVector.getBytesRef(p, formatterScratch), zoneId));
        } catch (IllegalArgumentException e) {
          warnings.registerException(e);
          result.appendNull();
        }
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "DateParseEvaluator[" + "val=" + val + ", formatter=" + formatter + ", zoneId=" + zoneId + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(val, formatter);
  }
}
