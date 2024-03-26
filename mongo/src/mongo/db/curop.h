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

#include <algorithm>
#include <atomic>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <map>
#include <memory>
#include <ratio>
#include <string>
#include <utility>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/user_acquisition_stats.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/client.h"
#include "mongo/db/commands.h"
#include "mongo/db/concurrency/flow_control_ticketholder.h"
#include "mongo/db/concurrency/lock_stats.h"
#include "mongo/db/cursor_id.h"
#include "mongo/db/database_name.h"
#include "mongo/db/generic_cursor_gen.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/operation_cpu_timer.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/profile_filter.h"
#include "mongo/db/query/cursor_response_gen.h"
#include "mongo/db/query/plan_executor.h"
#include "mongo/db/query/plan_summary_stats.h"
#include "mongo/db/query/query_stats/data_bearing_node_metrics.h"
#include "mongo/db/query/query_stats/key.h"
#include "mongo/db/server_options.h"
#include "mongo/db/stats/resource_consumption_metrics.h"
#include "mongo/db/storage/storage_stats.h"
#include "mongo/db/tenant_id.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/logv2/attribute_storage.h"
#include "mongo/logv2/log_options.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/rpc/message.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/duration.h"
#include "mongo/util/progress_meter.h"
#include "mongo/util/serialization_context.h"
#include "mongo/util/string_map.h"
#include "mongo/util/system_tick_source.h"
#include "mongo/util/tick_source.h"
#include "mongo/util/time_support.h"

#ifndef MONGO_CONFIG_USE_RAW_LATCHES
#include "mongo/util/diagnostic_info.h"
#endif

namespace mongo {

class Client;
class CurOp;
class OperationContext;
struct PlanSummaryStats;

/* lifespan is different than CurOp because of recursives with DBDirectClient */
class OpDebug {
public:
    /**
     * Holds counters for execution statistics that can be accumulated by one or more operations.
     * They're accumulated as we go for a single operation, but are also extracted and stored
     * externally if they need to be accumulated across multiple operations (which have multiple
     * CurOps), including for cursors and multi-statement transactions.
     */
    class AdditiveMetrics {
    public:
        AdditiveMetrics() = default;
        AdditiveMetrics(const AdditiveMetrics& other) {
            this->add(other);
        }

        AdditiveMetrics& operator=(const AdditiveMetrics& other) {
            reset();
            add(other);
            return *this;
        }

        /**
         * Adds all the fields of another AdditiveMetrics object together with the fields of this
         * AdditiveMetrics instance.
         */
        void add(const AdditiveMetrics& otherMetrics);

        /**
         * Adds all of the fields of the given DataBearingNodeMetrics object together with the
         * corresponding fields of this object.
         */
        void aggregateDataBearingNodeMetrics(const query_stats::DataBearingNodeMetrics& metrics);
        void aggregateDataBearingNodeMetrics(
            const boost::optional<query_stats::DataBearingNodeMetrics>& metrics);

        /**
         * Aggregate CursorMetrics (e.g., from a remote cursor) into this AdditiveMetrics instance.
         */
        void aggregateCursorMetrics(const CursorMetrics& metrics);

        /**
         * Resets all members to the default state.
         */
        void reset();

        /**
         * Returns true if the AdditiveMetrics object we are comparing has the same field values as
         * this AdditiveMetrics instance.
         */
        bool equals(const AdditiveMetrics& otherMetrics) const;

        /**
         * Increments writeConflicts by n.
         */
        void incrementWriteConflicts(long long n);

        /**
         * Increments temporarilyUnavailableErrors by n.
         */
        void incrementTemporarilyUnavailableErrors(long long n);

        /**
         * Increments keysInserted by n.
         */
        void incrementKeysInserted(long long n);

        /**
         * Increments keysDeleted by n.
         */
        void incrementKeysDeleted(long long n);

        /**
         * Increments nreturned by n.
         */
        void incrementNreturned(long long n);

        /**
         * Increments nBatches by 1.
         */
        void incrementNBatches();

        /**
         * Increments ninserted by n.
         */
        void incrementNinserted(long long n);

        /**
         * Increments nUpserted by n.
         */
        void incrementNUpserted(long long n);

        /**
         * Increments prepareReadConflicts by n.
         */
        void incrementPrepareReadConflicts(long long n);

        /**
         * Increments executionTime by n.
         */
        void incrementExecutionTime(Microseconds n);

        /**
         * Generates a string showing all non-empty fields. For every non-empty field field1,
         * field2, ..., with corresponding values value1, value2, ..., we will output a string in
         * the format: "<field1>:<value1> <field2>:<value2> ...".
         */
        std::string report() const;
        BSONObj reportBSON() const;

        void report(logv2::DynamicAttributes* pAttrs) const;

        boost::optional<long long> keysExamined;
        boost::optional<long long> docsExamined;

        // Number of records that match the query.
        boost::optional<long long> nMatched;
        // Number of records returned so far.
        boost::optional<long long> nreturned;
        // Number of batches returned so far.
        boost::optional<long long> nBatches;
        // Number of records written (no no-ops).
        boost::optional<long long> nModified;
        boost::optional<long long> ninserted;
        boost::optional<long long> ndeleted;
        boost::optional<long long> nUpserted;

