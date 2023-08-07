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

#include "mongo/db/pipeline/group_processor_base.h"

#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/exec/document_value/value_comparator.h"
#include "mongo/db/pipeline/expression.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/stats/counters.h"

namespace mongo {

GroupProcessorBase::GroupProcessorBase(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                       int64_t maxMemoryUsageBytes)
    : _expCtx(expCtx),
      _memoryTracker{expCtx->allowDiskUse && !expCtx->inMongos, maxMemoryUsageBytes},
      _groups(expCtx->getValueComparator().makeUnorderedValueMap<Accumulators>()) {}

void GroupProcessorBase::addAccumulationStatement(AccumulationStatement accumulationStatement) {
    tassert(7801002, "Can't mutate accumulated fields after initialization", !_executionStarted);
    _accumulatedFields.push_back(accumulationStatement);
    _memoryTracker.set(accumulationStatement.fieldName, 0);
}

void GroupProcessorBase::setExecutionStarted() {
    if (!_executionStarted) {
        invariant(_accumulatedFieldMemoryTrackers.empty());
        for (const auto& accum : _accumulatedFields) {
            _accumulatedFieldMemoryTrackers.push_back(&_memoryTracker[accum.fieldName]);
        }
    }
    _executionStarted = true;
}

void GroupProcessorBase::freeMemory() {
    for (auto& group : _groups) {
        for (size_t i = 0; i < group.second.size(); ++i) {
            // Subtract the current usage.
            _accumulatedFieldMemoryTrackers[i]->update(-1 * group.second[i]->getMemUsage());

            group.second[i]->reduceMemoryConsumptionIfAble();

            // Update the memory usage for this AccumulationStatement.
            _accumulatedFieldMemoryTrackers[i]->update(group.second[i]->getMemUsage());
        }
    }
}

void GroupProcessorBase::setIdExpression(const boost::intrusive_ptr<Expression> idExpression) {
    tassert(7801001, "Can't mutate _id fields after initialization", !_executionStarted);
    if (auto object = dynamic_cast<ExpressionObject*>(idExpression.get())) {
        auto& childExpressions = object->getChildExpressions();
        invariant(!childExpressions.empty());  // We expect to have converted an empty object into a
                                               // constant expression.

        // grouping on an "artificial" object. Rather than create the object for each input
        // in initialize(), instead group on the output of the raw expressions. The artificial
        // object will be created at the end in makeDocument() while outputting results.
        for (auto&& childExpPair : childExpressions) {
            _idFieldNames.push_back(childExpPair.first);
            _idExpressions.push_back(childExpPair.second);
        }
    } else {
        _idExpressions.push_back(idExpression);
    }
}

boost::intrusive_ptr<Expression> GroupProcessorBase::getIdExpression() const {
    // _idFieldNames is empty and _idExpressions has one element when the _id expression is not an
    // object expression.
    if (_idFieldNames.empty() && _idExpressions.size() == 1) {
        return _idExpressions[0];
    }

    tassert(6586300,
            "Field and its expression must be always paired in ExpressionObject",
            _idFieldNames.size() > 0 && _idFieldNames.size() == _idExpressions.size());

    // Each expression in '_idExpressions' may have been optimized and so, compose the object _id
    // expression out of the optimized expressions.
    std::vector<std::pair<std::string, boost::intrusive_ptr<Expression>>> fieldsAndExprs;
    for (size_t i = 0; i < _idExpressions.size(); ++i) {
        fieldsAndExprs.emplace_back(_idFieldNames[i], _idExpressions[i]);
    }

    return ExpressionObject::create(_idExpressions[0]->getExpressionContext(),
                                    std::move(fieldsAndExprs));
}

void GroupProcessorBase::reset() {
    // Free our resources.
    _groups = _expCtx->getValueComparator().makeUnorderedValueMap<Accumulators>();
    _memoryTracker.resetCurrent();
}

Value GroupProcessorBase::computeGroupKey(const Document& root) const {
    // If only one expression, return result directly
    if (_idExpressions.size() == 1) {
        Value retValue = _idExpressions[0]->evaluate(root, &_expCtx->variables);
        return retValue.missing() ? Value(BSONNULL) : std::move(retValue);
    } else {
        // Multiple expressions get results wrapped in a vector
        std::vector<Value> vals;
        vals.reserve(_idExpressions.size());
        for (size_t i = 0; i < _idExpressions.size(); i++) {
            vals.push_back(_idExpressions[i]->evaluate(root, &_expCtx->variables));
        }
        return Value(std::move(vals));
    }
}

std::pair<GroupProcessorBase::GroupsMap::iterator, bool> GroupProcessorBase::findOrCreateGroup(
    const Value& key) {
    auto emplaceResult = _groups.try_emplace(key);
    auto& group = emplaceResult.first->second;

    const size_t numAccumulators = _accumulatedFields.size();
    if (emplaceResult.second) {
        _memoryTracker.set(_memoryTracker.currentMemoryBytes() + key.getApproximateSize());

        // Initialize and add the accumulators
        Value expandedId = expandId(key);
        Document idDoc =
            expandedId.getType() == BSONType::Object ? expandedId.getDocument() : Document();
        group.reserve(numAccumulators);
        for (size_t i = 0; i < numAccumulators; i++) {
            const auto& accumulatedField = _accumulatedFields[i];
            auto accum = accumulatedField.makeAccumulator();
            Value initializerValue =
                accumulatedField.expr.initializer->evaluate(idDoc, &_expCtx->variables);
            accum->startNewGroup(initializerValue);
            _accumulatedFieldMemoryTrackers[i]->update(accum->getMemUsage());
            group.push_back(std::move(accum));
        }
    }
    // Check that we have accumulated state for each of the accumulation statements.
    dassert(numAccumulators == group.size());

    return emplaceResult;
}

void GroupProcessorBase::accumulate(GroupsMap::iterator groupIter,
                                    size_t accumulatorIdx,
                                    Value accumulatorArg) {
    const size_t numAccumulators = _accumulatedFields.size();
    invariant(numAccumulators == groupIter->second.size());
    invariant(accumulatorIdx < numAccumulators);

    auto& accumulator = groupIter->second[accumulatorIdx];
    const auto prevMemUsage = accumulator->getMemUsage();
    accumulator->process(accumulatorArg, _doingMerge);
    _accumulatedFieldMemoryTrackers[accumulatorIdx]->update(accumulator->getMemUsage() -
                                                            prevMemUsage);
}

Value GroupProcessorBase::expandId(const Value& val) {
    // _id doesn't get wrapped in a document
    if (_idFieldNames.empty())
        return val;

    // _id is a single-field document containing val
    if (_idFieldNames.size() == 1)
        return Value(DOC(_idFieldNames[0] << val));

    // _id is a multi-field document containing the elements of val
    const std::vector<Value>& vals = val.getArray();
    invariant(_idFieldNames.size() == vals.size());
    MutableDocument md(vals.size());
    for (size_t i = 0; i < vals.size(); i++) {
        md[_idFieldNames[i]] = vals[i];
    }
    return md.freezeToValue();
}

Document GroupProcessorBase::makeDocument(const Value& id,
                                          const Accumulators& accums,
                                          bool mergeableOutput) {
    const size_t n = _accumulatedFields.size();
    MutableDocument out(1 + n);

    // Add the _id field.
    out.addField("_id", expandId(id));

    // Add the rest of the fields.
    for (size_t i = 0; i < n; ++i) {
        Value val = accums[i]->getValue(mergeableOutput);
        if (val.missing()) {
            // we return null in this case so return objects are predictable
            out.addField(_accumulatedFields[i].fieldName, Value(BSONNULL));
        } else {
            out.addField(_accumulatedFields[i].fieldName, std::move(val));
        }
    }

    _stats.totalOutputDataSizeBytes += out.getApproximateSize();
    return out.freeze();
}

}  // namespace mongo
