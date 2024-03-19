/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <memory>
#include <queue>
#include <utility>

#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_set_window_fields.h"
#include "mongo/db/pipeline/expression.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/window_function/partition_iterator.h"
#include "mongo/db/pipeline/window_function/window_bounds.h"
#include "mongo/db/pipeline/window_function/window_function.h"
#include "mongo/db/query/sort_pattern.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/memory_usage_tracker.h"

namespace mongo {

struct WindowFunctionStatement;

/**
 * An interface for an executor class capable of evaluating a function over a given window
 * definition. The function must expose an accumulate-type interface and potentially a remove
 * interface depending on the window bounds.
 *
 * This class is also responsible for handling partition edge cases; for instance when either the
 * lower bound falls before the start of the partition or the upper bound spills off of the end.
 */
class WindowFunctionExec {
public:
    /**
     * Creates an appropriate WindowFunctionExec that is capable of evaluating the window function
     * over the given bounds, both found within the WindowFunctionStatement.
     */
    static std::unique_ptr<WindowFunctionExec> create(ExpressionContext* expCtx,
                                                      PartitionIterator* iter,
                                                      const WindowFunctionStatement& functionStmt,
                                                      const boost::optional<SortPattern>& sortBy,
                                                      MemoryUsageTracker* memTracker);

    virtual ~WindowFunctionExec() = default;

    /**
     * Retrieve the next value computed by the window function.
     */
    virtual Value getNext() = 0;

    /**
     * Resets the executor as well as any execution state to a clean slate.
     */
    virtual void reset() = 0;

protected:
    WindowFunctionExec(PartitionAccessor iter, MemoryUsageTracker::Impl* memTracker)
        : _iter(iter), _memTracker(memTracker){};

    PartitionAccessor _iter;
    MemoryUsageTracker::Impl* _memTracker;
};

/**
 * Base class for executors that need to remove documents from their held functions. The
 * 'WindowFunctionState' parameter must expose an 'add()' and corresponding
 * 'getValue()' method to get the accumulation result. It must also expose a 'remove()' method to
 * remove a specific document from the calculation.
 */
class WindowFunctionExecRemovable : public WindowFunctionExec {
public:
    Value getNext() override {
        update();
        return _function->getValue();
    }

    void reset() override {
        _function->reset();
        _values = std::queue<MemoryUsageTokenWith<Value>>();
        _memTracker->set(_function->getApproximateSize());
        doReset();
    }

protected:
    WindowFunctionExecRemovable(PartitionIterator* iter,
                                PartitionAccessor::Policy policy,
                                boost::intrusive_ptr<Expression> input,
                                std::unique_ptr<WindowFunctionState> function,
                                MemoryUsageTracker::Impl* memTracker)
        : WindowFunctionExec(PartitionAccessor(iter, policy), memTracker),
          _input(std::move(input)),
          _function(std::move(function)) {
        _memTracker->set(_function->getApproximateSize());
    }

    void addValue(Value v) {
        long long prior = _function->getApproximateSize();
        _function->add(v);
        _values.emplace(MemoryUsageToken{v.getApproximateSize(), _memTracker}, std::move(v));
        _memTracker->add(_function->getApproximateSize() - prior);
    }

    void removeValue() {
        tassert(5429400, "Tried to remove more values than we added", !_values.empty());
        long long prior = _function->getApproximateSize();
        auto& v = _values.front();
        _function->remove(std::move(v.value()));
        _values.pop();
        _memTracker->add(_function->getApproximateSize() - prior);
    }

    boost::intrusive_ptr<Expression> _input;
    // Keep track of values in the window function that will need to be removed later.
    std::queue<MemoryUsageTokenWith<Value>> _values;

private:
    /**
     * This method notifies the executor that the underlying PartitionIterator
     * '_iter' has been advanced one time since the last call to initialize() or
     * update(). It should determine how the window has changed (which documents have
     * entered it? which have left it?) and call addValue(), removeValue() as needed.
     */
    virtual void update() = 0;

    /**
     * Derived classes should reset their own internal state in the implementation of this instead
     * of overriding `reset()` to allow for resetting the values owned by the base class.
     */
    virtual void doReset() = 0;

    std::unique_ptr<WindowFunctionState> _function;
};

}  // namespace mongo