        // Number of index keys inserted.
        boost::optional<long long> keysInserted;
        // Number of index keys removed.
        boost::optional<long long> keysDeleted;

        // The following fields are atomic because they are reported by CurrentOp. This is an
        // exception to the prescription that OpDebug only be used by the owning thread because
        // these metrics are tracked over the course of a transaction by SingleTransactionStats,
        // which is built on OpDebug.

        // Number of read conflicts caused by a prepared transaction.
        AtomicWord<long long> prepareReadConflicts{0};
        AtomicWord<long long> writeConflicts{0};
        AtomicWord<long long> temporarilyUnavailableErrors{0};

        // Amount of time spent executing a query.
        boost::optional<Microseconds> executionTime;

        // True if the query plan involves an in-memory sort.
        bool hasSortStage{false};
        // True if the given query used disk.
        bool usedDisk{false};
        // True if any plan(s) involved in servicing the query (including internal queries sent to
        // shards) came from the multi-planner (not from the plan cache and not a query with a
        // single solution).
        bool fromMultiPlanner{false};
        // False unless all plan(s) involved in servicing the query came from the plan cache.
        // This is because we want to report a "negative" outcome (plan cache miss) if any internal
        // query involved missed the cache. Optional because we need tri-state (true, false, not
        // set) to make the "sticky towards false" logic work.
        boost::optional<bool> fromPlanCache;
    };

    OpDebug() = default;

    void report(OperationContext* opCtx,
                const SingleThreadedLockStats* lockStats,
                const ResourceConsumption::OperationMetrics* operationMetrics,
                logv2::DynamicAttributes* pAttrs) const;

    void reportStorageStats(logv2::DynamicAttributes* pAttrs) const;

    /**
     * Appends information about the current operation to "builder"
     *
     * @param curop reference to the CurOp that owns this OpDebug
     * @param lockStats lockStats object containing locking information about the operation
     */
    void append(OperationContext* opCtx,
                const SingleThreadedLockStats& lockStats,
                FlowControlTicketholder::CurOp flowControlStats,
                BSONObjBuilder& builder) const;

    static std::function<BSONObj(ProfileFilter::Args args)> appendStaged(StringSet requestedFields,
                                                                         bool needWholeDocument);
    static void appendUserInfo(const CurOp&, BSONObjBuilder&, AuthorizationSession*);

    /**
     * Copies relevant plan summary metrics to this OpDebug instance.
     */
    void setPlanSummaryMetrics(const PlanSummaryStats& planSummaryStats);

    /**
     * The resulting object has zeros omitted. As is typical in this file.
     */
    static BSONObj makeFlowControlObject(FlowControlTicketholder::CurOp flowControlStats);

    /**
     * Make object from $search stats with non-populated values omitted.
     */
    BSONObj makeMongotDebugStatsObject() const;

    /**
     * Gets the type of the namespace on which the current operation operates.
     */
    std::string getCollectionType(const NamespaceString& nss) const;

    /**
     * Accumulate resolved views.
     */
    void addResolvedViews(const std::vector<NamespaceString>& namespaces,
                          const std::vector<BSONObj>& pipeline);

    /**
     * Get or append the array with resolved views' info.
     */
    BSONArray getResolvedViewsInfo() const;
    void appendResolvedViewsInfo(BSONObjBuilder& builder) const;

    /**
     * Get a snapshot of the cursor metrics suitable for inclusion in a command response.
     */
    CursorMetrics getCursorMetrics() const;

    // -------------------

    // basic options
    // _networkOp represents the network-level op code: OP_QUERY, OP_GET_MORE, OP_MSG, etc.
    NetworkOp networkOp{opInvalid};  // only set this through setNetworkOp_inlock() to keep synced
    // _logicalOp is the logical operation type, ie 'dbQuery' regardless of whether this is an
    // OP_QUERY find, a find command using OP_QUERY, or a find command using OP_MSG.
    // Similarly, the return value will be dbGetMore for both OP_GET_MORE and getMore command.
    LogicalOp logicalOp{LogicalOp::opInvalid};  // only set this through setNetworkOp_inlock()
    bool iscommand{false};

    // detailed options
    long long cursorid{-1};
    bool exhaust{false};

    // For search using mongot.
    boost::optional<long long> mongotCursorId{boost::none};
    boost::optional<long long> msWaitingForMongot{boost::none};
    long long mongotBatchNum = 0;
    BSONObj mongotCountVal = BSONObj();
    BSONObj mongotSlowQueryLog = BSONObj();

    long long sortSpills{0};           // The total number of spills to disk from sort stages
    size_t sortTotalDataSizeBytes{0};  // The amount of data we've sorted in bytes
    long long keysSorted{0};           // The number of keys that we've sorted.
    long long collectionScans{0};      // The number of collection scans during query execution.
    long long collectionScansNonTailable{0};  // The number of non-tailable collection scans.
    std::set<std::string> indexesUsed;        // The indexes used during query execution.

    // True if a replan was triggered during the execution of this operation.
    boost::optional<std::string> replanReason;

    bool cursorExhausted{
        false};  // true if the cursor has been closed at end a find/getMore operation

    BSONObj execStats;  // Owned here.

