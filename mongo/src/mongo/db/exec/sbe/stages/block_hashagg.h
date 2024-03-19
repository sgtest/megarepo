/**
 *    Copyright (C) 2024-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#pragma once

#include <absl/hash/hash.h>

#include "mongo/db/exec/sbe/expressions/expression.h"
#include "mongo/db/exec/sbe/stages/hashagg_base.h"
#include "mongo/db/exec/sbe/stages/stages.h"
#include "mongo/db/exec/sbe/values/block_interface.h"
#include "mongo/db/exec/sbe/values/slot.h"
#include "mongo/db/exec/sbe/vm/vm.h"
#include "mongo/stdx/unordered_map.h"

namespace mongo {
namespace sbe {

/**
 * Block equivalent of the HashAgg stage. Only supports algebraic accumulators (median for example
 * is not supported).
 *
 * Debug string representation:
 * block_group bitset=bitmapSlotId [<groupby slot>]
 *     [slot_1 = block_expr_1, ..., slot_n = block_expr_n]
 *     [slot_1 = row_expr_1, ..., slot_n = row_expr_n] [_rowAccSlotId]
 *     childStage
 */
class BlockHashAggStage final : public HashAggBaseStage<BlockHashAggStage> {
public:
    struct BlockRowAccumulators {
        std::unique_ptr<EExpression> blockAgg;
        std::unique_ptr<EExpression> rowAgg;
    };

    // List of pairs, where the first part of the pair is a slot and second part of the pair
    // is a struct of the form {blockAgg, rowAgg} containing the corresponding accumulators.
    using BlockAndRowAggs = std::vector<std::pair<value::SlotId, BlockRowAccumulators>>;

    BlockHashAggStage(std::unique_ptr<PlanStage> input,
                      value::SlotVector groupSlotIds,
                      value::SlotId blockBitsetInSlotId,
                      value::SlotVector blockDataInSlotIds,
                      value::SlotVector accumulatorDataSlotIds,
                      value::SlotId accumulatorBitsetSlotId,
                      BlockAndRowAggs aggs,
                      bool allowDiskUse,
                      SlotExprPairVector mergingExprs,
                      PlanYieldPolicy* yieldPolicy,
                      PlanNodeId planNodeId,
                      bool participateInTrialRunTracking = true,
                      bool forceIncreasedSpilling = false);

    ~BlockHashAggStage();

    std::unique_ptr<PlanStage> clone() const final;

    void prepare(CompileCtx& ctx) final;
    value::SlotAccessor* getAccessor(CompileCtx& ctx, value::SlotId slot) final;
    void open(bool reOpen) final;
    PlanState getNext() final;
    void close() final;

    std::unique_ptr<PlanStageStats> getStats(bool includeDebugInfo) const final;
    const SpecificStats* getSpecificStats() const final;
    HashAggStats* getHashAggStats();
    std::vector<DebugPrinter::Block> debugPrint() const final;
    size_t estimateCompileTimeSize() const final;

    /*
     * TODO SERVER-85731 tune this parameter.
     * The partition approach is essentially O(partition_size*block_size).
     * The elementwise approach is O(block_size).
     * So we could tune this with some constant, or make a possibly smarter decision based on the
     * ratio of block size to number of partitions. If the "num_partitions/block_size" is high, we
     * choose element-wise approach. If it's low, we choose the partition approach.
     */
    static const size_t kMaxNumPartitionsForTokenizedPath = 5;
    // TODO SERVER-85731: Determine what block size is optimal.
    static constexpr size_t kBlockOutSize = 128;

private:
    /*
     * Given the groupby key, looks up the entry in our hash table and runs the block and row
     * accumulators. Assumes that our input slots to these accumulators are already setup.
     */
    void executeBlockLevelAccumulatorCode(const value::MaterializedRow& key);

    void executeRowLevelAccumulatorCode(
        const value::DeblockedTagVals& extractedBitmap,
        const std::vector<value::DeblockedTagVals>& extractedGbInputs,
        const std::vector<value::DeblockedTagVals>& extractedData);

    struct TokenizedKeys {
        std::vector<value::MaterializedRow> keys;
        std::vector<size_t> idxs;
    };

    /**
     * Take a vector of TokenizedBlocks and combine them into the actual keys that will be used for
     * the hash table. These combined keys will then be tokenized again, so that we can identify the
     * unique compound keys. Returns TokenizedKeys containing the actual materialized keys and a
     * vector of indices where keys[idxs[i]] represents the materialized key of the ith element in
     * the input. If the number of unique keys encountered exceeds
     * kMaxNumPartitionsForTokenizedPath, return boost::none and use the element wise path.
     */
    boost::optional<TokenizedKeys> tokenizeTokenInfos(
        const std::vector<value::TokenizedBlock>& tokenInfos,
        const std::vector<value::DeblockedTagVals>& deblockedTokens);

    boost::optional<BlockHashAggStage::TokenizedKeys> tryTokenizeGbs();

