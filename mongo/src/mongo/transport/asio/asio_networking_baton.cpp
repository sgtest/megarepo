/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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


#include <sys/eventfd.h>

#include "mongo/transport/asio/asio_networking_baton.h"

#include "mongo/base/checked_cast.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/db/operation_context.h"
#include "mongo/logv2/log.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/errno_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kNetwork


namespace mongo {
namespace transport {
namespace {

MONGO_FAIL_POINT_DEFINE(blockAsioNetworkingBatonBeforePoll);

Status getDetachedError() {
    return {ErrorCodes::ShutdownInProgress, "Baton detached"};
}

Status getCanceledError() {
    return {ErrorCodes::CallbackCanceled, "Baton wait canceled"};
}

/**
 * RAII type that wraps up an `eventfd` and reading/writing to it.
 * We don't use the counter portion and only use the file descriptor (i.e., `fd`) to notify and
 * interrupt the client thread blocked polling (see `AsioNetworkingBaton::run`).
 */
struct EventFDHolder {
    EventFDHolder(const EventFDHolder&) = delete;
    EventFDHolder& operator=(const EventFDHolder&) = delete;

    EventFDHolder() = default;

    ~EventFDHolder() {
        ::close(fd);
    }

    void notify() {
        while (::eventfd_write(fd, 1) != 0) {
            const auto savedErrno = errno;
            if (savedErrno == EINTR)
                continue;
            LOGV2_FATAL(6328202, "eventfd write failed", "fd"_attr = fd, "errno"_attr = savedErrno);
        }
    }

    void wait() {
        // If we have activity on the `eventfd`, pull the count out.
        ::eventfd_t u;
        while (::eventfd_read(fd, &u) != 0) {
            const auto savedErrno = errno;
            if (savedErrno == EINTR)
                continue;
            LOGV2_FATAL(6328203, "eventfd read failed", "fd"_attr = fd, "errno"_attr = savedErrno);
        }
    }

private:
    static int _initFd() {
        int fd = ::eventfd(0, EFD_CLOEXEC);
        // On error, -1 is returned and `errno` is set
        if (fd < 0) {
            auto ec = lastPosixError();
            const auto errorCode = ec == posixError(EMFILE) || ec == posixError(ENFILE)
                ? ErrorCodes::TooManyFilesOpen
                : ErrorCodes::UnknownError;
            Status status(errorCode,
                          fmt::format("error in creating eventfd: {}, errno: {}",
                                      errorMessage(ec),
                                      ec.value()));
            LOGV2_ERROR(6328201, "Unable to create eventfd object", "error"_attr = status);
            iasserted(status);
        }
        return fd;
    }

public:
    const int fd = _initFd();
};

const auto getEventFDForClient = Client::declareDecoration<EventFDHolder>();

EventFDHolder& efd(OperationContext* opCtx) {
    return getEventFDForClient(opCtx->getClient());
}

/**
 * This is only used by `run_until()` and `waitUntil()`, and provides a unique timer id. This unique
 * id is supplied by `ReactorTimer`, and used by the baton for internal bookkeeping.
 */
class DummyTimer final : public ReactorTimer {
public:
    void cancel(const BatonHandle& baton = nullptr) override {
        MONGO_UNREACHABLE;
    }