    // The hash of the PlanCache key for the query being run. This may change depending on what
    // indexes are present.
    boost::optional<uint32_t> planCacheKey;
    // The hash of the query's "stable" key. This represents the query's shape.
    boost::optional<uint32_t> queryHash;
    // The hash of the query's shape.
    boost::optional<query_shape::QueryShapeHash> queryShapeHash;

    /* The QueryStatsInfo struct was created to bundle all the queryStats related fields of CurOp &
     * OpDebug together (SERVER-83280).
     *
     * ClusterClientCursorImpl and ClientCursor also contain _queryStatsKey and _queryStatsKeyHash
     * members but NOT a wasRateLimited member. Variable names & accesses would be more consistent
     * across the code if ClusterClientCursorImpl and ClientCursor each also had a QueryStatsInfo
     * struct, but we considered and rejected two different potential implementations of this:
     *  - Option 1:
     *    Declare a QueryStatsInfo struct in each .h file. Every struct would have key and keyHash
     *    fields, and a wasRateLimited field would be added only to CurOp. But, it seemed confusing
     *    to have slightly different structs with the same name declared three different times.
     *  - Option 2:
     *    Create a query_stats_info.h that declares QueryStatsInfo--identical to the version defined
     *    in this file. CurOp/OpDebug, ClientCursor, and ClusterClientCursorImpl would then all
     *    have their own QueryStatsInfo instances, potentially as a unique_ptr or boost::optional. A
     *    benefit to this would be the ability to to just move the entire QueryStatsInfo struct from
     *    Op to the Cursor, instead of copying it over field by field (the current method). But:
     *      - The current code moves ownership of the key, but copies the keyHash. So, for workflows
     *        that require multiple cursors, like sharding, one cursor would own the key, but all
     *        cursors would have copies of the keyHash. The problem with trying to move around the
     *        struct in its entirety is that access to the *entire* struct would be lost on the
     *        move, meaning there's no way to retain the keyHash (that doesn't largely nullify the
     *        benefits of having the struct).
     *      - It seemed odd to have ClientCursor and ClusterClientCursorImpl using the struct but
     *        never needing the wasRateLimited field.
     */

    // Note that the only case when key, keyHash, and wasRateLimited of the below struct are null,
    // none, and false is if the query stats feature flag is turned off.
    struct QueryStatsInfo {
        // Uniquely identifies one query stats entry.
        // nullptr if `wasRateLimited` is true.
        std::unique_ptr<query_stats::Key> key;
        // A cached value of `absl::HashOf(key)`.
        // Always populated if `key` is non-null. boost::none if `wasRateLimited` is true.
        boost::optional<std::size_t> keyHash;
        // True if the request was rate limited and stats should not be collected.
        bool wasRateLimited = false;
        // Sometimes we need to request metrics as part of a higher-level operation without
        // actually caring about the metrics for this specific operation. In those cases, we
        // use metricsRequested to indicate we should request metrics from other nodes.
        bool metricsRequested = false;
    };

    QueryStatsInfo queryStatsInfo;

    // The query framework that this operation used. Will be unknown for non query operations.
    PlanExecutor::QueryFramework queryFramework{PlanExecutor::QueryFramework::kUnknown};

    // Tracks the amount of indexed loop joins in a pushed down lookup stage.
    int indexedLoopJoin{0};

    // Tracks the amount of nested loop joins in a pushed down lookup stage.
    int nestedLoopJoin{0};

    // Tracks the amount of hash lookups in a pushed down lookup stage.
    int hashLookup{0};

    // Tracks the amount of spills by hash lookup in a pushed down lookup stage.
    int hashLookupSpillToDisk{0};

    // Details of any error (whether from an exception or a command returning failure).
    Status errInfo = Status::OK();

    // Amount of time spent planning the query. Begins after parsing and ends
    // after optimizations.
    Microseconds planningTime{0};

    // Cost computed by the cost-based optimizer.
    boost::optional<double> estimatedCost;
    // Cardinality computed by the cost-based optimizer.
    boost::optional<double> estimatedCardinality;

    // Amount of CPU time used by this thread. Will remain -1 if this platform does not support
    // this feature.
    Nanoseconds cpuTime{-1};

    int responseLength{-1};

    // Shard targeting info.
    int nShards{-1};

    // Stores the duration of time spent blocked on prepare conflicts.
    Milliseconds prepareConflictDurationMillis{0};

    // Total time spent looking up database entry in the local catalog cache, including eventual
    // refreshes.
    Milliseconds catalogCacheDatabaseLookupMillis{0};

    // Total time spent looking up collection entry in the local catalog cache, including eventual
    // refreshes.
    Milliseconds catalogCacheCollectionLookupMillis{0};

    // Total time spent looking up index entries in the local cache, including eventual refreshes.
    Milliseconds catalogCacheIndexLookupMillis{0};

    // Stores the duration of time spent waiting for the shard to refresh the database and wait for
    // the database critical section.
    Milliseconds databaseVersionRefreshMillis{0};

    // Stores the duration of time spent waiting for the shard to refresh the collection and wait
    // for the collection critical section.
    Milliseconds placementVersionRefreshMillis{0};

    // Stores the duration of time spent waiting for the specified user write concern to
    // be fulfilled.
    Milliseconds waitForWriteConcernDurationMillis{0};

