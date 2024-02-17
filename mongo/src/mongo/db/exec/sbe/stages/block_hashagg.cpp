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

#include "mongo/db/exec/sbe/values/block_interface.h"
#include "mongo/db/exec/sbe/values/value.h"

#include "mongo/db/exec/sbe/expressions/compile_ctx.h"
#include "mongo/db/exec/sbe/stages/block_hashagg.h"
#include "mongo/db/storage/kv/kv_engine.h"
#include "mongo/db/storage/storage_engine.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

#include "mongo/db/exec/sbe/size_estimator.h"

namespace mongo {
namespace sbe {
namespace {
bool allFalse(std::pair<value::TypeTags, value::Value> bitset) {
    invariant(bitset.first == value::TypeTags::valueBlock);
    // TODO SERVER-85739 use special cases for different types of blocks.
    const auto& deblocked = value::bitcastTo<value::ValueBlock*>(bitset.second)->extract();
    for (size_t i = 0; i < deblocked.count(); i++) {
        invariant(deblocked[i].first == value::TypeTags::Boolean);
        if (value::bitcastTo<bool>(deblocked[i].second)) {
            return false;
        }
    }
    return true;
}

// Given a vector of partition IDs, and partition ID, create a bitset indicating whether each
// element in the vector matches the given partition ID.
std::unique_ptr<value::ValueBlock> computeBitmapForPartition(
    const std::vector<size_t>& partitionMap, size_t partition) {
    std::vector<bool> bitmap;
    bitmap.resize(partitionMap.size());
    for (size_t i = 0; i < partitionMap.size(); i++) {
        bitmap[i] = (partitionMap[i] == partition);
    }
    return std::make_unique<value::BoolBlock>(std::move(bitmap));
}

// Takes two bitsets of equal size, one as a ValueBlock and one as a tag/val, and returns a bitset
// of the same size with elements pairwise ANDed together.
std::unique_ptr<value::ValueBlock> bitAnd(value::ValueBlock* bitset1, value::ValueBlock* bitset2) {
    // TODO SERVER-85738 Implement efficient bitAnd operation on blocks.
    auto vals1 = bitset1->extract();
    auto vals2 = bitset2->extract();
    invariant(vals1.count() == vals2.count());

    std::vector<bool> vecResult;
    vecResult.resize(vals1.count());
    for (size_t i = 0; i < vals1.count(); i++) {
        invariant(vals1[i].first == value::TypeTags::Boolean &&
                  vals2[i].first == value::TypeTags::Boolean);
        vecResult[i] =
            value::bitcastTo<bool>(vals1[i].second) && value::bitcastTo<bool>(vals2[i].second);
    }
    return std::make_unique<value::BoolBlock>(std::move(vecResult));
}

/*
 * Block that holds a view of a single value. It does not take ownership of the given value. This is
 * used because the block accumulators expect block inputs, but in some cases we may need to provide
 * is scalars that we do not own.
 * Used only for BlockHashAgg.
 */
class SingletonViewBlock final : public value::ValueBlock {
public:
    SingletonViewBlock() {}

    SingletonViewBlock(value::TypeTags tag, value::Value val) : _tag(tag), _val(val) {}

    void setTagVal(std::pair<value::TypeTags, value::Value> tagVal) {
        _tag = tagVal.first;
        _val = tagVal.second;
    }

    std::unique_ptr<ValueBlock> clone() const override {
        return std::make_unique<SingletonViewBlock>(_tag, _val);
    }

    boost::optional<size_t> tryCount() const override {
        return 1;
    }