    Future<void> waitUntil(Date_t timeout, const BatonHandle& baton = nullptr) override {
        MONGO_UNREACHABLE;
    }
};

}  // namespace

void AsioNetworkingBaton::schedule(Task func) noexcept {
    auto task = [this, func = std::move(func)](stdx::unique_lock<Mutex> lk) mutable {
        auto status = _opCtx ? Status::OK() : getDetachedError();
        lk.unlock();
        func(std::move(status));
    };

    stdx::unique_lock lk(_mutex);
    if (!_opCtx) {
        // Run the task inline if the baton is detached.
        task(std::move(lk));
        return;
    }

    _scheduled.push_back(std::move(task));
    if (_inPoll)
        notify();
}

void AsioNetworkingBaton::notify() noexcept {
    efd(_opCtx).notify();
}

Waitable::TimeoutState AsioNetworkingBaton::run_until(ClockSource* clkSource,
                                                      Date_t deadline) noexcept {
    // Set up a timer on the baton with the specified deadline. This synthetic timer is used by
    // `_poll()`, which is called through `run()`, to enforce a deadline for the blocking `::poll`.
    DummyTimer timer;
    auto future = waitUntil(timer, deadline);

    run(clkSource);

    // If the future is ready, our timer interrupted `run()`, in which case we timed out.
    if (future.isReady()) {
        future.get();
        return Waitable::TimeoutState::Timeout;
    } else {
        cancelTimer(timer);
        return Waitable::TimeoutState::NoTimeout;
    }
}

void AsioNetworkingBaton::run(ClockSource* clkSource) noexcept {
    // On the way out, fulfill promises and run scheduled jobs without holding the lock.
    std::list<Promise<void>> toFulfill;
    const ScopeGuard guard([&] {
        for (auto& promise : toFulfill) {
            promise.emplaceValue();
        }

        auto lk = stdx::unique_lock(_mutex);
        while (!_scheduled.empty()) {
            auto scheduled = std::exchange(_scheduled, {});
            for (auto& job : scheduled) {
                job(std::move(lk));
                job = nullptr;
                lk = stdx::unique_lock(_mutex);
            }
        }
    });

    stdx::unique_lock lk(_mutex);

    // If anything was scheduled, run it now and skip polling and processing timers.
    if (!_scheduled.empty())
        return;

    toFulfill.splice(toFulfill.end(), _poll(lk, clkSource));

    // Fire expired timers
    const auto now = clkSource->now();
    for (auto it = _timers.begin(); it != _timers.end() && it->first <= now;
         it = _timers.erase(it)) {
        toFulfill.push_back(std::move(it->second.promise));
        _timersById.erase(it->second.id);
    }
}

void AsioNetworkingBaton::markKillOnClientDisconnect() noexcept {
    auto client = _opCtx->getClient();
    invariant(client);
    if (auto session = client->session()) {
        auto code = client->getDisconnectErrorCode();
        _addSession(*session, POLLRDHUP).getAsync([this, code](Status status) {
            if (status.isOK())
                _opCtx->markKilled(code);
        });
    }
}

Future<void> AsioNetworkingBaton::addSession(Session& session, Type type) noexcept {
    return _addSession(session, type == Type::In ? POLLIN : POLLOUT);
}

Future<void> AsioNetworkingBaton::waitUntil(const ReactorTimer& reactorTimer,
                                            Date_t expiration) noexcept try {
    auto pf = makePromiseFuture<void>();
    _safeExecute(stdx::unique_lock(_mutex),
                 [this, expiration, timer = Timer{reactorTimer.id(), std::move(pf.promise)}](
                     stdx::unique_lock<Mutex>) mutable {
                     auto iter = _timers.emplace(expiration, std::move(timer));
                     _timersById[iter->second.id] = iter;
                 });
    return std::move(pf.future);
} catch (const DBException& ex) {
    return ex.toStatus();
}

Future<void> AsioNetworkingBaton::waitUntil(Date_t expiration, const CancellationToken& token) try {
    auto pf = makePromiseFuture<void>();
    DummyTimer dummy;
    const size_t timerId = dummy.id();
    _safeExecute(stdx::unique_lock(_mutex),
                 [this, timerId, expiration, promise = std::move(pf.promise), &token](
                     stdx::unique_lock<Mutex>) mutable {
                     Timer timer{timerId, std::move(promise)};
                     auto iter = _timers.emplace(expiration, std::move(timer));
                     _timersById[iter->second.id] = iter;
                 });
    token.onCancel().thenRunOn(shared_from_this()).getAsync([this, timerId](Status s) {
        if (s.isOK()) {
            _cancelTimer(timerId);
        }
    });
    return std::move(pf.future);
} catch (const DBException& ex) {
    return ex.toStatus();
}

bool AsioNetworkingBaton::cancelSession(Session& session) noexcept {
    const auto id = session.id();

    stdx::unique_lock lk(_mutex);
    if (_sessions.find(id) == _sessions.end())
        return false;

    _safeExecute(std::move(lk), [this, id](stdx::unique_lock<Mutex> lk) {
        auto iter = _sessions.find(id);
        if (iter == _sessions.end())
            return;

        TransportSession ts = std::exchange(iter->second, {});
        _sessions.erase(iter);
        lk.unlock();

        ts.promise.setError(getCanceledError());
    });

    return true;
}

bool AsioNetworkingBaton::cancelTimer(const ReactorTimer& timer) noexcept {
    const auto id = timer.id();
    return _cancelTimer(id);
}

bool AsioNetworkingBaton::_cancelTimer(size_t id) noexcept {
    stdx::unique_lock lk(_mutex);
    if (_timersById.find(id) == _timersById.end())
        return false;

    _safeExecute(std::move(lk), [this, id](stdx::unique_lock<Mutex> lk) {
        auto iter = _timersById.find(id);
        if (iter == _timersById.end())
            return;

        Timer batonTimer = std::exchange(iter->second->second, {});
        _timers.erase(iter->second);
        _timersById.erase(iter);
        lk.unlock();

        batonTimer.promise.setError(getCanceledError());
    });

    return true;
}

bool AsioNetworkingBaton::canWait() noexcept {
    stdx::lock_guard lk(_mutex);
    return _opCtx;
}

void AsioNetworkingBaton::_safeExecute(stdx::unique_lock<Mutex> lk, AsioNetworkingBaton::Job job) {
    if (!_opCtx) {
        // If we're detached, no job can safely execute.
        iasserted(getDetachedError());
    }

    if (_inPoll) {
        _scheduled.push_back(std::move(job));
        notify();
    } else {
        job(std::move(lk));
    }
}

std::list<Promise<void>> AsioNetworkingBaton::_poll(stdx::unique_lock<Mutex>& lk,
                                                    ClockSource* clkSource) {
    const auto now = clkSource->now();

    // If we have a timer, then use it to enforce a timeout for polling.
    boost::optional<Date_t> deadline;
    if (!_timers.empty()) {
        deadline = _timers.begin()->first;

        // Don't poll if we have already passed the deadline.
        if (*deadline <= now)
            return {};
    }

    if (deadline && !clkSource->tracksSystemClock()) {
        // The clock source and `::poll` may track time differently, so use the clock source to
        // enforce the timeout.
        clkSource->setAlarm(*deadline, [self = shared_from_this()] { self->notify(); });
        deadline.reset();
    }

    _pollSet.clear();
    _pollSet.reserve(_sessions.size() + 1);
    _pollSet.push_back({efd(_opCtx).fd, POLLIN, 0});

    _pollSessions.clear();
    _pollSessions.reserve(_sessions.size());

    for (auto iter = _sessions.begin(); iter != _sessions.end(); ++iter) {
        _pollSet.push_back({iter->second.fd, iter->second.events, 0});
        _pollSessions.push_back(iter);
    }

    int events = [&] {
        _inPoll = true;
        lk.unlock();

        const ScopeGuard guard([&] {
            lk.lock();
            _inPoll = false;
        });

        blockAsioNetworkingBatonBeforePoll.pauseWhileSet();
        int timeout = deadline ? Milliseconds(*deadline - now).count() : -1;
        int events = ::poll(_pollSet.data(), _pollSet.size(), timeout);
        if (events < 0) {
            auto ec = lastSystemError();
            if (ec != systemError(EINTR))
                LOGV2_FATAL(50834, "error in poll", "error"_attr = errorMessage(ec));
        }
        return events;
    }();

    if (events <= 0)
        return {};  // Polling was timed out or interrupted.

    auto psit = _pollSet.begin();
    // Consume the notification on the `eventfd` object if there is any.
    if (psit->revents) {
        efd(_opCtx).wait();
        --events;
    }
    ++psit;

    std::list<Promise<void>> promises;
    for (auto sit = _pollSessions.begin(); events && sit != _pollSessions.end(); ++sit, ++psit) {
        if (psit->revents) {
            promises.push_back(std::move((*sit)->second.promise));
            _sessions.erase(*sit);
            --events;
        }
    }

    invariant(events == 0, "Failed to process all events after going through registered sessions");
    return promises;
}

Future<void> AsioNetworkingBaton::_addSession(Session& session, short events) try {
    auto pf = makePromiseFuture<void>();
    TransportSession ts{checked_cast<AsioSession&>(session).getSocket().native_handle(),
                        events,
                        std::move(pf.promise)};
    _safeExecute(stdx::unique_lock(_mutex),
                 [this, id = session.id(), ts = std::move(ts)](stdx::unique_lock<Mutex>) mutable {
                     invariant(_sessions.emplace(id, std::move(ts)).second,
                               "Adding session to baton failed");
                 });
    return std::move(pf.future);
} catch (const DBException& ex) {
    return ex.toStatus();
}

void AsioNetworkingBaton::detachImpl() noexcept {
    decltype(_scheduled) scheduled;
    decltype(_sessions) sessions;
    decltype(_timers) timers;

    {
        stdx::lock_guard lk(_mutex);

        invariant(_opCtx->getBaton().get() == this);
        _opCtx->setBaton(nullptr);

        _opCtx = nullptr;

        using std::swap;
        swap(_scheduled, scheduled);
        swap(_sessions, sessions);
        swap(_timers, timers);
    }

    for (auto& job : scheduled) {
        job(stdx::unique_lock(_mutex));
        job = nullptr;
    }

    for (auto& session : sessions) {
        session.second.promise.setError(getDetachedError());
    }

    for (auto& pair : timers) {
        pair.second.promise.setError(getDetachedError());
    }
}

}  // namespace transport
}  // namespace mongo