    // Stores the duration of time spent waiting in a queue for a ticket to be acquired.
    Milliseconds waitForTicketDurationMillis{0};

    // Stores the duration of execution after removing time spent blocked.
    Milliseconds workingTimeMillis{0};

    // Stores the total time an operation spends with an uncommitted oplog slot held open. Indicator
    // that an operation is holding back replication by causing oplog holes to remain open for
    // unusual amounts of time.
    Microseconds totalOplogSlotDurationMicros{0};

    // Stores the amount of the data processed by the throttle cursors in MB/sec.
    boost::optional<float> dataThroughputLastSecond;
    boost::optional<float> dataThroughputAverage;

    // Used to track the amount of time spent waiting for a response from remote operations.
    boost::optional<Microseconds> remoteOpWaitTime;

    // Stores the current operation's count of these metrics. If they are needed to be accumulated
    // elsewhere, they should be extracted by another aggregator (like the ClientCursor) to ensure
    // these only ever reflect just this CurOp's consumption.
    AdditiveMetrics additiveMetrics;

    // Stores storage statistics.
    std::unique_ptr<StorageStats> storageStats;

    bool waitingForFlowControl{false};

    // Records the WC that was waited on during the operation. (The WC in opCtx can't be used
    // because it's only set while the Command itself executes.)
    boost::optional<WriteConcernOptions> writeConcern;

    // Whether this is an oplog getMore operation for replication oplog fetching.
    bool isReplOplogGetMore{false};

    // Maps namespace of a resolved view to its dependency chain and the fully unrolled pipeline. To
    // make log line deterministic and easier to test, use ordered map. As we don't expect many
    // resolved views per query, a hash map would unlikely provide any benefits.
    std::map<NamespaceString, std::pair<std::vector<NamespaceString>, std::vector<BSONObj>>>
        resolvedViews;

    // Stores the time the operation spent waiting for ingress admission control ticket
    Microseconds waitForIngressAdmissionTicketDurationMicros{0};
};

/**
 * Container for data used to report information about an OperationContext.
 *
 * Every OperationContext in a server with CurOp support has a stack of CurOp
 * objects. The entry at the top of the stack is used to record timing and
 * resource statistics for the executing operation or suboperation.
 *
 * All of the accessor methods on CurOp may be called by the thread executing
 * the associated OperationContext at any time, or by other threads that have
 * locked the context's owning Client object.
 *
 * The mutator methods on CurOp whose names end _inlock may only be called by the thread
 * executing the associated OperationContext and Client, and only when that thread has also
 * locked the Client object.  All other mutators may only be called by the thread executing
 * CurOp, but do not require holding the Client lock.  The exception to this is the kill()
 * method, which is self-synchronizing.
 *
 * The OpDebug member of a CurOp, accessed via the debug() accessor should *only* be accessed
 * from the thread executing an operation, and as a result its fields may be accessed without
 * any synchronization.
 */
class CurOp {
    CurOp(const CurOp&) = delete;
    CurOp& operator=(const CurOp&) = delete;

public:
    static CurOp* get(const OperationContext* opCtx);
    static CurOp* get(const OperationContext& opCtx);

    /**
     * Writes a report of the operation being executed by the given client to the supplied
     * BSONObjBuilder, in a format suitable for display in currentOp. Does not include a lockInfo
     * report, since this may be called in either a mongoD or mongoS context and the latter does not
     * supply lock stats. The client must be locked before calling this method.
     */
    static void reportCurrentOpForClient(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                         Client* client,
                                         bool truncateOps,
                                         bool backtraceMode,
                                         BSONObjBuilder* infoBuilder);

    static bool currentOpBelongsToTenant(Client* client, TenantId tenantId);

    /**
     * Serializes the fields of a GenericCursor which do not appear elsewhere in the currentOp
     * output. If 'maxQuerySize' is given, truncates the cursor's originatingCommand but preserves
     * the comment.
     */
    static BSONObj truncateAndSerializeGenericCursor(GenericCursor* cursor,
                                                     boost::optional<size_t> maxQuerySize);

    /**
     * Pushes this CurOp to the top of the given "opCtx"'s CurOp stack.
     */
    void push(OperationContext* opCtx);

    CurOp() = default;

    /**
     * This allows the caller to set the command on the CurOp without using setCommand_inlock and
     * having to acquire the Client lock or having to leave a comment indicating why the
     * client lock isn't necessary.
     */
    explicit CurOp(const Command* command) : _command{command} {}

    ~CurOp();

    /**
     * Fills out CurOp and OpDebug with basic info common to all commands. We require the NetworkOp
     * in order to distinguish which protocol delivered this request, e.g. OP_QUERY or OP_MSG. This
     * is set early in the request processing backend and does not typically need to be called
     * thereafter. Locks the client as needed to apply the specified settings.
     */
    void setGenericOpRequestDetails(NamespaceString nss,
                                    const Command* command,
                                    BSONObj cmdObj,
                                    NetworkOp op);

    /**
     * Sets metrics collected at the end of an operation onto curOp's OpDebug instance. Note that
     * this is used in tandem with OpDebug::setPlanSummaryMetrics so should not repeat any metrics
     * collected there.
     */
    void setEndOfOpMetrics(long long nreturned);

