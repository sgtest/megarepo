// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.xpack.esql.expression.function.scalar.date;

import java.lang.Override;
import java.lang.String;
import java.util.Locale;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.LongBlock;
import org.elasticsearch.compute.data.LongVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.core.Releasables;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link DateFormat}.
 * This class is generated. Do not edit it.
 */
public final class DateFormatEvaluator implements EvalOperator.ExpressionEvaluator {
  private final EvalOperator.ExpressionEvaluator val;

  private final EvalOperator.ExpressionEvaluator formatter;

  private final Locale locale;

  private final DriverContext driverContext;

  public DateFormatEvaluator(EvalOperator.ExpressionEvaluator val,
      EvalOperator.ExpressionEvaluator formatter, Locale locale, DriverContext driverContext) {
    this.val = val;
    this.formatter = formatter;
    this.locale = locale;
    this.driverContext = driverContext;
  }

  @Override
  public Block.Ref eval(Page page) {
    try (Block.Ref valRef = val.eval(page)) {
      if (valRef.block().areAllValuesNull()) {
        return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
      }
      LongBlock valBlock = (LongBlock) valRef.block();
      try (Block.Ref formatterRef = formatter.eval(page)) {
        if (formatterRef.block().areAllValuesNull()) {
          return Block.Ref.floating(Block.constantNullBlock(page.getPositionCount(), driverContext.blockFactory()));
        }
        BytesRefBlock formatterBlock = (BytesRefBlock) formatterRef.block();
        LongVector valVector = valBlock.asVector();
        if (valVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), valBlock, formatterBlock));
        }
        BytesRefVector formatterVector = formatterBlock.asVector();
        if (formatterVector == null) {
          return Block.Ref.floating(eval(page.getPositionCount(), valBlock, formatterBlock));
        }
        return Block.Ref.floating(eval(page.getPositionCount(), valVector, formatterVector).asBlock());
      }
    }
  }

  public BytesRefBlock eval(int positionCount, LongBlock valBlock, BytesRefBlock formatterBlock) {
    try(BytesRefBlock.Builder result = BytesRefBlock.newBlockBuilder(positionCount, driverContext.blockFactory())) {
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
        result.appendBytesRef(DateFormat.process(valBlock.getLong(valBlock.getFirstValueIndex(p)), formatterBlock.getBytesRef(formatterBlock.getFirstValueIndex(p), formatterScratch), locale));
      }
      return result.build();
    }
  }

  public BytesRefVector eval(int positionCount, LongVector valVector,
      BytesRefVector formatterVector) {
    try(BytesRefVector.Builder result = BytesRefVector.newVectorBuilder(positionCount, driverContext.blockFactory())) {
      BytesRef formatterScratch = new BytesRef();
      position: for (int p = 0; p < positionCount; p++) {
        result.appendBytesRef(DateFormat.process(valVector.getLong(p), formatterVector.getBytesRef(p, formatterScratch), locale));
      }
      return result.build();
    }
  }

  @Override
  public String toString() {
    return "DateFormatEvaluator[" + "val=" + val + ", formatter=" + formatter + ", locale=" + locale + "]";
  }

  @Override
  public void close() {
    Releasables.closeExpectNoException(val, formatter);
  }
}