    value::DeblockedTagVals deblock(
        boost::optional<value::DeblockedTagValStorage>& storage) override {
        return {1, &_tag, &_val};
    }

private:
    value::TypeTags _tag;
    value::Value _val;
};
}  // namespace

BlockHashAggStage::BlockHashAggStage(std::unique_ptr<PlanStage> input,
                                     value::SlotId groupSlotId,
                                     boost::optional<value::SlotId> blockBitsetInSlotId,
                                     value::SlotVector blockDataInSlotIds,
                                     value::SlotId rowAccSlotId,
                                     value::SlotId accumulatorBitsetSlotId,
                                     value::SlotVector accumulatorDataSlotIds,
                                     BlockAndRowAggs aggs,
                                     PlanNodeId planNodeId,
                                     bool participateInTrialRunTracking)
    : PlanStage("block_hashagg"_sd, planNodeId, participateInTrialRunTracking),
      _groupSlot(groupSlotId),
      _blockBitsetInSlotId(blockBitsetInSlotId),
      _blockDataInSlotIds(std::move(blockDataInSlotIds)),
      _accumulatorBitsetSlotId(accumulatorBitsetSlotId),
      _accumulatorDataSlotIds(std::move(accumulatorDataSlotIds)),
      _rowAccSlotId(rowAccSlotId),
      _blockRowAggs(std::move(aggs)) {
    invariant(_blockDataInSlotIds.size() == _accumulatorDataSlotIds.size());

    _children.emplace_back(std::move(input));
    _outAggBlocks.resize(_blockRowAggs.size());
    _blockDataInAccessors.resize(_blockDataInSlotIds.size());
    _accumulatorDataAccessors.resize(_accumulatorDataSlotIds.size());
}

std::unique_ptr<PlanStage> BlockHashAggStage::clone() const {
    BlockAndRowAggs blockRowAggs;
    for (const auto& [slot, aggs] : _blockRowAggs) {
        blockRowAggs.emplace(slot,
                             BlockRowAccumulators{aggs.blockAgg->clone(), aggs.rowAgg->clone()});
    }

    return std::make_unique<BlockHashAggStage>(_children[0]->clone(),
                                               _groupSlot,
                                               _blockBitsetInSlotId,
                                               _blockDataInSlotIds,
                                               _rowAccSlotId,
                                               _accumulatorBitsetSlotId,
                                               _accumulatorDataSlotIds,
                                               std::move(blockRowAggs),
                                               _commonStats.nodeId,
                                               _participateInTrialRunTracking);
}

void BlockHashAggStage::doSaveState(bool relinquishCursor) {}

void BlockHashAggStage::doRestoreState(bool relinquishCursor) {}

void BlockHashAggStage::doDetachFromOperationContext() {}

void BlockHashAggStage::doAttachToOperationContext(OperationContext* opCtx) {}

void BlockHashAggStage::prepare(CompileCtx& ctx) {
    _children[0]->prepare(ctx);

    _idHtAccessor.emplace(_htIt, 0);
    _idAccessorIn = _children[0]->getAccessor(ctx, _groupSlot);
    invariant(_idAccessorIn);

    if (_blockBitsetInSlotId) {
        _blockBitsetInAccessor = _children[0]->getAccessor(ctx, *_blockBitsetInSlotId);
        invariant(_blockBitsetInAccessor);
    }

    for (size_t i = 0; i < _blockDataInSlotIds.size(); i++) {
        _blockDataInAccessors[i] = _children[0]->getAccessor(ctx, _blockDataInSlotIds[i]);
        invariant(_blockDataInAccessors[i]);
    }

    value::SlotSet dupCheck;
    auto throwIfDupSlot = [&dupCheck](boost::optional<value::SlotId> slot) {
        if (slot) {
            auto [_, inserted] = dupCheck.emplace(*slot);
            tassert(7953400, "duplicate slot id", inserted);
        }
    };

    _outAccessorsMap.reserve(_blockRowAggs.size() + 1);
    throwIfDupSlot(_groupSlot);
    throwIfDupSlot(_blockBitsetInSlotId);
    _outAccessorsMap[_groupSlot] = &_outIdBlockAccessor;

    _outAggBlockAccessors.resize(_blockRowAggs.size());
    // Change the agg slot accessors to point to the blocks.
    size_t i = 0;
    for (auto& outBlock : _outAggBlocks) {
        _outAggBlockAccessors[i].reset(
            false, value::TypeTags::valueBlock, value::bitcastFrom<value::ValueBlock*>(&outBlock));
        ++i;
    }

    i = 0;
    for (auto& [slot, aggs] : _blockRowAggs) {
        throwIfDupSlot(slot);

        _rowAggHtAccessors.emplace_back(std::make_unique<HashAggAccessor>(_htIt, i));
        _outAccessorsMap[slot] = &_outAggBlockAccessors[i];

        ctx.root = this;
        ctx.aggExpression = true;
        ctx.accumulator = _rowAggHtAccessors.back().get();
        _aggCodes.emplace_back(aggs.rowAgg->compile(ctx));
        ctx.aggExpression = false;

        // Also compile the block level agg.
        _blockLevelAggCodes.emplace_back(aggs.blockAgg->compile(ctx));
        ++i;
    }
    _compiled = true;
}

value::SlotAccessor* BlockHashAggStage::getAccessor(CompileCtx& ctx, value::SlotId slot) {
    if (_compiled && _outAccessorsMap.count(slot)) {
        return _outAccessorsMap[slot];
    } else {
        if (_rowAccSlotId == slot) {
            return &_rowAccAccessor;
        } else if (_accumulatorBitsetSlotId == slot) {
            return &_accumulatorBitsetAccessor;
        }
        for (size_t i = 0; i < _accumulatorDataSlotIds.size(); i++) {
            if (_accumulatorDataSlotIds[i] == slot) {
                return &_accumulatorDataAccessors[i];
            }
        }
        return _children[0]->getAccessor(ctx, slot);
    }
}

void BlockHashAggStage::executeAccumulatorCode(value::MaterializedRow key) {
    // If all bits are false, there's no work to do. We don't want to make an erroneous
    // entry in our hash map.
    if (allFalse(_accumulatorBitsetAccessor.getViewOfValue())) {
        return;
    }

    _htIt = _ht.find(key);
    if (_htIt == _ht.end()) {
        // New key we haven't seen before.
        key.makeOwned();
        auto [it, _] = _ht.emplace(std::move(key), value::MaterializedRow{0});
        // Initialize accumulators.
        it->second.resize(_rowAggHtAccessors.size());
        _htIt = it;
    }

    // Now run the block level accumulators followed by the row level accumulators.
    size_t i = 0;
    for (auto& blockAccum : _blockLevelAggCodes) {
        auto [owned, tag, val] = _bytecode.run(blockAccum.get());

        // Now run the row level accumulator.
        _rowAccAccessor.reset(owned, tag, val);
        auto [rowOwned, rowTag, rowVal] = _bytecode.run(_aggCodes[i].get());
        _rowAggHtAccessors[i]->reset(rowOwned, rowTag, rowVal);
        ++i;
    }
}

void BlockHashAggStage::runAccumulatorsTokenized(size_t nPartitions,
                                                 value::DeblockedTagVals deblockedTokens,
                                                 value::TokenizedBlock tokenInfo) {
    auto bitmapInTag = value::TypeTags::Nothing;
    auto bitmapInVal = value::Value{0};

    if (_blockBitsetInAccessor) {
        std::tie(bitmapInTag, bitmapInVal) = _blockBitsetInAccessor->getViewOfValue();
        invariant(bitmapInTag == value::TypeTags::valueBlock);
    }

    // Process the accumulators for each partition rather than one element at a time.
    for (size_t partition = 0; partition < nPartitions; ++partition) {
        value::MaterializedRow key{1};
        auto [idTag, idVal] = deblockedTokens[partition];
        key.reset(0, false, idTag, idVal);

        // The accumulators use `_accumulatorBitsetAccessor` to determine which values to
        // accumulate. If we have multiple partitions, we need some additional logic to
        // indicate which partition we're processing.
        // TODO SERVER-85739 we can avoid allocating a new bitset for every input. We
        // can potentially reuse the same bitset. It also might not be worth the
        // additional code complexity.
        if (nPartitions > 1 || !_blockBitsetInAccessor) {
            // Combine the partition bitmap and input bitmap using bitAnd().
            auto partitionBitset = computeBitmapForPartition(tokenInfo.idxs, partition);

            auto accBitset = _blockBitsetInAccessor
                ? bitAnd(partitionBitset.get(), value::bitcastTo<value::ValueBlock*>(bitmapInVal))
                : std::move(partitionBitset);

            _accumulatorBitsetAccessor.reset(
                true,
                value::TypeTags::valueBlock,
                value::bitcastFrom<value::ValueBlock*>(accBitset.release()));
        } else {
            // The partition bitmap would be all 1s if we computed it, so we can just use
            // the input bitmap in this case.
            _accumulatorBitsetAccessor.reset(false, bitmapInTag, bitmapInVal);
        }

        for (size_t i = 0; i < _blockDataInAccessors.size(); i++) {
            auto [dataTag, dataVal] = _blockDataInAccessors[i]->getViewOfValue();
            _accumulatorDataAccessors[i].reset(false, dataTag, dataVal);
        }

        executeAccumulatorCode(std::move(key));
    }
}

void BlockHashAggStage::runAccumulatorsElementWise(size_t blockSize) {
    auto bitmapInTag = value::TypeTags::Nothing;
    auto bitmapInVal = value::Value{0};
    boost::optional<value::DeblockedTagVals> extractedBitmap;

    if (_blockBitsetInAccessor) {
        std::tie(bitmapInTag, bitmapInVal) = _blockBitsetInAccessor->getViewOfValue();
        invariant(bitmapInTag == value::TypeTags::valueBlock);

        extractedBitmap.emplace(value::bitcastTo<value::ValueBlock*>(bitmapInVal)->extract());
        invariant(extractedBitmap->count() == blockSize);
    }

    auto [gbInputTag, gbInputVal] = _idAccessorIn->getViewOfValue();
    invariant(gbInputTag == value::TypeTags::valueBlock);
    auto extractedGbInput = value::bitcastTo<value::ValueBlock*>(gbInputVal)->extract();

    const size_t numDataInputs = _accumulatorDataSlotIds.size();

    // Extract each data block into this array for when we process them element-wise.
    std::vector<value::DeblockedTagVals> extractedDataIn;
    extractedDataIn.resize(numDataInputs);
    for (size_t i = 0; i < numDataInputs; i++) {
        auto [dataTag, dataVal] = _blockDataInAccessors[i]->getViewOfValue();
        invariant(dataTag == value::TypeTags::valueBlock);
        extractedDataIn[i] = value::bitcastTo<value::ValueBlock*>(dataVal)->extract();
    }

    // Create bitmap and blocks to hold in accessors. We update these blocks as we loop through the
    // data in the blocks. For the bitmap, we can avoid the overhead of running the accumulators by
    // checking the input bit first. We can hold a singular `true` in the accumulator bitmap slot
    // since we know it'll be true.
    value::BoolBlock singletonBitmap(std::vector<bool>{true});
    _accumulatorBitsetAccessor.reset(false,
                                     value::TypeTags::valueBlock,
                                     value::bitcastFrom<value::ValueBlock*>(&singletonBitmap));

    std::vector<SingletonViewBlock> singletonDataBlocks;
    singletonDataBlocks.resize(numDataInputs);
    for (size_t i = 0; i < numDataInputs; i++) {
        _accumulatorDataAccessors[i].reset(
            false,
            value::TypeTags::valueBlock,
            value::bitcastFrom<value::ValueBlock*>(&singletonDataBlocks[i]));
    }

    for (size_t blockIndex = 0; blockIndex < blockSize; ++blockIndex) {
        value::MaterializedRow key{1};
        auto [idTag, idVal] = extractedGbInput[blockIndex];
        key.reset(0, false, idTag, idVal);

        if (extractedBitmap) {
            auto [bitTag, bitVal] = (*extractedBitmap)[blockIndex];
            invariant(bitTag == value::TypeTags::Boolean);

            if (!value::bitcastTo<bool>(bitVal)) {
                continue;
            }
        }

        // Update our accessors (via the blocks) with the current value.
        for (size_t i = 0; i < numDataInputs; i++) {
            singletonDataBlocks[i].setTagVal(extractedDataIn[i][blockIndex]);
        }

        // Run the accumulators to update the hash map.
        executeAccumulatorCode(std::move(key));
    }

    _accumulatorBitsetAccessor.reset(false, value::TypeTags::Nothing, 0);
    for (size_t i = 0; i < numDataInputs; i++) {
        _accumulatorDataAccessors[i].reset(false, value::TypeTags::Nothing, 0);
    }
}

void BlockHashAggStage::open(bool reOpen) {
    auto optTimer(getOptTimer(_opCtx));
    _children[0]->open(reOpen);
    _commonStats.opens++;

    if (reOpen) {
        _done = false;
    }

    while (PlanState::ADVANCED == _children[0]->getNext()) {
        auto [gbInputTag, gbInputVal] = _idAccessorIn->getViewOfValue();
        value::TokenizedBlock tokenInfo;
        if (gbInputTag == value::TypeTags::valueBlock) {
            tokenInfo = value::getValueBlock(gbInputVal)->tokenize();
        } else {
            // For single value input, create a singleton block for tokenization.
            auto [gbInputCopyTag, gbInputCopyVal] = value::copyValue(gbInputTag, gbInputVal);
            tokenInfo.tokens = std::make_unique<value::HeterogeneousBlock>(
                std::vector{gbInputCopyTag}, std::vector{gbInputCopyVal});
            tokenInfo.idxs.push_back(0);
        }

        auto deblockedTokens = tokenInfo.tokens->extract();
        const auto nPartitions = deblockedTokens.count();

        if (nPartitions <= kMaxNumPartitionsForTokenizedPath) {
            runAccumulatorsTokenized(nPartitions, std::move(deblockedTokens), std::move(tokenInfo));
        } else {
            runAccumulatorsElementWise(tokenInfo.idxs.size());
        }

        if (_tracker && _tracker->trackProgress<TrialRunTracker::kNumResults>(1)) {
            // During trial runs, we want to limit the amount of work done by opening a blocking
            // stage, like this one. The blocking stage tracks the number of documents it has
            // read from its child, and if the TrialRunTracker ends the trial, a special
            // exception returns control back to the planner.
            _tracker = nullptr;
            _children[0]->close();
            uasserted(ErrorCodes::QueryTrialRunCompleted, "Trial run early exit in group");
        }
    }

    _accumulatorBitsetAccessor.reset(false, value::TypeTags::Nothing, 0);
    _htIt = _ht.end();
}

PlanState BlockHashAggStage::getNext() {
    auto optTimer(getOptTimer(_opCtx));

    _outIdBlock.clear();
    _outIdBlock.reserve(kBlockOutSize);
    for (auto& b : _outAggBlocks) {
        b.clear();
        b.reserve(kBlockOutSize);
    }

    _outIdBlockAccessor.reset(
        false, value::TypeTags::valueBlock, value::bitcastFrom<value::ValueBlock*>(&_outIdBlock));
    size_t numRows = 0;
    while (numRows < kBlockOutSize) {
        if (_htIt == _ht.end()) {
            _htIt = _ht.begin();
        } else {
            ++_htIt;
        }

        if (_done) {
            return trackPlanState(PlanState::IS_EOF);
        }

        if (_htIt == _ht.end()) {
            _done = true;
            if (numRows == 0) {
                return trackPlanState(PlanState::IS_EOF);
            } else {
                return trackPlanState(PlanState::ADVANCED);
            }
        }

        invariant(_outAggBlocks.size() == _outAggBlockAccessors.size());
        invariant(_outAggBlocks.size() == _rowAggHtAccessors.size());

        // Copy the key from the current element in the HT into the out block.
        auto [t, v] = _idHtAccessor->copyOrMoveValue();
        _outIdBlock.push_back(t, v);

        // Copy the values from the current element in the HT into the out block.
        size_t i = 0;
        for (auto& acc : _rowAggHtAccessors) {
            auto [t, v] = acc->copyOrMoveValue();
            _outAggBlocks[i].push_back(t, v);
            ++i;
        }

        ++numRows;
    }

    return trackPlanState(PlanState::ADVANCED);
}

std::unique_ptr<PlanStageStats> BlockHashAggStage::getStats(bool includeDebugInfo) const {
    auto ret = std::make_unique<PlanStageStats>(_commonStats);
    ret->specific = std::make_unique<HashAggStats>(_specificStats);

    if (includeDebugInfo) {
        DebugPrinter printer;
        BSONObjBuilder bob;
        bob.append("groupBySlot", _groupSlot);
        bob.append("rowAccSlotId", _rowAccSlotId);

        BSONObjBuilder blockExprBob(bob.subobjStart("blockExpressions"));
        for (auto&& [slot, expr] : _blockRowAggs) {
            blockExprBob.append(str::stream() << slot, printer.print(expr.blockAgg->debugPrint()));
        }

        BSONObjBuilder rowExprBob(bob.subobjStart("rowExpressions"));
        for (auto&& [slot, expr] : _blockRowAggs) {
            rowExprBob.append(str::stream() << slot, printer.print(expr.rowAgg->debugPrint()));
        }

        ret->debugInfo = bob.obj();
    }

    ret->children.emplace_back(_children[0]->getStats(includeDebugInfo));
    return ret;
}

const SpecificStats* BlockHashAggStage::getSpecificStats() const {
    return &_specificStats;
}

void BlockHashAggStage::close() {
    auto optTimer(getOptTimer(_opCtx));

    trackClose();
    _children[0]->close();
}

std::vector<DebugPrinter::Block> BlockHashAggStage::debugPrint() const {
    auto ret = PlanStage::debugPrint();

    ret.emplace_back(DebugPrinter::Block("[`"));
    DebugPrinter::addIdentifier(ret, _groupSlot);
    ret.emplace_back(DebugPrinter::Block("`]"));

    // Print the row-level and block-level accessors.
    for (bool blockExprPrint : {true, false}) {
        ret.emplace_back(DebugPrinter::Block("[`"));
        bool first = true;
        for (auto&& [slot, expr] : _blockRowAggs) {
            if (!first) {
                ret.emplace_back(DebugPrinter::Block("`,"));
            }

            DebugPrinter::addIdentifier(ret, slot);
            ret.emplace_back("=");
            const auto& aggExpr = blockExprPrint ? expr.blockAgg : expr.rowAgg;
            DebugPrinter::addBlocks(ret, aggExpr->debugPrint());
            first = false;
        }
        ret.emplace_back("`]");
    }

    ret.emplace_back(DebugPrinter::Block("[`"));
    DebugPrinter::addIdentifier(ret, _rowAccSlotId);
    ret.emplace_back(DebugPrinter::Block("`]"));

    DebugPrinter::addNewLine(ret);
    DebugPrinter::addBlocks(ret, _children[0]->debugPrint());

    return ret;
}

size_t BlockHashAggStage::estimateCompileTimeSize() const {
    size_t size = sizeof(*this);
    size += size_estimator::estimate(_children);
    size += size_estimator::estimate(_blockRowAggs);
    size += size_estimator::estimate(_outAccessorsMap);
    return size;
}

void BlockHashAggStage::doDetachFromTrialRunTracker() {
    _tracker = nullptr;
}

PlanStage::TrialRunTrackerAttachResultMask BlockHashAggStage::doAttachToTrialRunTracker(
    TrialRunTracker* tracker, TrialRunTrackerAttachResultMask childrenAttachResult) {
    // The BlockHashAggStage only tracks the "numResults" metric when it is the most deeply nested
    // blocking stage.
    if (!(childrenAttachResult & TrialRunTrackerAttachResultFlags::AttachedToBlockingStage)) {
        _tracker = tracker;
    }

    // Return true to indicate that the tracker is attached to a blocking stage: either this stage
    // or one of its descendent stages.
    return childrenAttachResult | TrialRunTrackerAttachResultFlags::AttachedToBlockingStage;
}

}  // namespace sbe
}  // namespace mongo
