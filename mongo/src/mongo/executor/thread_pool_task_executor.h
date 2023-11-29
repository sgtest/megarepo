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

#pragma once

#include <boost/optional.hpp>
#include <list>
#include <memory>
#include <mutex>

#include "mongo/base/status_with.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/baton.h"
#include "mongo/db/operation_context.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/executor/task_executor.h"
#include "mongo/platform/mutex.h"
#include "mongo/stdx/condition_variable.h"
#include "mongo/stdx/thread.h"
#include "mongo/transport/baton.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future.h"
#include "mongo/util/hierarchical_acquisition.h"
#include "mongo/util/interruptible.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/time_support.h"

namespace mongo {

class ThreadPoolInterface;

namespace executor {

struct ConnectionPoolStats;
class NetworkInterface;

/**
 * Implementation of a TaskExecutor that uses a pool of threads to execute work items.
 */
class ThreadPoolTaskExecutor final : public TaskExecutor {
    ThreadPoolTaskExecutor(const ThreadPoolTaskExecutor&) = delete;
    ThreadPoolTaskExecutor& operator=(const ThreadPoolTaskExecutor&) = delete;

public:
    /**
     * Constructs an instance of ThreadPoolTaskExecutor that runs tasks in "pool" and uses "net"
     * for network operations.
     */
    ThreadPoolTaskExecutor(std::unique_ptr<ThreadPoolInterface> pool,
                           std::shared_ptr<NetworkInterface> net);

    /**
     * Destroys a ThreadPoolTaskExecutor.
     */
    ~ThreadPoolTaskExecutor();

    void startup() override;
    void shutdown() override;
    void join() override;
    SharedSemiFuture<void> joinAsync() override;
    bool isShuttingDown() const override;
    void appendDiagnosticBSON(BSONObjBuilder* b) const override;
    Date_t now() override;
    StatusWith<EventHandle> makeEvent() override;
    void signalEvent(const EventHandle& event) override;
    StatusWith<CallbackHandle> onEvent(const EventHandle& event, CallbackFn&& work) override;
    StatusWith<stdx::cv_status> waitForEvent(OperationContext* opCtx,
                                             const EventHandle& event,
                                             Date_t deadline) override;
    void waitForEvent(const EventHandle& event) override;
    StatusWith<CallbackHandle> scheduleWork(CallbackFn&& work) override;
    StatusWith<CallbackHandle> scheduleWorkAt(Date_t when, CallbackFn&& work) override;
    StatusWith<CallbackHandle> scheduleRemoteCommandOnAny(
        const RemoteCommandRequestOnAny& request,
        const RemoteCommandOnAnyCallbackFn& cb,
        const BatonHandle& baton = nullptr) override;
    StatusWith<CallbackHandle> scheduleExhaustRemoteCommandOnAny(
        const RemoteCommandRequestOnAny& request,
        const RemoteCommandOnAnyCallbackFn& cb,
        const BatonHandle& baton = nullptr);
    void cancel(const CallbackHandle& cbHandle) override;
    void wait(const CallbackHandle& cbHandle,
              Interruptible* interruptible = Interruptible::notInterruptible()) override;

    void appendConnectionStats(ConnectionPoolStats* stats) const override;

    void dropConnections(const HostAndPort& hostAndPort) override;

    void appendNetworkInterfaceStats(BSONObjBuilder&) const override;

    /**
     * Returns true if there are any tasks in any of _poolInProgressQueue, _networkInProgressQueue,
     * or _sleepersQueue.
     */
    bool hasTasks();

private:
    class CallbackState;
    class EventState;
    using WorkQueue = std::list<std::shared_ptr<CallbackState>>;
    using EventList = std::list<std::shared_ptr<EventState>>;

    /**
     * Representation of the stage of life of a thread pool.
     *
     * A pool starts out in the preStart state, and ends life in the shutdownComplete state.  Work
     * may only be scheduled in the preStart and running states. Threads may only be started in the
     * running state. In shutdownComplete, there are no remaining threads or pending tasks to
     * execute.
     *
     * Diagram of legal transitions:
     *
     * preStart -> running -> joinRequired -> joining -> shutdownComplete
     *        \               ^
     *         \_____________/
     *
     * NOTE: The enumeration values below are compared using operator<, etc, with the expectation
     * that a -> b in the diagram above implies that a < b in the enum below.
     */
    enum State { preStart, running, joinRequired, joining, shutdownComplete };