    /**
     * Marks the operation end time, records the length of the client response if a valid response
     * exists, and then - subject to the current values of slowMs and sampleRate - logs this CurOp
     * to file under the given LogComponent. Returns 'true' if, in addition to being logged, this
     * operation should also be profiled.
     */
    bool completeAndLogOperation(const logv2::LogOptions& logOptions,
                                 std::shared_ptr<const ProfileFilter> filter,
                                 boost::optional<size_t> responseLength = boost::none,
                                 boost::optional<long long> slowMsOverride = boost::none,
                                 bool forceLog = false);

    bool haveOpDescription() const {
        return !_opDescription.isEmpty();
    }

    /**
     * The BSONObj returned may not be owned by CurOp. Callers should call getOwned() if they plan
     * to reference beyond the lifetime of this CurOp instance.
     */
    BSONObj opDescription() const {
        return _opDescription;
    }

    /**
     * Returns an owned BSONObj representing the original command. Used only by the getMore
     * command.
     */
    BSONObj originatingCommand() const {
        return _originatingCommand;
    }

    void enter_inlock(NamespaceString nss, int dbProfileLevel);
    void enter_inlock(const DatabaseName& dbName, int dbProfileLevel);

    /**
     * Sets the type of the current network operation.
     */
    void setNetworkOp_inlock(NetworkOp op) {
        _networkOp = op;
        _debug.networkOp = op;
    }

    /**
     * Sets the type of the current logical operation.
     */
    void setLogicalOp_inlock(LogicalOp op) {
        _logicalOp = op;
        _debug.logicalOp = op;
    }

    /**
     * Marks the current operation as being a command.
     */
    void markCommand_inlock() {
        _isCommand = true;
    }

    /**
     * Returns a structure containing data used for profiling, accessed only by a thread
     * currently executing the operation context associated with this CurOp.
     */
    OpDebug& debug() {
        return _debug;
    }

    /**
     * Gets the name of the namespace on which the current operation operates.
     */
    std::string getNS() const;

    /**
     * Returns a non-const copy of the UserAcquisitionStats shared_ptr. The caller takes shared
     * ownership of the userAcquisitionStats.
     */
    SharedUserAcquisitionStats getUserAcquisitionStats() const {
        return _userAcquisitionStats;
    }

    /**
     * Gets the name of the namespace on which the current operation operates.
     */
    const NamespaceString& getNSS() const {
        return _nss;
    }

    /**
     * Returns true if the elapsed time of this operation is such that it should be profiled or
     * profile level is set to 2. Uses total time if the operation is done, current elapsed time
     * otherwise.
     *
     * When a custom filter is set, we conservatively assume it would match this operation.
     */
    bool shouldDBProfile() {
        // Profile level 2 should override any sample rate or slowms settings.
        if (_dbprofile >= 2)
            return true;

        if (_dbprofile <= 0)
            return false;

        if (CollectionCatalog::get(opCtx())->getDatabaseProfileSettings(getNSS().dbName()).filter)
            return true;

        return elapsedTimeExcludingPauses() >= Milliseconds{serverGlobalParams.slowMS.load()};
    }

    /**
     * Raises the profiling level for this operation to "dbProfileLevel" if it was previously
     * less than "dbProfileLevel".
     *
     * This belongs on OpDebug, and so does not have the _inlock suffix.
     */
    void raiseDbProfileLevel(int dbProfileLevel);

    int dbProfileLevel() const {
        return _dbprofile;
    }

    /**
     * Gets the network operation type. No lock is required if called by the thread executing
     * the operation, but the lock must be held if called from another thread.
     */
    NetworkOp getNetworkOp() const {
        return _networkOp;
    }

    /**
     * Gets the logical operation type. No lock is required if called by the thread executing
     * the operation, but the lock must be held if called from another thread.
     */
    LogicalOp getLogicalOp() const {
        return _logicalOp;
    }

    /**
     * Returns true if the current operation is known to be a command.
     */
    bool isCommand() const {
        return _isCommand;
    }

    //
    // Methods for getting/setting elapsed time. Note that the observed elapsed time may be
    // negative, if the system time has been reset during the course of this operation.
    //

    void ensureStarted() {
        (void)startTime();
    }
    bool isStarted() const {
        return _start.load() != 0;
    }
    void done();
    bool isDone() const {
        return _end > 0;
    }
    bool isPaused() {
        return _lastPauseTime != 0;
    }

    /**
     * Stops the operation latency timer from "ticking". Time spent paused is not included in the
     * latencies returned by elapsedTimeExcludingPauses().
     *
     * Illegal to call if either the CurOp has not been started, or the CurOp is already in a paused
     * state.
     */
    void pauseTimer() {
        invariant(isStarted());
        invariant(_lastPauseTime == 0);
        _lastPauseTime = _tickSource->getTicks();
    }

    /**
     * Starts the operation latency timer "ticking" again. Illegal to call if the CurOp has not been
     * started and then subsequently paused.
     */
    void resumeTimer() {
        invariant(isStarted());
        invariant(_lastPauseTime > 0);
        _totalPausedDuration +=
            _tickSource->ticksTo<Microseconds>(_tickSource->getTicks() - _lastPauseTime);
        _lastPauseTime = 0;
    }

