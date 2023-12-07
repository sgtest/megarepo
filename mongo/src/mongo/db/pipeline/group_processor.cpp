/**
 *    Copyright (C) 2018-present MongoDB, Inc.
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

#include "mongo/db/pipeline/group_processor.h"

#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/exec/document_value/value_comparator.h"
#include "mongo/db/pipeline/expression.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/stats/counters.h"

namespace mongo {

namespace {

/**
 * Generates a new file name on each call using a static, atomic and monotonically increasing
 * number.
 *
 * Each user of the Sorter must implement this function to ensure that all temporary files that the
 * Sorter instances produce are uniquely identified using a unique file name extension with separate
 * atomic variable. This is necessary because the sorter.cpp code is separately included in multiple
 * places, rather than compiled in one place and linked, and so cannot provide a globally unique ID.
 */
std::string nextFileName() {
    static AtomicWord<unsigned> documentSourceGroupFileCounter;
    return "extsort-doc-group." + std::to_string(documentSourceGroupFileCounter.fetchAndAdd(1));
}

}  // namespace

GroupProcessor::GroupProcessor(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                               int64_t maxMemoryUsageBytes)
    : GroupProcessorBase(expCtx, maxMemoryUsageBytes) {}

boost::optional<Document> GroupProcessor::getNext() {
    if (_spilled) {
        return getNextSpilled();
    } else {
        return getNextStandard();
    }
}

boost::optional<Document> GroupProcessor::getNextSpilled() {
    // We aren't streaming, and we have spilled to disk.
    if (!_sorterIterator)
        return boost::none;

    Value currentId = _firstPartOfNextGroup.first;
    const size_t numAccumulators = _accumulatedFields.size();

    // Call startNewGroup on every accumulator.
    Value expandedId = expandId(currentId);
    Document idDoc =
        expandedId.getType() == BSONType::Object ? expandedId.getDocument() : Document();
    for (size_t i = 0; i < numAccumulators; ++i) {
        Value initializerValue =
            _accumulatedFields[i].expr.initializer->evaluate(idDoc, &_expCtx->variables);
        _currentAccumulators[i]->reset();
        _currentAccumulators[i]->startNewGroup(initializerValue);
    }

    while (_expCtx->getValueComparator().evaluate(currentId == _firstPartOfNextGroup.first)) {
        // Inside of this loop, _firstPartOfNextGroup is the current data being processed.
        // At loop exit, it is the first value to be processed in the next group.
        switch (numAccumulators) {  // mirrors switch in spill()
            case 1:                 // Single accumulators serialize as a single Value.
                _currentAccumulators[0]->process(_firstPartOfNextGroup.second, true);
                [[fallthrough]];
            case 0:  // No accumulators so no Values.
                break;
            default: {  // Multiple accumulators serialize as an array of Values.
                const std::vector<Value>& accumulatorStates =
                    _firstPartOfNextGroup.second.getArray();
                for (size_t i = 0; i < numAccumulators; i++) {
                    _currentAccumulators[i]->process(accumulatorStates[i], true);
                }
            }
        }

        if (!_sorterIterator->more()) {
            _sorterIterator.reset();
            break;
        }

        _firstPartOfNextGroup = _sorterIterator->next();
    }

    return makeDocument(currentId, _currentAccumulators, _expCtx->needsMerge);
}

boost::optional<Document> GroupProcessor::getNextStandard() {
    // Not spilled, and not streaming.
    if (!_groupsIterator || _groupsIterator == _groups.end())
        return boost::none;

    auto& it = *_groupsIterator;

    Document out = makeDocument(it->first, it->second, _expCtx->needsMerge);
    ++it;
    return out;
}

namespace {

using GroupsMap = GroupProcessorBase::GroupsMap;

class SorterComparator {
public:
    SorterComparator(ValueComparator valueComparator) : _valueComparator(valueComparator) {}

    int operator()(const Value& lhs, const Value& rhs) const {
        return _valueComparator.compare(lhs, rhs);
    }

private:
    ValueComparator _valueComparator;
};

class SpillSTLComparator {
public:
    SpillSTLComparator(ValueComparator valueComparator) : _valueComparator(valueComparator) {}

    bool operator()(const GroupsMap::value_type* lhs, const GroupsMap::value_type* rhs) const {
        return _valueComparator.evaluate(lhs->first < rhs->first);
    }

private:
    ValueComparator _valueComparator;
};

}  // namespace

void GroupProcessor::add(const Value& groupKey, const Document& root) {
    auto [groupIter, inserted] = findOrCreateGroup(groupKey);

    const size_t numAccumulators = _accumulatedFields.size();
    auto& group = groupIter->second;
    for (size_t i = 0; i < numAccumulators; i++) {
        // Only process the input and update the memory footprint if the current accumulator
        // needs more input.
        if (group[i]->needsInput()) {
            accumulate(groupIter, i, computeAccumulatorArg(root, i));
        }
    }

    if (shouldSpillWithAttemptToSaveMemory() || shouldSpillForDebugBuild(inserted)) {
        spill();
    }
}

