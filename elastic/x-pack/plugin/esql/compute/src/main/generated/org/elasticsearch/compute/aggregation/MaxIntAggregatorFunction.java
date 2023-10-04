// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License
// 2.0; you may not use this file except in compliance with the Elastic License
// 2.0.
package org.elasticsearch.compute.aggregation;

import java.lang.Integer;
import java.lang.Override;
import java.lang.String;
import java.lang.StringBuilder;
import java.util.List;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BooleanBlock;
import org.elasticsearch.compute.data.BooleanVector;
import org.elasticsearch.compute.data.ElementType;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;

/**
 * {@link AggregatorFunction} implementation for {@link MaxIntAggregator}.
 * This class is generated. Do not edit it.
 */
public final class MaxIntAggregatorFunction implements AggregatorFunction {
  private static final List<IntermediateStateDesc> INTERMEDIATE_STATE_DESC = List.of(
      new IntermediateStateDesc("max", ElementType.INT),
      new IntermediateStateDesc("seen", ElementType.BOOLEAN)  );

  private final DriverContext driverContext;

  private final IntState state;

  private final List<Integer> channels;

  public MaxIntAggregatorFunction(DriverContext driverContext, List<Integer> channels,
      IntState state) {
    this.driverContext = driverContext;
    this.channels = channels;
    this.state = state;
  }

  public static MaxIntAggregatorFunction create(DriverContext driverContext,
      List<Integer> channels) {
    return new MaxIntAggregatorFunction(driverContext, channels, new IntState(MaxIntAggregator.init()));
  }

  public static List<IntermediateStateDesc> intermediateStateDesc() {
    return INTERMEDIATE_STATE_DESC;
  }

  @Override
  public int intermediateBlockCount() {
    return INTERMEDIATE_STATE_DESC.size();
  }

  @Override
  public void addRawInput(Page page) {
    Block uncastBlock = page.getBlock(channels.get(0));
    if (uncastBlock.areAllValuesNull()) {
      return;
    }
    IntBlock block = (IntBlock) uncastBlock;
    IntVector vector = block.asVector();
    if (vector != null) {
      addRawVector(vector);
    } else {
      addRawBlock(block);
    }
  }

  private void addRawVector(IntVector vector) {
    state.seen(true);
    for (int i = 0; i < vector.getPositionCount(); i++) {
      state.intValue(MaxIntAggregator.combine(state.intValue(), vector.getInt(i)));
    }
  }

  private void addRawBlock(IntBlock block) {
    for (int p = 0; p < block.getPositionCount(); p++) {
      if (block.isNull(p)) {
        continue;
      }
      state.seen(true);
      int start = block.getFirstValueIndex(p);
      int end = start + block.getValueCount(p);
      for (int i = start; i < end; i++) {
        state.intValue(MaxIntAggregator.combine(state.intValue(), block.getInt(i)));
      }
    }
  }

  @Override
  public void addIntermediateInput(Page page) {
    assert channels.size() == intermediateBlockCount();
    assert page.getBlockCount() >= channels.get(0) + intermediateStateDesc().size();
    IntVector max = page.<IntBlock>getBlock(channels.get(0)).asVector();
    BooleanVector seen = page.<BooleanBlock>getBlock(channels.get(1)).asVector();
    assert max.getPositionCount() == 1;
    assert max.getPositionCount() == seen.getPositionCount();
    if (seen.getBoolean(0)) {
      state.intValue(MaxIntAggregator.combine(state.intValue(), max.getInt(0)));
      state.seen(true);
    }
  }

  @Override
  public void evaluateIntermediate(Block[] blocks, int offset) {
    state.toIntermediate(blocks, offset);
  }

  @Override
  public void evaluateFinal(Block[] blocks, int offset, DriverContext driverContext) {
    if (state.seen() == false) {
      blocks[offset] = Block.constantNullBlock(1, driverContext.blockFactory());
      return;
    }
    blocks[offset] = IntBlock.newConstantBlockWith(state.intValue(), 1, driverContext.blockFactory());
  }

  @Override
  public String toString() {
    StringBuilder sb = new StringBuilder();
    sb.append(getClass().getSimpleName()).append("[");
    sb.append("channels=").append(channels);
    sb.append("]");
    return sb.toString();
  }

  @Override
  public void close() {
    state.close();
  }
}