    /**
     * Ensures that remoteOpWait will be recorded in the OpDebug.
     *
     * This method is separate from startRemoteOpWait because operation types that do record
     * remoteOpWait, such as a getMore of a sharded aggregation, should always include the
     * remoteOpWait field even if its value is zero. An operation should call
     * ensureRecordRemoteOpWait() to declare that it wants to report remoteOpWait, and call
     * startRemoteOpWaitTimer()/stopRemoteOpWaitTimer() to measure the time.
     *
     * This timer uses the same clock source as elapsedTimeTotal().
     */
    void ensureRecordRemoteOpWait() {
        if (!_debug.remoteOpWaitTime) {
            _debug.remoteOpWaitTime.emplace(0);
        }
    }

    /**
     * Starts the remoteOpWait timer.
     *
     * Does nothing if ensureRecordRemoteOpWait() was not called or the current operation was not
     * marked as started.
     */
    void startRemoteOpWaitTimer() {
        // There are some commands that send remote operations but do not mark the current operation
        // as started. We do not record remote op wait time for those commands.
        if (!isStarted()) {
            return;
        }
        invariant(!isDone());
        invariant(!isPaused());
        invariant(!_remoteOpStartTime);
        if (_debug.remoteOpWaitTime) {
            _remoteOpStartTime.emplace(elapsedTimeTotal());
        }
    }

    /**
     * Stops the remoteOpWait timer.
     *
     * Does nothing if ensureRecordRemoteOpWait() was not called or the current operation was not
     * marked as started.
     */
    void stopRemoteOpWaitTimer() {
        // There are some commands that send remote operations but do not mark the current operation
        // as started. We do not record remote op wait time for those commands.
        if (!isStarted()) {
            return;
        }
        invariant(!isDone());
        invariant(!isPaused());
        if (_debug.remoteOpWaitTime) {
            Microseconds end = elapsedTimeTotal();
            invariant(_remoteOpStartTime);
            // On most systems a monotonic clock source will be used to measure time. When a
            // monotonic clock is not available we fallback to using the realtime system clock. When
            // used, a backward shift of the realtime system clock could lead to a negative delta.
            Microseconds delta = std::max((end - *_remoteOpStartTime), Microseconds{0});
            *_debug.remoteOpWaitTime += delta;
            _remoteOpStartTime = boost::none;
        }
        invariant(!_remoteOpStartTime);
    }

    /**
     * If this op has been marked as done(), returns the wall clock duration between being marked as
     * started with ensureStarted() and the call to done().
     *
     * Otherwise, returns the wall clock duration between the start time and now.
     *
     * If this op has not yet been started, returns 0.
     */
    Microseconds elapsedTimeTotal() {
        auto start = _start.load();
        if (start == 0) {
            return Microseconds{0};
        }

        return computeElapsedTimeTotal(start, _end.load());
    }

    /**
     * Returns the total elapsed duration minus any time spent in a paused state. See
     * elapsedTimeTotal() for the definition of the total duration and pause/resumeTimer() for
     * details on pausing.
     *
     * If this op has not yet been started, returns 0.
     *
     * Illegal to call while the timer is paused.
     */
    Microseconds elapsedTimeExcludingPauses() {
        invariant(!_lastPauseTime);

        auto start = _start.load();
        if (start == 0) {
            return Microseconds{0};
        }

        return computeElapsedTimeTotal(start, _end.load()) - _totalPausedDuration;
    }
    /**
    * The planningTimeMicros metric, reported in the system profiler and in queryStats, is measured
    * using the Curop instance's _tickSource. Currently, _tickSource is only paused in places where
    logical work is being done. If this were to change, and _tickSource
    were to be paused during query planning for reasons unrelated to the work of
    planning/optimization, it would break the planning time measurement below.
    *
    */
    void beginQueryPlanningTimer() {
        // This is an inner executor/cursor, the metrics for which don't get tracked by
        // OpDebug::planningTime.
        if (_queryPlanningStart.load() != 0) {
            return;
        }
        _queryPlanningStart = _tickSource->getTicks();
    }

    void stopQueryPlanningTimer() {
        // The planningTime metric is defined as being done once PrepareExecutionHelper::prepare()
        // is hit, which calls this function to stop the timer. As certain queries like $lookup
        // require inner cursors/executors that will follow this same codepath, it is important to
        // make sure the metric exclusively captures the time associated with the outermost cursor.
        // This is done by making sure planningTime has not already been set and that start has been
        // marked (as inner executors are prepared outside of the codepath that begins the planning
        // timer).
        auto start = _queryPlanningStart.load();
        if (debug().planningTime == Microseconds{0} && start != 0) {
            _queryPlanningEnd = _tickSource->getTicks();
            debug().planningTime = computeElapsedTimeTotal(start, _queryPlanningEnd.load());
        }
    }

    /**
     * Starts the waitForWriteConcern timer.
     *
     * The timer must be ended before it can be started again.
     */
    void beginWaitForWriteConcernTimer() {
        invariant(_waitForWriteConcernStart.load() == 0);
        _waitForWriteConcernStart = _tickSource->getTicks();
        _waitForWriteConcernEnd = 0;
    }

