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
import org.elasticsearch.compute.data.LongVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.EvalOperator;
import org.elasticsearch.xpack.esql.expression.function.Warnings;
import org.elasticsearch.xpack.ql.tree.Source;

/**
 * {@link EvalOperator.ExpressionEvaluator} implementation for {@link DateExtract}.
 * This class is generated. Do not edit it.
 */
public final class DateExtractEvaluator implements EvalOperator.ExpressionEvaluator {
  private final Warnings warnings;

  private final EvalOperator.ExpressionEvaluator value;

  private final EvalOperator.ExpressionEvaluator chronoField;

  private final ZoneId zone;

  public DateExtractEvaluator(Source source, EvalOperator.ExpressionEvaluator value,
      EvalOperator.ExpressionEvaluator chronoField, ZoneId zone) {
    this.warnings = new Warnings(source);
    this.value = value;
    this.chronoField = chronoField;
    this.zone = zone;
  }

  @Override
  public Block eval(Page page) {
    Block valueUncastBlock = value.eval(page);
    if (valueUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    LongBlock valueBlock = (LongBlock) valueUncastBlock;
    Block chronoFieldUncastBlock = chronoField.eval(page);
    if (chronoFieldUncastBlock.areAllValuesNull()) {
      return Block.constantNullBlock(page.getPositionCount());
    }
    BytesRefBlock chronoFieldBlock = (BytesRefBlock) chronoFieldUncastBlock;
    LongVector valueVector = valueBlock.asVector();
    if (valueVector == null) {
      return eval(page.getPositionCount(), valueBlock, chronoFieldBlock);
    }
    BytesRefVector chronoFieldVector = chronoFieldBlock.asVector();
    if (chronoFieldVector == null) {
      return eval(page.getPositionCount(), valueBlock, chronoFieldBlock);
    }
    return eval(page.getPositionCount(), valueVector, chronoFieldVector);
  }

  public LongBlock eval(int positionCount, LongBlock valueBlock, BytesRefBlock chronoFieldBlock) {
    LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount);
    BytesRef chronoFieldScratch = new BytesRef();
    position: for (int p = 0; p < positionCount; p++) {
      if (valueBlock.isNull(p) || valueBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      if (chronoFieldBlock.isNull(p) || chronoFieldBlock.getValueCount(p) != 1) {
        result.appendNull();
        continue position;
      }
      try {
        result.appendLong(DateExtract.process(valueBlock.getLong(valueBlock.getFirstValueIndex(p)), chronoFieldBlock.getBytesRef(chronoFieldBlock.getFirstValueIndex(p), chronoFieldScratch), zone));
      } catch (IllegalArgumentException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  public LongBlock eval(int positionCount, LongVector valueVector,
      BytesRefVector chronoFieldVector) {
    LongBlock.Builder result = LongBlock.newBlockBuilder(positionCount);
    BytesRef chronoFieldScratch = new BytesRef();
    position: for (int p = 0; p < positionCount; p++) {
      try {
        result.appendLong(DateExtract.process(valueVector.getLong(p), chronoFieldVector.getBytesRef(p, chronoFieldScratch), zone));
      } catch (IllegalArgumentException e) {
        warnings.registerException(e);
        result.appendNull();
      }
    }
    return result.build();
  }

  @Override
  public String toString() {
    return "DateExtractEvaluator[" + "value=" + value + ", chronoField=" + chronoField + ", zone=" + zone + "]";
  }
}