    /*
     * Finds the unique values in our input key blocks and processes them in together. For example
     * if half of the keys are 1 and the other half are 2, we can avoid many hash table lookups and
     * accumulator calls by processing the data with the same keys together. This is best if there
     * are only a few partitions.
     */
    void runAccumulatorsTokenized(const TokenizedKeys& tokenizedKeys);

    /*
     * Runs the accumulators on each element of the inputs, one at a time. This is best if the
     * number of unique keys is high so the partitioning approach would be quadratic.
     */
    void runAccumulatorsElementWise();

    // Returns the next accumulator key or boost::none if we've run out of spilled keys.
    boost::optional<value::MaterializedRow> getNextSpilledHelper();
    PlanState getNextSpilled();

    /*
     * Populates the bitmap out-slot with a block of 'nElements' true values.
     */
    void populateBitmapSlot(size_t nElements);

    value::ValueBlock* makeMonoBlock(value::TypeTags tag, value::Value val);

    // Groupby key slots.
    const value::SlotVector _groupSlots;
    std::vector<value::SlotAccessor*> _idInAccessors;

    // Input/output slot for bitset corresponding to data input.
    // On input, this indicates which rows are to be included in the group by.
    // On output, this slot contains a bitset of all 1s, for use by additional block
    // operations.
    const value::SlotId _blockBitsetInSlotId;
    value::SlotAccessor* _blockBitsetInAccessor = nullptr;
    value::OwnedValueAccessor _blockBitsetOutAccessor;

    // Input slots for data, eventually passed to the accumulator data slots.
    value::SlotVector _blockDataInSlotIds;
    std::vector<value::SlotAccessor*> _blockDataInAccessors;

    // Slot for bitset used by block accumulators.
    const value::SlotId _accumulatorBitsetSlotId;
    value::OwnedValueAccessor _accumulatorBitsetAccessor;

    // Used as the input for row-level accumulators.
    const value::SlotVector _accumulatorDataSlotIds;
    std::vector<value::ViewOfValueAccessor> _accumulatorDataAccessors;
    value::SlotAccessorMap _accumulatorDataAccessorMap;

    /*
     * A map from SlotId to a pair of {blockAccumulator, rowAccumulator}. This SlotId is the
     * input the block accumulator reads from, and is also the output that the row accumulator
     * writes to.
     */
    BlockAndRowAggs _blockRowAggs;

    SlotExprPairVector _mergingExprs;

    HashAggStats _specificStats;

    value::SlotAccessorMap _outAccessorsMap;

    std::vector<value::OwnedValueAccessor> _outIdBlockAccessors;
    std::vector<value::HeterogeneousBlock> _outIdBlocks;

    std::vector<value::OwnedValueAccessor> _outAggBlockAccessors;
    std::vector<value::HeterogeneousBlock> _outAggBlocks;

    // Code for block and row accumulators.
    std::vector<std::unique_ptr<vm::CodeFragment>> _blockLevelAggCodes;
    std::vector<std::unique_ptr<vm::CodeFragment>> _aggCodes;

    // Bytecode for the merging expressions, executed if partial aggregates are spilled to a record
    // store and need to be subsequently combined.
    std::vector<std::unique_ptr<vm::CodeFragment>> _mergingExprCodes;

    std::vector<std::unique_ptr<HashAggAccessor>> _rowAggHtAccessors;
    std::vector<std::unique_ptr<value::OwnedValueAccessor>> _rowAggRSAccessors;
    std::vector<std::unique_ptr<value::SwitchAccessor>> _rowAggAccessors;

    // Hash table where we'll map groupby key to the accumulators.
    std::vector<std::unique_ptr<HashKeyAccessor>> _idHtAccessors;

    size_t _currentBlockSize = 0;
    value::ValueBlock* _bitmapBlock = nullptr;
    std::vector<value::ValueBlock*> _gbBlocks;
    std::vector<value::ValueBlock*> _dataBlocks;
    std::vector<value::TokenizedBlock> _tokenInfos;
    std::vector<value::DeblockedTagVals> _deblockedTokens;
    std::deque<boost::optional<value::MonoBlock>> _monoBlocks;

    vm::ByteCode _bytecode;
    bool _compiled = false;

    bool _done = false;

    // Partial aggregates that have been spilled and restored are passed into the bytecode in
    // '_mergingExprCodes' via '_spilledAccessors' so that they can be merged to compute the
    // final aggregate value.
    std::vector<value::ViewOfValueAccessor> _spilledAccessors;
    value::SlotAccessorMap _spilledAccessorMap;

    // Place to stash the next keys and values during the streaming phase. The record store cursor
    // doesn't offer a "peek" API, so we need to hold onto the next row between getNext() calls when
    // the key value advances.
    BufBuilder _stashedBuffer;
    BufBuilder _currentBuffer;
    boost::optional<SpilledRow> _stashedNextRow;
};

}  // namespace sbe
}  // namespace mongo