    /**
     * Stops the waitForWriteConcern timer.
     *
     * Does nothing if the timer has not been started.
     */
    void stopWaitForWriteConcernTimer() {
        auto start = _waitForWriteConcernStart.load();
        if (start != 0) {
            _waitForWriteConcernEnd = _tickSource->getTicks();
            auto duration = duration_cast<Milliseconds>(
                computeElapsedTimeTotal(start, _waitForWriteConcernEnd.load()));
            _atomicWaitForWriteConcernDurationMillis =
                _atomicWaitForWriteConcernDurationMillis.load() + duration;
            debug().waitForWriteConcernDurationMillis = _atomicWaitForWriteConcernDurationMillis;
            _waitForWriteConcernStart = 0;
        }
    }

    /**
     * If the platform supports the CPU timer, and we haven't collected this operation's CPU time
     * already, then calculates this operation's CPU time and stores it on the 'OpDebug'.
     */
    void calculateCpuTime();

    /**
     * 'opDescription' must be either an owned BSONObj or guaranteed to outlive the OperationContext
     * it is associated with.
     */
    void setOpDescription_inlock(const BSONObj& opDescription);

    /**
     * Sets the original command object.
     */
    void setOriginatingCommand_inlock(const BSONObj& commandObj) {
        _originatingCommand = commandObj.getOwned();
    }

    const Command* getCommand() const {
        return _command;
    }
    void setCommand_inlock(const Command* command) {
        _command = command;
    }

    /**
     * Returns whether the current operation is a read, write, or command.
     */
    Command::ReadWriteType getReadWriteType() const;

    /**
     * Appends information about this CurOp to "builder". If "truncateOps" is true, appends a string
     * summary of any objects which exceed the threshold size. If truncateOps is false, append the
     * entire object.
     *
     * If called from a thread other than the one executing the operation associated with this
     * CurOp, it is necessary to lock the associated Client object before executing this method.
     */
    void reportState(BSONObjBuilder* builder,
                     const SerializationContext& serializationContext,
                     bool truncateOps = false);

    /**
     * Sets the message for FailPoints used.
     */
    void setFailPointMessage_inlock(StringData message) {
        _failPointMessage = message.toString();
    }

    /**
     * Sets the message for this CurOp.
     */
    void setMessage_inlock(StringData message);

    /**
     * Sets the message and the progress meter for this CurOp.
     *
     * Accessors and modifiers of ProgressMeter associated with the CurOp must follow the same
     * locking scheme as CurOp. It is necessary to hold the lock while this method executes.
     */
    ProgressMeter& setProgress_inlock(StringData name,
                                      unsigned long long progressMeterTotal = 0,
                                      int secondsBetween = 3);

    /**
     * Captures stats on the locker after transaction resources are unstashed to the operation
     * context to be able to correctly ignore stats from outside this CurOp instance.
     */
    void updateStatsOnTransactionUnstash();

    /**
     * Captures stats on the locker that happened during this CurOp instance before transaction
     * resources are stashed. Also cleans up stats taken when transaction resources were unstashed.
     */
    void updateStatsOnTransactionStash();

    /*
     * Gets the message for FailPoints used.
     */
    const std::string& getFailPointMessage() const {
        return _failPointMessage;
    }

    /**
     * Gets the message for this CurOp.
     */
    const std::string& getMessage() const {
        return _message;
    }

    CurOp* parent() const {
        return _parent;
    }
    boost::optional<GenericCursor> getGenericCursor_inlock() const {
        return _genericCursor;
    }

    void yielded(int numYields = 1) {
        _numYields.fetchAndAdd(numYields);
    }

    /**
     * Returns the number of times yielded() was called.  Callers on threads other
     * than the one executing the operation must lock the client.
     */
    int numYields() const {
        return _numYields.load();
    }

    /**
     * this should be used very sparingly
     * generally the Context should set this up
     * but sometimes you want to do it ahead of time
     */
    void setNS_inlock(NamespaceString nss);
    void setNS_inlock(const DatabaseName& dbName);

    StringData getPlanSummary() const {
        return _planSummary;
    }

    void setPlanSummary_inlock(StringData summary) {
        _planSummary = summary.toString();
    }

    void setPlanSummary_inlock(std::string summary) {
        _planSummary = std::move(summary);
    }

    void setGenericCursor_inlock(GenericCursor gc);

    boost::optional<SingleThreadedLockStats> getLockStatsBase() const {
        return _lockStatsBase;
    }

    void setTickSource_forTest(TickSource* tickSource) {
        _tickSource = tickSource;
    }

    void setShouldOmitDiagnosticInformation_inlock(WithLock, bool shouldOmitDiagnosticInfo) {
        _shouldOmitDiagnosticInformation = shouldOmitDiagnosticInfo;
    }
    bool getShouldOmitDiagnosticInformation() const {
        return _shouldOmitDiagnosticInformation;
    }

    void setWaitingForIngressAdmission(WithLock, bool waiting) {
        _waitingForIngressAdmission = waiting;
    }

private:
    class CurOpStack;

    /**
     * Gets the OperationContext associated with this CurOp.
     * This must only be called after the CurOp has been pushed to an OperationContext's CurOpStack.
     */
    OperationContext* opCtx();

    TickSource::Tick startTime();
    Microseconds computeElapsedTimeTotal(TickSource::Tick startTime,
                                         TickSource::Tick endTime) const;