    /**
     * Returns an EventList containing one unsignaled EventState. This is a helper function for
     * performing allocations outside of _mutex, and should only be called by makeSingletonWork and
     * makeEvent().
     */
    static EventList makeSingletonEventList();

    /**
     * Returns an object suitable for passing to enqueueCallbackState_inlock that represents
     * executing "work" no sooner than "when" (defaults to ASAP). This function may and should be
     * called outside of _mutex.
     */
    static WorkQueue makeSingletonWorkQueue(CallbackFn work,
                                            const BatonHandle& baton,
                                            Date_t when = {});

    /**
     * Moves the single callback in "wq" to the end of "queue". It is required that "wq" was
     * produced via a call to makeSingletonWorkQueue().
     */
    StatusWith<CallbackHandle> enqueueCallbackState_inlock(WorkQueue* queue, WorkQueue* wq);

    /**
     * Signals the given event.
     */
    void signalEvent_inlock(const EventHandle& event, stdx::unique_lock<Latch> lk);

    /**
     * Schedules all items from "fromQueue" into the thread pool and moves them into
     * _poolInProgressQueue.
     */
    void scheduleIntoPool_inlock(WorkQueue* fromQueue, stdx::unique_lock<Latch> lk);

    /**
     * Schedules the given item from "fromQueue" into the thread pool and moves it into
     * _poolInProgressQueue.
     */
    void scheduleIntoPool_inlock(WorkQueue* fromQueue,
                                 boost::optional<WorkQueue::iterator> iter,
                                 stdx::unique_lock<Latch> lk);

    /**
     * Schedules entries from "begin" through "end" in "fromQueue" into the thread pool
     * and moves them into _poolInProgressQueue.
     */
    void scheduleIntoPool_inlock(WorkQueue* fromQueue,
                                 boost::optional<WorkQueue::iterator>& begin,
                                 boost::optional<WorkQueue::iterator>& end,
                                 stdx::unique_lock<Latch> lk);

    /**
     * Schedules cbState into the thread pool and places it into _poolInProgressQueue. Does not
     * remove the entry from the original queue.
     */
    void scheduleExhaustIntoPool_inlock(std::shared_ptr<CallbackState> cbState,
                                        stdx::unique_lock<Latch> lk);
    /**
     * Executes the callback specified by "cbState".
     */
    void runCallback(std::shared_ptr<CallbackState> cbState);

    /**
     * Executes the callback specified by "cbState". Will not mark cbState as finished.
     */
    void runCallbackExhaust(std::shared_ptr<CallbackState> cbState,
                            WorkQueue::iterator expectedExhaustIter);

    bool _inShutdown_inlock() const;
    void _setState_inlock(State newState);
    stdx::unique_lock<Latch> _join(stdx::unique_lock<Latch> lk);

    // The network interface used for remote command execution and waiting.
    std::shared_ptr<NetworkInterface> _net;

    // The thread pool that executes scheduled work items.
    std::shared_ptr<ThreadPoolInterface> _pool;

    // Mutex guarding all remaining fields.
    mutable Mutex _mutex = MONGO_MAKE_LATCH(
        // This is sadly held for a subset of task execution HierarchicalAcquisitionLevel(1),
        "ThreadPoolTaskExecutor::_mutex");

    // Queue containing all items currently scheduled into the thread pool but not yet completed.
    WorkQueue _poolInProgressQueue;

    // Queue containing all items currently scheduled into the network interface.
    WorkQueue _networkInProgressQueue;

    // Queue containing all items waiting for a particular point in time to execute.
    WorkQueue _sleepersQueue;

    // List of all events that have yet to be signaled.
    EventList _unsignaledEvents;

    // Lifecycle state of this executor.
    stdx::condition_variable _stateChange;
    State _state = preStart;

    friend std::shared_ptr<TaskExecutor> makePinnedConnectionTaskExecutor(
        std::shared_ptr<TaskExecutor>);
};

}  // namespace executor
}  // namespace mongo