void GroupProcessor::readyGroups() {
    _spilled = !_sortedFiles.empty();
    if (_spilled) {
        if (!_groups.empty()) {
            spill();
        }

        _groups = _expCtx->getValueComparator().makeUnorderedValueMap<Accumulators>();

        _sorterIterator.reset(Sorter<Value, Value>::Iterator::merge(
            _sortedFiles, SortOptions(), SorterComparator(_expCtx->getValueComparator())));

        // prepare current to accumulate data
        _currentAccumulators.reserve(_accumulatedFields.size());
        for (const auto& accumulatedField : _accumulatedFields) {
            _currentAccumulators.push_back(accumulatedField.makeAccumulator());
        }

        MONGO_verify(_sorterIterator->more());  // we put data in, we should get something out.
        _firstPartOfNextGroup = _sorterIterator->next();
    } else {
        // start the group iterator
        _groupsIterator = _groups.begin();
    }
}

void GroupProcessor::reset() {
    // Free our resources.
    GroupProcessorBase::reset();

    _sorterIterator.reset();
    _sortedFiles.clear();
    // Make us look done.
    _groupsIterator = _groups.end();
}

bool GroupProcessor::shouldSpillWithAttemptToSaveMemory() {
    if (!_memoryTracker.allowDiskUse() && !_memoryTracker.withinMemoryLimit()) {
        freeMemory();
    }

    if (!_memoryTracker.withinMemoryLimit()) {
        uassert(ErrorCodes::QueryExceededMemoryLimitNoDiskUseAllowed,
                "Exceeded memory limit for $group, but didn't allow external sort."
                " Pass allowDiskUse:true to opt in.",
                _memoryTracker.allowDiskUse());
        return true;
    }
    return false;
}

bool GroupProcessor::shouldSpillForDebugBuild(bool isNewGroup) {
    // In debug mode, spill every time we have a duplicate id to stress merge logic.
    return (kDebugBuild && !_expCtx->opCtx->readOnly() && !isNewGroup &&  // is not a new group
            !_expCtx->inMongos &&             // can't spill to disk in mongos
            _memoryTracker.allowDiskUse() &&  // never spill when disk use is explicitly prohibited
            _sortedFiles.size() < 20);
}

void GroupProcessor::spill() {
    _stats.spills++;
    _stats.numBytesSpilledEstimate += _memoryTracker.currentMemoryBytes();
    _stats.spilledRecords += _groups.size();

    std::vector<const GroupProcessorBase::GroupsMap::value_type*>
        ptrs;  // using pointers to speed sorting
    ptrs.reserve(_groups.size());
    for (auto it = _groups.begin(), end = _groups.end(); it != end; ++it) {
        ptrs.push_back(&*it);
    }

    stable_sort(ptrs.begin(), ptrs.end(), SpillSTLComparator(_expCtx->getValueComparator()));

    // Initialize '_file' in a lazy manner only when it is needed.
    if (!_file) {
        _spillStats = std::make_unique<SorterFileStats>(nullptr /* sorterTracker */);
        _file = std::make_shared<Sorter<Value, Value>::File>(
            _expCtx->tempDir + "/" + nextFileName(), _spillStats.get());
    }
    SortedFileWriter<Value, Value> writer(SortOptions().TempDir(_expCtx->tempDir), _file);
    switch (_accumulatedFields.size()) {  // same as ptrs[i]->second.size() for all i.
        case 0:                           // no values, essentially a distinct
            for (size_t i = 0; i < ptrs.size(); i++) {
                writer.addAlreadySorted(ptrs[i]->first, Value());
            }
            break;

        case 1:  // just one value, use optimized serialization as single Value
            for (size_t i = 0; i < ptrs.size(); i++) {
                writer.addAlreadySorted(ptrs[i]->first,
                                        ptrs[i]->second[0]->getValue(/*toBeMerged=*/true));
            }
            break;

        default:  // multiple values, serialize as array-typed Value
            for (size_t i = 0; i < ptrs.size(); i++) {
                std::vector<Value> accums;
                for (size_t j = 0; j < ptrs[i]->second.size(); j++) {
                    accums.push_back(ptrs[i]->second[j]->getValue(/*toBeMerged=*/true));
                }
                writer.addAlreadySorted(ptrs[i]->first, Value(std::move(accums)));
            }
            break;
    }

    auto& metricsCollector = ResourceConsumption::MetricsCollector::get(_expCtx->opCtx);
    metricsCollector.incrementKeysSorted(ptrs.size());
    metricsCollector.incrementSorterSpills(1);

    // Zero out the current per-accumulation statement memory consumption, as the memory has been
    // freed by spilling.
    GroupProcessorBase::reset();

    _sortedFiles.emplace_back(writer.done());
    if (_spillStats) {
        _stats.spilledDataStorageSize = _spillStats->bytesSpilled();
    }
}

}  // namespace mongo

#include "mongo/db/sorter/sorter.cpp"
// Explicit instantiation unneeded since we aren't exposing Sorter outside of this file.