    Milliseconds _sumBlockedTimeTotal();

    /**
     * Handles failpoints that check whether a command has completed or not.
     * Used for testing purposes instead of the getLog command.
     */
    void _checkForFailpointsAfterCommandLogged();

    static const OperationContext::Decoration<CurOpStack> _curopStack;

    // The stack containing this CurOp instance.
    // This is set when this instance is pushed to the stack.
    CurOpStack* _stack{nullptr};

    // The CurOp beneath this CurOp instance in its stack, if any.
    // This is set when this instance is pushed to a non-empty stack.
    CurOp* _parent{nullptr};

    const Command* _command{nullptr};

    // The time at which this CurOp instance was marked as started.
    std::atomic<TickSource::Tick> _start{0};  // NOLINT

    // The time at which this CurOp instance was marked as done or 0 if the CurOp is not yet done.
    std::atomic<TickSource::Tick> _end{0};  // NOLINT

    // This CPU timer tracks the CPU time spent for this operation. Will be nullptr on unsupported
    // platforms.
    std::unique_ptr<OperationCPUTimer> _cpuTimer;

    // The time at which this CurOp instance had its timer paused, or 0 if the timer is not
    // currently paused.
    TickSource::Tick _lastPauseTime{0};

    // The cumulative duration for which the timer has been paused.
    Microseconds _totalPausedDuration{0};

    // The elapsedTimeTotal() value at which the remoteOpWait timer was started, or empty if the
    // remoteOpWait timer is not currently running.
    boost::optional<Microseconds> _remoteOpStartTime;

    // _networkOp represents the network-level op code: OP_QUERY, OP_GET_MORE, OP_MSG, etc.
    NetworkOp _networkOp{opInvalid};  // only set this through setNetworkOp_inlock() to keep synced
    // _logicalOp is the logical operation type, ie 'dbQuery' regardless of whether this is an
    // OP_QUERY find, a find command using OP_QUERY, or a find command using OP_MSG.
    // Similarly, the return value will be dbGetMore for both OP_GET_MORE and getMore command.
    LogicalOp _logicalOp{LogicalOp::opInvalid};  // only set this through setNetworkOp_inlock()

    bool _isCommand{false};
    int _dbprofile{0};  // 0=off, 1=slow, 2=all
    NamespaceString _nss;
    BSONObj _opDescription;
    BSONObj _originatingCommand;  // Used by getMore to display original command.
    OpDebug _debug;
    std::string _failPointMessage;  // Used to store FailPoint information.
    std::string _message;
    boost::optional<ProgressMeter> _progressMeter;
    AtomicWord<int> _numYields{0};
    // A GenericCursor containing information about the active cursor for a getMore operation.
    boost::optional<GenericCursor> _genericCursor;

    std::string _planSummary;

    // The lock stats being reported on the locker that accrued outside of this operation. This
    // includes the snapshot of lock stats taken when this CurOp instance is pushed to a CurOpStack
    // or the snapshot of lock stats taken when transaction resources are unstashed to this
    // operation context.
    boost::optional<SingleThreadedLockStats> _lockStatsBase;

    // The snapshot of lock stats taken when transaction resources are stashed. This captures the
    // locker activity that happened on this operation before the locker is released back to
    // transaction resources.
    boost::optional<SingleThreadedLockStats> _lockStatsOnceStashed;

    // The ticket wait times being reported on the locker that accrued outside of this operation.
    // This includes ticket wait times already accrued when the CurOp instance is pushed to a
    // CurOpStack or ticket wait times on locker when transaction resources are unstashed to this
    // operation context.
    Microseconds _ticketWaitBase{0};

    // The ticket wait times that accrued during this operation captured before the locker is
    // released back to transaction resources and stashed.
    Microseconds _ticketWaitWhenStashed{0};

    SharedUserAcquisitionStats _userAcquisitionStats{std::make_shared<UserAcquisitionStats>()};

    TickSource* _tickSource = globalSystemTickSource();
    // These values are used to calculate the amount of time spent planning a query.
    std::atomic<TickSource::Tick> _queryPlanningStart{0};  // NOLINT
    std::atomic<TickSource::Tick> _queryPlanningEnd{0};    // NOLINT

    // These values are used to calculate the amount of time spent waiting for write concern.
    std::atomic<TickSource::Tick> _waitForWriteConcernStart{0};  // NOLINT
    std::atomic<TickSource::Tick> _waitForWriteConcernEnd{0};    // NOLINT
    // This metric is the same value as debug().waitForWriteConcernDurationMillis.
    // We cannot use std::atomic in OpDebug since it is not copy assignable, but using a non-atomic
    // allows for a data race between stopWaitForWriteConcernTimer and curop::reportState.
    std::atomic<Milliseconds> _atomicWaitForWriteConcernDurationMillis{Milliseconds{0}};  // NOLINT

    // True if waiting for ingress admission ticket
    bool _waitingForIngressAdmission{false};

    // Flag to decide if diagnostic information should be omitted.
    bool _shouldOmitDiagnosticInformation{false};

    // TODO SERVER-87201: Remove need to zero out blocked time prior to operation starting.
    Milliseconds _blockedTimeAtStart{0};
};

}  // namespace mongo
