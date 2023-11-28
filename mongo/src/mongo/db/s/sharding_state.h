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

#include <boost/optional/optional.hpp>
#include <cstdint>
#include <string>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/bson/oid.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/service_context.h"
#include "mongo/db/shard_id.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/platform/mutex.h"

namespace mongo {

/**
 * There is one instance of this object per service context on each shard node (primary or
 * secondary). It sits at the top of the hierarchy of the Shard Role runtime-authoritative caches
 * (the subordinate ones being the DatabaseShardingState and CollectionShardingState) and contains
 * global information about the shardedness of the current process, such as its shardId and the
 * clusterId to which it belongs.
 *
 * SYNCHRONISATION: This class can only be initialised once and if 'setInitialized' is called, it
 * never gets destroyed or uninitialized. Because of this it does not require external
 * synchronisation. Initialisation is driven from outside (specifically
 * ShardingInitializationMongoD, which should be its only caller).
 */
class ShardingState {
    ShardingState(const ShardingState&) = delete;
    ShardingState& operator=(const ShardingState&) = delete;

public:
    ShardingState();
    ~ShardingState();

    static ShardingState* get(ServiceContext* serviceContext);
    static ShardingState* get(OperationContext* operationContext);

    /**
     * Puts the sharding state singleton in the "initialization completed" state with either
     * successful initialization or an error. This method may only be called once for the lifetime
     * of the object.
     */
    void setInitialized(ShardId shardId, OID clusterId);
    void setInitialized(Status failedStatus);

    /**
     * If 'setInitialized' has not been called, returns boost::none. Otherwise, returns the status
     * with which 'setInitialized' was called. This is used by the initialization sequence to decide
     * whether to set up the sharding services.
     */
    boost::optional<Status> initializationStatus();

    /**
     * Returns true if 'setInitialized' has been called with shardId and clusterId.
     *
     * Code that needs to perform extra actions if sharding is initialized, but does not need to
     * error if not, should use this. Alternatively, see ShardingState::canAcceptShardedCommands().
     */
    bool enabled() const;

    /**
     * Waits until sharding state becomes enabled or a new error occurred after we started waiting.
     */
    void waitUntilEnabled(OperationContext* opCtx);

    /**
     * Returns Status::OK if the ShardingState is enabled; if not, returns an error describing
     * whether the ShardingState is just not yet initialized, or if this shard is not running with
     * --shardsvr at all.
     *
     * Code that should error if sharding state has not been initialized should use this to report
     * a more descriptive error. Alternatively, see ShardingState::enabled().
     */
    Status canAcceptShardedCommands() const;

    /**
     * Returns the shard id to which this node belongs.
     */
    ShardId shardId();

    /**
     * Returns the cluster id of the cluster to which this node belongs.
     */
    OID clusterId();

    /**
     * For testing only. This is a workaround for the fact that it is not possible to get a clean
     * ServiceContext in between test executions. Because of this, tests which require that they get
     * started with a clean (uninitialized) ShardingState must invoke this in their tearDown method.
     */
    void clearForTests();

private:
    // Progress of the sharding state initialization
    enum class InitializationState : uint32_t {
        // Initial state. The server must be under exclusive lock when this state is entered. No
        // metadata is available yet and it is not known whether there is any min optime metadata,
        // which needs to be recovered. From this state, the server may enter INITIALIZING, if a
        // recovey document is found or stay in it until initialize has been called.
        kNew,

        // Sharding state is fully usable.
        kInitialized,

        // Some initialization error occurred. The _initializationStatus variable will contain the
        // error.
        kError,
    };

    /**
     * Returns the initialization state.
     */
    InitializationState _getInitializationState() const {
        return static_cast<InitializationState>(_initializationState.load());
    }

    // Protects state for initializing '_shardId', '_clusterId', and '_initializationStatus'.
    // Protects read access for '_initializationStatus'.
    Mutex _mutex = MONGO_MAKE_LATCH("ShardingState::_mutex");

    // State of the initialization of the sharding state along with any potential errors
    AtomicWord<unsigned> _initializationState{static_cast<uint32_t>(InitializationState::kNew)};

    // For '_shardId' and '_clusterId':
    //
    // These variables will invariant if attempts are made to access them before 'enabled()' has
    // been set to true. 'enabled()' being set to true guarantees that these variables have been
    // set, and that they will remain unchanged for the duration of the ShardingState object's
    // lifetime. Because these variables will not change, it's unnecessary to acquire the class's
    // mutex in order to call these getters and access their underlying data.

    // Sets the shard name for this host.
    ShardId _shardId;

    // The id for the cluster this shard belongs to.
    OID _clusterId;

    // Only valid if _initializationState is kError. Contains the reason for initialization failure.
    Status _initializationStatus{ErrorCodes::InternalError, "Uninitialized value"};
    stdx::condition_variable _initStateChangedCV;
};

}  // namespace mongo
