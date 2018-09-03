/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr.action;

import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.support.TransportActions;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.transport.NetworkExceptionHelper;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.translog.Translog;
import org.elasticsearch.persistent.AllocatedPersistentTask;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.xpack.ccr.action.bulk.BulkShardOperationsResponse;

import java.io.IOException;
import java.util.AbstractMap;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.NavigableMap;
import java.util.Objects;
import java.util.PriorityQueue;
import java.util.Queue;
import java.util.TreeMap;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import java.util.function.LongConsumer;
import java.util.function.LongSupplier;
import java.util.stream.Collectors;

/**
 * The node task that fetch the write operations from a leader shard and
 * persists these ops in the follower shard.
 */
public abstract class ShardFollowNodeTask extends AllocatedPersistentTask {

    public static final int DEFAULT_MAX_BATCH_OPERATION_COUNT = 1024;
    public static final int DEFAULT_MAX_CONCURRENT_READ_BATCHES = 1;
    public static final int DEFAULT_MAX_CONCURRENT_WRITE_BATCHES = 1;
    public static final int DEFAULT_MAX_WRITE_BUFFER_SIZE = 10240;
    public static final long DEFAULT_MAX_BATCH_SIZE_IN_BYTES = Long.MAX_VALUE;
    private static final int RETRY_LIMIT = 10;
    public static final TimeValue DEFAULT_RETRY_TIMEOUT = new TimeValue(500);
    public static final TimeValue DEFAULT_IDLE_SHARD_RETRY_DELAY = TimeValue.timeValueSeconds(10);

    private static final Logger LOGGER = Loggers.getLogger(ShardFollowNodeTask.class);

    private final String leaderIndex;
    private final ShardFollowTask params;
    private final TimeValue retryTimeout;
    private final TimeValue idleShardChangesRequestDelay;
    private final BiConsumer<TimeValue, Runnable> scheduler;
    private final LongSupplier relativeTimeProvider;

    private long leaderGlobalCheckpoint;
    private long leaderMaxSeqNo;
    private long lastRequestedSeqNo;
    private long followerGlobalCheckpoint = 0;
    private long followerMaxSeqNo = 0;
    private int numConcurrentReads = 0;
    private int numConcurrentWrites = 0;
    private long currentMappingVersion = 0;
    private long totalFetchTimeMillis = 0;
    private long numberOfSuccessfulFetches = 0;
    private long numberOfFailedFetches = 0;
    private long operationsReceived = 0;
    private long totalTransferredBytes = 0;
    private long totalIndexTimeMillis = 0;
    private long numberOfSuccessfulBulkOperations = 0;
    private long numberOfFailedBulkOperations = 0;
    private long numberOfOperationsIndexed = 0;
    private long lastFetchTime = -1;
    private final Queue<Translog.Operation> buffer = new PriorityQueue<>(Comparator.comparing(Translog.Operation::seqNo));
    private final LinkedHashMap<Long, ElasticsearchException> fetchExceptions;

    ShardFollowNodeTask(long id, String type, String action, String description, TaskId parentTask, Map<String, String> headers,
                        ShardFollowTask params, BiConsumer<TimeValue, Runnable> scheduler, final LongSupplier relativeTimeProvider) {
        super(id, type, action, description, parentTask, headers);
        this.params = params;
        this.scheduler = scheduler;
        this.relativeTimeProvider = relativeTimeProvider;
        this.retryTimeout = params.getRetryTimeout();
        this.idleShardChangesRequestDelay = params.getIdleShardRetryDelay();
        /*
         * We keep track of the most recent fetch exceptions, with the number of exceptions that we track equal to the maximum number of
         * concurrent fetches. For each failed fetch, we track the from sequence number associated with the request, and we clear the entry
         * when the fetch task associated with that from sequence number succeeds.
         */
        this.fetchExceptions = new LinkedHashMap<Long, ElasticsearchException>() {
            @Override
            protected boolean removeEldestEntry(final Map.Entry<Long, ElasticsearchException> eldest) {
                return size() > params.getMaxConcurrentReadBatches();
            }
        };

        if (params.getLeaderClusterAlias() != null) {
            leaderIndex = params.getLeaderClusterAlias() + ":" + params.getLeaderShardId().getIndexName();
        } else {
            leaderIndex = params.getLeaderShardId().getIndexName();
        }
    }

    void start(
            final long leaderGlobalCheckpoint,
            final long leaderMaxSeqNo,
            final long followerGlobalCheckpoint,
            final long followerMaxSeqNo) {
        /*
         * While this should only ever be called once and before any other threads can touch these fields, we use synchronization here to
         * avoid the need to declare these fields as volatile. That is, we are ensuring thesefields are always accessed under the same lock.
         */
        synchronized (this) {
            this.leaderGlobalCheckpoint = leaderGlobalCheckpoint;
            this.leaderMaxSeqNo = leaderMaxSeqNo;
            this.followerGlobalCheckpoint = followerGlobalCheckpoint;
            this.followerMaxSeqNo = followerMaxSeqNo;
            this.lastRequestedSeqNo = followerGlobalCheckpoint;
        }

        // updates follower mapping, this gets us the leader mapping version and makes sure that leader and follower mapping are identical
        updateMapping(mappingVersion -> {
            synchronized (ShardFollowNodeTask.this) {
                currentMappingVersion = mappingVersion;
            }
            LOGGER.info("{} Started to follow leader shard {}, followGlobalCheckPoint={}, mappingVersion={}",
                params.getFollowShardId(), params.getLeaderShardId(), followerGlobalCheckpoint, mappingVersion);
            coordinateReads();
        });
    }

    synchronized void coordinateReads() {
        if (isStopped()) {
            LOGGER.info("{} shard follow task has been stopped", params.getFollowShardId());
            return;
        }

        LOGGER.trace("{} coordinate reads, lastRequestedSeqNo={}, leaderGlobalCheckpoint={}",
            params.getFollowShardId(), lastRequestedSeqNo, leaderGlobalCheckpoint);
        final int maxBatchOperationCount = params.getMaxBatchOperationCount();
        while (hasReadBudget() && lastRequestedSeqNo < leaderGlobalCheckpoint) {
            final long from = lastRequestedSeqNo + 1;
            final long maxRequiredSeqNo = Math.min(leaderGlobalCheckpoint, from + maxBatchOperationCount - 1);
            final int requestBatchCount;
            if (numConcurrentReads == 0) {
                // This is the only request, we can optimistically fetch more documents if possible but not enforce max_required_seqno.
                requestBatchCount = maxBatchOperationCount;
            } else {
                requestBatchCount = Math.toIntExact(maxRequiredSeqNo - from + 1);
            }
            assert 0 < requestBatchCount && requestBatchCount <= maxBatchOperationCount : "request_batch_count=" + requestBatchCount;
            LOGGER.trace("{}[{} ongoing reads] read from_seqno={} max_required_seqno={} batch_count={}",
                params.getFollowShardId(), numConcurrentReads, from, maxRequiredSeqNo, requestBatchCount);
            numConcurrentReads++;
            sendShardChangesRequest(from, requestBatchCount, maxRequiredSeqNo);
            lastRequestedSeqNo = maxRequiredSeqNo;
        }

        if (numConcurrentReads == 0 && hasReadBudget()) {
            assert lastRequestedSeqNo == leaderGlobalCheckpoint;
            // We sneak peek if there is any thing new in the leader.
            // If there is we will happily accept
            numConcurrentReads++;
            long from = lastRequestedSeqNo + 1;
            LOGGER.trace("{}[{}] peek read [{}]", params.getFollowShardId(), numConcurrentReads, from);
            sendShardChangesRequest(from, maxBatchOperationCount, lastRequestedSeqNo);
        }
    }

    private boolean hasReadBudget() {
        assert Thread.holdsLock(this);
        if (numConcurrentReads >= params.getMaxConcurrentReadBatches()) {
            LOGGER.trace("{} no new reads, maximum number of concurrent reads have been reached [{}]",
                params.getFollowShardId(), numConcurrentReads);
            return false;
        }
        if (buffer.size() > params.getMaxWriteBufferSize()) {
            LOGGER.trace("{} no new reads, buffer limit has been reached [{}]", params.getFollowShardId(), buffer.size());
            return false;
        }
        return true;
    }

    private synchronized void coordinateWrites() {
        if (isStopped()) {
            LOGGER.info("{} shard follow task has been stopped", params.getFollowShardId());
            return;
        }

        while (hasWriteBudget() && buffer.isEmpty() == false) {
            long sumEstimatedSize = 0L;
            int length = Math.min(params.getMaxBatchOperationCount(), buffer.size());
            List<Translog.Operation> ops = new ArrayList<>(length);
            for (int i = 0; i < length; i++) {
                Translog.Operation op = buffer.remove();
                ops.add(op);
                sumEstimatedSize += op.estimateSize();
                if (sumEstimatedSize > params.getMaxBatchSizeInBytes()) {
                    break;
                }
            }
            numConcurrentWrites++;
            LOGGER.trace("{}[{}] write [{}/{}] [{}]", params.getFollowShardId(), numConcurrentWrites, ops.get(0).seqNo(),
                ops.get(ops.size() - 1).seqNo(), ops.size());
            sendBulkShardOperationsRequest(ops);
        }
    }

    private boolean hasWriteBudget() {
        assert Thread.holdsLock(this);
        if (numConcurrentWrites >= params.getMaxConcurrentWriteBatches()) {
            LOGGER.trace("{} maximum number of concurrent writes have been reached [{}]",
                params.getFollowShardId(), numConcurrentWrites);
            return false;
        }
        return true;
    }

    private void sendShardChangesRequest(long from, int maxOperationCount, long maxRequiredSeqNo) {
        sendShardChangesRequest(from, maxOperationCount, maxRequiredSeqNo, new AtomicInteger(0));
    }

    private void sendShardChangesRequest(long from, int maxOperationCount, long maxRequiredSeqNo, AtomicInteger retryCounter) {
        final long startTime = relativeTimeProvider.getAsLong();
        synchronized (this) {
            lastFetchTime = startTime;
        }
        innerSendShardChangesRequest(from, maxOperationCount,
                response -> {
                    synchronized (ShardFollowNodeTask.this) {
                        totalFetchTimeMillis += TimeUnit.NANOSECONDS.toMillis(relativeTimeProvider.getAsLong() - startTime);
                        numberOfSuccessfulFetches++;
                        fetchExceptions.remove(from);
                        operationsReceived += response.getOperations().length;
                        totalTransferredBytes += Arrays.stream(response.getOperations()).mapToLong(Translog.Operation::estimateSize).sum();
                    }
                    handleReadResponse(from, maxRequiredSeqNo, response);
                },
                e -> {
                    synchronized (ShardFollowNodeTask.this) {
                        totalFetchTimeMillis += TimeUnit.NANOSECONDS.toMillis(relativeTimeProvider.getAsLong() - startTime);
                        numberOfFailedFetches++;
                        fetchExceptions.put(from, new ElasticsearchException(e));
                    }
                    handleFailure(e, retryCounter, () -> sendShardChangesRequest(from, maxOperationCount, maxRequiredSeqNo, retryCounter));
                });
    }

    void handleReadResponse(long from, long maxRequiredSeqNo, ShardChangesAction.Response response) {
        maybeUpdateMapping(response.getMappingVersion(), () -> innerHandleReadResponse(from, maxRequiredSeqNo, response));
    }

    /** Called when some operations are fetched from the leading */
    protected void onOperationsFetched(Translog.Operation[] operations) {

    }

    synchronized void innerHandleReadResponse(long from, long maxRequiredSeqNo, ShardChangesAction.Response response) {
        onOperationsFetched(response.getOperations());
        leaderGlobalCheckpoint = Math.max(leaderGlobalCheckpoint, response.getGlobalCheckpoint());
        leaderMaxSeqNo = Math.max(leaderMaxSeqNo, response.getMaxSeqNo());
        final long newFromSeqNo;
        if (response.getOperations().length == 0) {
            newFromSeqNo = from;
        } else {
            assert response.getOperations()[0].seqNo() == from :
                "first operation is not what we asked for. From is [" + from + "], got " + response.getOperations()[0];
            buffer.addAll(Arrays.asList(response.getOperations()));
            final long maxSeqNo = response.getOperations()[response.getOperations().length - 1].seqNo();
            assert maxSeqNo ==
                Arrays.stream(response.getOperations()).mapToLong(Translog.Operation::seqNo).max().getAsLong();
            newFromSeqNo = maxSeqNo + 1;
            // update last requested seq no as we may have gotten more than we asked for and we don't want to ask it again.
            lastRequestedSeqNo = Math.max(lastRequestedSeqNo, maxSeqNo);
            assert lastRequestedSeqNo <= leaderGlobalCheckpoint :  "lastRequestedSeqNo [" + lastRequestedSeqNo +
                "] is larger than the global checkpoint [" + leaderGlobalCheckpoint + "]";
            coordinateWrites();
        }
        if (newFromSeqNo <= maxRequiredSeqNo && isStopped() == false) {
            int newSize = Math.toIntExact(maxRequiredSeqNo - newFromSeqNo + 1);
            LOGGER.trace("{} received [{}] ops, still missing [{}/{}], continuing to read...",
                params.getFollowShardId(), response.getOperations().length, newFromSeqNo, maxRequiredSeqNo);
            sendShardChangesRequest(newFromSeqNo, newSize, maxRequiredSeqNo);
        } else {
            // read is completed, decrement
            numConcurrentReads--;
            if (response.getOperations().length == 0 && leaderGlobalCheckpoint == lastRequestedSeqNo)  {
                // we got nothing and we have no reason to believe asking again well get us more, treat shard as idle and delay
                // future requests
                LOGGER.trace("{} received no ops and no known ops to fetch, scheduling to coordinate reads",
                    params.getFollowShardId());
                scheduler.accept(idleShardChangesRequestDelay, this::coordinateReads);
            } else {
                coordinateReads();
            }
        }
    }

    private void sendBulkShardOperationsRequest(List<Translog.Operation> operations) {
        sendBulkShardOperationsRequest(operations, new AtomicInteger(0));
    }

    private void sendBulkShardOperationsRequest(List<Translog.Operation> operations, AtomicInteger retryCounter) {
        final long startTime = relativeTimeProvider.getAsLong();
        innerSendBulkShardOperationsRequest(operations,
                response -> {
                    synchronized (ShardFollowNodeTask.this) {
                        totalIndexTimeMillis += TimeUnit.NANOSECONDS.toMillis(relativeTimeProvider.getAsLong() - startTime);
                        numberOfSuccessfulBulkOperations++;
                        numberOfOperationsIndexed += operations.size();
                    }
                    handleWriteResponse(response);
                },
                e -> {
                    synchronized (ShardFollowNodeTask.this) {
                        totalIndexTimeMillis += TimeUnit.NANOSECONDS.toMillis(relativeTimeProvider.getAsLong() - startTime);
                        numberOfFailedBulkOperations++;
                    }
                    handleFailure(e, retryCounter, () -> sendBulkShardOperationsRequest(operations, retryCounter));
                }
        );
    }

    private synchronized void handleWriteResponse(final BulkShardOperationsResponse response) {
        this.followerGlobalCheckpoint = Math.max(this.followerGlobalCheckpoint, response.getGlobalCheckpoint());
        this.followerMaxSeqNo = Math.max(this.followerMaxSeqNo, response.getMaxSeqNo());
        numConcurrentWrites--;
        assert numConcurrentWrites >= 0;
        coordinateWrites();

        // In case that buffer has more ops than is allowed then reads may all have been stopped,
        // this invocation makes sure that we start a read when there is budget in case no reads are being performed.
        coordinateReads();
    }

    private synchronized void maybeUpdateMapping(Long minimumRequiredMappingVersion, Runnable task) {
        if (currentMappingVersion >= minimumRequiredMappingVersion) {
            LOGGER.trace("{} mapping version [{}] is higher or equal than minimum required mapping version [{}]",
                params.getFollowShardId(), currentMappingVersion, minimumRequiredMappingVersion);
            task.run();
        } else {
            LOGGER.trace("{} updating mapping, mapping version [{}] is lower than minimum required mapping version [{}]",
                params.getFollowShardId(), currentMappingVersion, minimumRequiredMappingVersion);
            updateMapping(mappingVersion -> {
                currentMappingVersion = mappingVersion;
                task.run();
            });
        }
    }

    private void updateMapping(LongConsumer handler) {
        updateMapping(handler, new AtomicInteger(0));
    }

    private void updateMapping(LongConsumer handler, AtomicInteger retryCounter) {
        innerUpdateMapping(handler, e -> handleFailure(e, retryCounter, () -> updateMapping(handler, retryCounter)));
    }

    private void handleFailure(Exception e, AtomicInteger retryCounter, Runnable task) {
        assert e != null;
        if (shouldRetry(e)) {
            if (isStopped() == false && retryCounter.incrementAndGet() <= RETRY_LIMIT) {
                LOGGER.debug(new ParameterizedMessage("{} error during follow shard task, retrying...", params.getFollowShardId()), e);
                scheduler.accept(retryTimeout, task);
            } else {
                markAsFailed(new ElasticsearchException("retrying failed [" + retryCounter.get() +
                    "] times, aborting...", e));
            }
        } else {
            markAsFailed(e);
        }
    }

    private boolean shouldRetry(Exception e) {
        return NetworkExceptionHelper.isConnectException(e) ||
            NetworkExceptionHelper.isCloseConnectionException(e) ||
            TransportActions.isShardNotAvailableException(e);
    }

    // These methods are protected for testing purposes:
    protected abstract void innerUpdateMapping(LongConsumer handler, Consumer<Exception> errorHandler);

    protected abstract void innerSendBulkShardOperationsRequest(
            List<Translog.Operation> operations, Consumer<BulkShardOperationsResponse> handler, Consumer<Exception> errorHandler);

    protected abstract void innerSendShardChangesRequest(long from, int maxOperationCount, Consumer<ShardChangesAction.Response> handler,
                                                         Consumer<Exception> errorHandler);

    @Override
    protected void onCancelled() {
        markAsCompleted();
    }

    protected boolean isStopped() {
        return isCancelled() || isCompleted();
    }

    public ShardId getFollowShardId() {
        return params.getFollowShardId();
    }

    @Override
    public synchronized Status getStatus() {
        final long timeSinceLastFetchMillis;
        if (lastFetchTime != -1) {
             timeSinceLastFetchMillis = TimeUnit.NANOSECONDS.toMillis(relativeTimeProvider.getAsLong() - lastFetchTime);
        } else {
            // To avoid confusion when ccr didn't yet execute a fetch:
            timeSinceLastFetchMillis = -1;
        }
        return new Status(
                leaderIndex,
                getFollowShardId().getId(),
                leaderGlobalCheckpoint,
                leaderMaxSeqNo,
                followerGlobalCheckpoint,
                followerMaxSeqNo,
                lastRequestedSeqNo,
                numConcurrentReads,
                numConcurrentWrites,
                buffer.size(),
                currentMappingVersion,
                totalFetchTimeMillis,
                numberOfSuccessfulFetches,
                numberOfFailedFetches,
                operationsReceived,
                totalTransferredBytes,
                totalIndexTimeMillis,
                numberOfSuccessfulBulkOperations,
                numberOfFailedBulkOperations,
                numberOfOperationsIndexed,
                new TreeMap<>(fetchExceptions),
                timeSinceLastFetchMillis);
    }

    public static class Status implements Task.Status {

        public static final String STATUS_PARSER_NAME = "shard-follow-node-task-status";

        static final ParseField LEADER_INDEX = new ParseField("leader_index");
        static final ParseField SHARD_ID = new ParseField("shard_id");
        static final ParseField LEADER_GLOBAL_CHECKPOINT_FIELD = new ParseField("leader_global_checkpoint");
        static final ParseField LEADER_MAX_SEQ_NO_FIELD = new ParseField("leader_max_seq_no");
        static final ParseField FOLLOWER_GLOBAL_CHECKPOINT_FIELD = new ParseField("follower_global_checkpoint");
        static final ParseField FOLLOWER_MAX_SEQ_NO_FIELD = new ParseField("follower_max_seq_no");
        static final ParseField LAST_REQUESTED_SEQ_NO_FIELD = new ParseField("last_requested_seq_no");
        static final ParseField NUMBER_OF_CONCURRENT_READS_FIELD = new ParseField("number_of_concurrent_reads");
        static final ParseField NUMBER_OF_CONCURRENT_WRITES_FIELD = new ParseField("number_of_concurrent_writes");
        static final ParseField NUMBER_OF_QUEUED_WRITES_FIELD = new ParseField("number_of_queued_writes");
        static final ParseField MAPPING_VERSION_FIELD = new ParseField("mapping_version");
        static final ParseField TOTAL_FETCH_TIME_MILLIS_FIELD = new ParseField("total_fetch_time_millis");
        static final ParseField NUMBER_OF_SUCCESSFUL_FETCHES_FIELD = new ParseField("number_of_successful_fetches");
        static final ParseField NUMBER_OF_FAILED_FETCHES_FIELD = new ParseField("number_of_failed_fetches");
        static final ParseField OPERATIONS_RECEIVED_FIELD = new ParseField("operations_received");
        static final ParseField TOTAL_TRANSFERRED_BYTES = new ParseField("total_transferred_bytes");
        static final ParseField TOTAL_INDEX_TIME_MILLIS_FIELD = new ParseField("total_index_time_millis");
        static final ParseField NUMBER_OF_SUCCESSFUL_BULK_OPERATIONS_FIELD = new ParseField("number_of_successful_bulk_operations");
        static final ParseField NUMBER_OF_FAILED_BULK_OPERATIONS_FIELD = new ParseField("number_of_failed_bulk_operations");
        static final ParseField NUMBER_OF_OPERATIONS_INDEXED_FIELD = new ParseField("number_of_operations_indexed");
        static final ParseField FETCH_EXCEPTIONS = new ParseField("fetch_exceptions");
        static final ParseField TIME_SINCE_LAST_FETCH_MILLIS_FIELD = new ParseField("time_since_last_fetch_millis");

        @SuppressWarnings("unchecked")
        static final ConstructingObjectParser<Status, Void> STATUS_PARSER = new ConstructingObjectParser<>(STATUS_PARSER_NAME,
            args -> new Status(
                    (String) args[0],
                    (int) args[1],
                    (long) args[2],
                    (long) args[3],
                    (long) args[4],
                    (long) args[5],
                    (long) args[6],
                    (int) args[7],
                    (int) args[8],
                    (int) args[9],
                    (long) args[10],
                    (long) args[11],
                    (long) args[12],
                    (long) args[13],
                    (long) args[14],
                    (long) args[15],
                    (long) args[16],
                    (long) args[17],
                    (long) args[18],
                    (long) args[19],
                    new TreeMap<>(
                            ((List<Map.Entry<Long, ElasticsearchException>>) args[20])
                                    .stream()
                                    .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue))),
                    (long) args[21]));

        public static final String FETCH_EXCEPTIONS_ENTRY_PARSER_NAME = "shard-follow-node-task-status-fetch-exceptions-entry";

        static final ConstructingObjectParser<Map.Entry<Long, ElasticsearchException>, Void> FETCH_EXCEPTIONS_ENTRY_PARSER =
                new ConstructingObjectParser<>(
                        FETCH_EXCEPTIONS_ENTRY_PARSER_NAME,
                        args -> new AbstractMap.SimpleEntry<>((long) args[0], (ElasticsearchException) args[1]));

        static {
            STATUS_PARSER.declareString(ConstructingObjectParser.constructorArg(), LEADER_INDEX);
            STATUS_PARSER.declareInt(ConstructingObjectParser.constructorArg(), SHARD_ID);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), LEADER_GLOBAL_CHECKPOINT_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), LEADER_MAX_SEQ_NO_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), FOLLOWER_GLOBAL_CHECKPOINT_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), FOLLOWER_MAX_SEQ_NO_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), LAST_REQUESTED_SEQ_NO_FIELD);
            STATUS_PARSER.declareInt(ConstructingObjectParser.constructorArg(), NUMBER_OF_CONCURRENT_READS_FIELD);
            STATUS_PARSER.declareInt(ConstructingObjectParser.constructorArg(), NUMBER_OF_CONCURRENT_WRITES_FIELD);
            STATUS_PARSER.declareInt(ConstructingObjectParser.constructorArg(), NUMBER_OF_QUEUED_WRITES_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), MAPPING_VERSION_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), TOTAL_FETCH_TIME_MILLIS_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), NUMBER_OF_SUCCESSFUL_FETCHES_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), NUMBER_OF_FAILED_FETCHES_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), OPERATIONS_RECEIVED_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), TOTAL_TRANSFERRED_BYTES);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), TOTAL_INDEX_TIME_MILLIS_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), NUMBER_OF_SUCCESSFUL_BULK_OPERATIONS_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), NUMBER_OF_FAILED_BULK_OPERATIONS_FIELD);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), NUMBER_OF_OPERATIONS_INDEXED_FIELD);
            STATUS_PARSER.declareObjectArray(ConstructingObjectParser.constructorArg(), FETCH_EXCEPTIONS_ENTRY_PARSER, FETCH_EXCEPTIONS);
            STATUS_PARSER.declareLong(ConstructingObjectParser.constructorArg(), TIME_SINCE_LAST_FETCH_MILLIS_FIELD);
        }

        static final ParseField FETCH_EXCEPTIONS_ENTRY_FROM_SEQ_NO = new ParseField("from_seq_no");
        static final ParseField FETCH_EXCEPTIONS_ENTRY_EXCEPTION = new ParseField("exception");

        static {
            FETCH_EXCEPTIONS_ENTRY_PARSER.declareLong(ConstructingObjectParser.constructorArg(), FETCH_EXCEPTIONS_ENTRY_FROM_SEQ_NO);
            FETCH_EXCEPTIONS_ENTRY_PARSER.declareObject(
                    ConstructingObjectParser.constructorArg(),
                    (p, c) -> ElasticsearchException.fromXContent(p),
                    FETCH_EXCEPTIONS_ENTRY_EXCEPTION);
        }

        private final String leaderIndex;

        public String leaderIndex() {
            return leaderIndex;
        }

        private final int shardId;

        public int getShardId() {
            return shardId;
        }

        private final long leaderGlobalCheckpoint;

        public long leaderGlobalCheckpoint() {
            return leaderGlobalCheckpoint;
        }

        private final long leaderMaxSeqNo;

        public long leaderMaxSeqNo() {
            return leaderMaxSeqNo;
        }

        private final long followerGlobalCheckpoint;

        public long followerGlobalCheckpoint() {
            return followerGlobalCheckpoint;
        }

        private final long followerMaxSeqNo;

        public long followerMaxSeqNo() {
            return followerMaxSeqNo;
        }

        private final long lastRequestedSeqNo;

        public long lastRequestedSeqNo() {
            return lastRequestedSeqNo;
        }

        private final int numberOfConcurrentReads;

        public int numberOfConcurrentReads() {
            return numberOfConcurrentReads;
        }

        private final int numberOfConcurrentWrites;

        public int numberOfConcurrentWrites() {
            return numberOfConcurrentWrites;
        }

        private final int numberOfQueuedWrites;

        public int numberOfQueuedWrites() {
            return numberOfQueuedWrites;
        }

        private final long mappingVersion;

        public long mappingVersion() {
            return mappingVersion;
        }

        private final long totalFetchTimeMillis;

        public long totalFetchTimeMillis() {
            return totalFetchTimeMillis;
        }

        private final long numberOfSuccessfulFetches;

        public long numberOfSuccessfulFetches() {
            return numberOfSuccessfulFetches;
        }

        private final long numberOfFailedFetches;

        public long numberOfFailedFetches() {
            return numberOfFailedFetches;
        }

        private final long operationsReceived;

        public long operationsReceived() {
            return operationsReceived;
        }

        private final long totalTransferredBytes;

        public long totalTransferredBytes() {
            return totalTransferredBytes;
        }

        private final long totalIndexTimeMillis;

        public long totalIndexTimeMillis() {
            return totalIndexTimeMillis;
        }

        private final long numberOfSuccessfulBulkOperations;

        public long numberOfSuccessfulBulkOperations() {
            return numberOfSuccessfulBulkOperations;
        }

        private final long numberOfFailedBulkOperations;

        public long numberOfFailedBulkOperations() {
            return numberOfFailedBulkOperations;
        }

        private final long numberOfOperationsIndexed;

        public long numberOfOperationsIndexed() {
            return numberOfOperationsIndexed;
        }

        private final NavigableMap<Long, ElasticsearchException> fetchExceptions;

        public NavigableMap<Long, ElasticsearchException> fetchExceptions() {
            return fetchExceptions;
        }

        private final long timeSinceLastFetchMillis;

        public long timeSinceLastFetchMillis() {
            return timeSinceLastFetchMillis;
        }

        Status(
                final String leaderIndex,
                final int shardId,
                final long leaderGlobalCheckpoint,
                final long leaderMaxSeqNo,
                final long followerGlobalCheckpoint,
                final long followerMaxSeqNo,
                final long lastRequestedSeqNo,
                final int numberOfConcurrentReads,
                final int numberOfConcurrentWrites,
                final int numberOfQueuedWrites,
                final long mappingVersion,
                final long totalFetchTimeMillis,
                final long numberOfSuccessfulFetches,
                final long numberOfFailedFetches,
                final long operationsReceived,
                final long totalTransferredBytes,
                final long totalIndexTimeMillis,
                final long numberOfSuccessfulBulkOperations,
                final long numberOfFailedBulkOperations,
                final long numberOfOperationsIndexed,
                final NavigableMap<Long, ElasticsearchException> fetchExceptions,
                final long timeSinceLastFetchMillis) {
            this.leaderIndex = leaderIndex;
            this.shardId = shardId;
            this.leaderGlobalCheckpoint = leaderGlobalCheckpoint;
            this.leaderMaxSeqNo = leaderMaxSeqNo;
            this.followerGlobalCheckpoint = followerGlobalCheckpoint;
            this.followerMaxSeqNo = followerMaxSeqNo;
            this.lastRequestedSeqNo = lastRequestedSeqNo;
            this.numberOfConcurrentReads = numberOfConcurrentReads;
            this.numberOfConcurrentWrites = numberOfConcurrentWrites;
            this.numberOfQueuedWrites = numberOfQueuedWrites;
            this.mappingVersion = mappingVersion;
            this.totalFetchTimeMillis = totalFetchTimeMillis;
            this.numberOfSuccessfulFetches = numberOfSuccessfulFetches;
            this.numberOfFailedFetches = numberOfFailedFetches;
            this.operationsReceived = operationsReceived;
            this.totalTransferredBytes = totalTransferredBytes;
            this.totalIndexTimeMillis = totalIndexTimeMillis;
            this.numberOfSuccessfulBulkOperations = numberOfSuccessfulBulkOperations;
            this.numberOfFailedBulkOperations = numberOfFailedBulkOperations;
            this.numberOfOperationsIndexed = numberOfOperationsIndexed;
            this.fetchExceptions = Objects.requireNonNull(fetchExceptions);
            this.timeSinceLastFetchMillis = timeSinceLastFetchMillis;
        }

        public Status(final StreamInput in) throws IOException {
            this.leaderIndex = in.readString();
            this.shardId = in.readVInt();
            this.leaderGlobalCheckpoint = in.readZLong();
            this.leaderMaxSeqNo = in.readZLong();
            this.followerGlobalCheckpoint = in.readZLong();
            this.followerMaxSeqNo = in.readZLong();
            this.lastRequestedSeqNo = in.readZLong();
            this.numberOfConcurrentReads = in.readVInt();
            this.numberOfConcurrentWrites = in.readVInt();
            this.numberOfQueuedWrites = in.readVInt();
            this.mappingVersion = in.readVLong();
            this.totalFetchTimeMillis = in.readVLong();
            this.numberOfSuccessfulFetches = in.readVLong();
            this.numberOfFailedFetches = in.readVLong();
            this.operationsReceived = in.readVLong();
            this.totalTransferredBytes = in.readVLong();
            this.totalIndexTimeMillis = in.readVLong();
            this.numberOfSuccessfulBulkOperations = in.readVLong();
            this.numberOfFailedBulkOperations = in.readVLong();
            this.numberOfOperationsIndexed = in.readVLong();
            this.fetchExceptions = new TreeMap<>(in.readMap(StreamInput::readVLong, StreamInput::readException));
            this.timeSinceLastFetchMillis = in.readZLong();
        }

        @Override
        public String getWriteableName() {
            return STATUS_PARSER_NAME;
        }

        @Override
        public void writeTo(final StreamOutput out) throws IOException {
            out.writeString(leaderIndex);
            out.writeVInt(shardId);
            out.writeZLong(leaderGlobalCheckpoint);
            out.writeZLong(leaderMaxSeqNo);
            out.writeZLong(followerGlobalCheckpoint);
            out.writeZLong(followerMaxSeqNo);
            out.writeZLong(lastRequestedSeqNo);
            out.writeVInt(numberOfConcurrentReads);
            out.writeVInt(numberOfConcurrentWrites);
            out.writeVInt(numberOfQueuedWrites);
            out.writeVLong(mappingVersion);
            out.writeVLong(totalFetchTimeMillis);
            out.writeVLong(numberOfSuccessfulFetches);
            out.writeVLong(numberOfFailedFetches);
            out.writeVLong(operationsReceived);
            out.writeVLong(totalTransferredBytes);
            out.writeVLong(totalIndexTimeMillis);
            out.writeVLong(numberOfSuccessfulBulkOperations);
            out.writeVLong(numberOfFailedBulkOperations);
            out.writeVLong(numberOfOperationsIndexed);
            out.writeMap(fetchExceptions, StreamOutput::writeVLong, StreamOutput::writeException);
            out.writeZLong(timeSinceLastFetchMillis);
        }

        @Override
        public XContentBuilder toXContent(final XContentBuilder builder, final Params params) throws IOException {
            builder.startObject();
            {
                builder.field(LEADER_INDEX.getPreferredName(), leaderIndex);
                builder.field(SHARD_ID.getPreferredName(), shardId);
                builder.field(LEADER_GLOBAL_CHECKPOINT_FIELD.getPreferredName(), leaderGlobalCheckpoint);
                builder.field(LEADER_MAX_SEQ_NO_FIELD.getPreferredName(), leaderMaxSeqNo);
                builder.field(FOLLOWER_GLOBAL_CHECKPOINT_FIELD.getPreferredName(), followerGlobalCheckpoint);
                builder.field(FOLLOWER_MAX_SEQ_NO_FIELD.getPreferredName(), followerMaxSeqNo);
                builder.field(LAST_REQUESTED_SEQ_NO_FIELD.getPreferredName(), lastRequestedSeqNo);
                builder.field(NUMBER_OF_CONCURRENT_READS_FIELD.getPreferredName(), numberOfConcurrentReads);
                builder.field(NUMBER_OF_CONCURRENT_WRITES_FIELD.getPreferredName(), numberOfConcurrentWrites);
                builder.field(NUMBER_OF_QUEUED_WRITES_FIELD.getPreferredName(), numberOfQueuedWrites);
                builder.field(MAPPING_VERSION_FIELD.getPreferredName(), mappingVersion);
                builder.humanReadableField(
                        TOTAL_FETCH_TIME_MILLIS_FIELD.getPreferredName(),
                        "total_fetch_time",
                        new TimeValue(totalFetchTimeMillis, TimeUnit.MILLISECONDS));
                builder.field(NUMBER_OF_SUCCESSFUL_FETCHES_FIELD.getPreferredName(), numberOfSuccessfulFetches);
                builder.field(NUMBER_OF_FAILED_FETCHES_FIELD.getPreferredName(), numberOfFailedFetches);
                builder.field(OPERATIONS_RECEIVED_FIELD.getPreferredName(), operationsReceived);
                builder.humanReadableField(
                        TOTAL_TRANSFERRED_BYTES.getPreferredName(),
                        "total_transferred",
                        new ByteSizeValue(totalTransferredBytes, ByteSizeUnit.BYTES));
                builder.humanReadableField(
                        TOTAL_INDEX_TIME_MILLIS_FIELD.getPreferredName(),
                        "total_index_time",
                        new TimeValue(totalIndexTimeMillis, TimeUnit.MILLISECONDS));
                builder.field(NUMBER_OF_SUCCESSFUL_BULK_OPERATIONS_FIELD.getPreferredName(), numberOfSuccessfulBulkOperations);
                builder.field(NUMBER_OF_FAILED_BULK_OPERATIONS_FIELD.getPreferredName(), numberOfFailedBulkOperations);
                builder.field(NUMBER_OF_OPERATIONS_INDEXED_FIELD.getPreferredName(), numberOfOperationsIndexed);
                builder.startArray(FETCH_EXCEPTIONS.getPreferredName());
                {
                    for (final Map.Entry<Long, ElasticsearchException> entry : fetchExceptions.entrySet()) {
                        builder.startObject();
                        {
                            builder.field(FETCH_EXCEPTIONS_ENTRY_FROM_SEQ_NO.getPreferredName(), entry.getKey());
                            builder.field(FETCH_EXCEPTIONS_ENTRY_EXCEPTION.getPreferredName());
                            builder.startObject();
                            {
                                ElasticsearchException.generateThrowableXContent(builder, params, entry.getValue());
                            }
                            builder.endObject();
                        }
                        builder.endObject();
                    }
                }
                builder.endArray();
                builder.humanReadableField(
                        TIME_SINCE_LAST_FETCH_MILLIS_FIELD.getPreferredName(),
                        "time_since_last_fetch",
                        new TimeValue(timeSinceLastFetchMillis, TimeUnit.MILLISECONDS));
            }
            builder.endObject();
            return builder;
        }

        public static Status fromXContent(final XContentParser parser) {
            return STATUS_PARSER.apply(parser, null);
        }

        @Override
        public boolean equals(final Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            final Status that = (Status) o;
            return leaderIndex.equals(that.leaderIndex) &&
                    shardId == that.shardId &&
                    leaderGlobalCheckpoint == that.leaderGlobalCheckpoint &&
                    leaderMaxSeqNo == that.leaderMaxSeqNo &&
                    followerGlobalCheckpoint == that.followerGlobalCheckpoint &&
                    followerMaxSeqNo == that.followerMaxSeqNo &&
                    lastRequestedSeqNo == that.lastRequestedSeqNo &&
                    numberOfConcurrentReads == that.numberOfConcurrentReads &&
                    numberOfConcurrentWrites == that.numberOfConcurrentWrites &&
                    numberOfQueuedWrites == that.numberOfQueuedWrites &&
                    mappingVersion == that.mappingVersion &&
                    totalFetchTimeMillis == that.totalFetchTimeMillis &&
                    numberOfSuccessfulFetches == that.numberOfSuccessfulFetches &&
                    numberOfFailedFetches == that.numberOfFailedFetches &&
                    operationsReceived == that.operationsReceived &&
                    totalTransferredBytes == that.totalTransferredBytes &&
                    numberOfSuccessfulBulkOperations == that.numberOfSuccessfulBulkOperations &&
                    numberOfFailedBulkOperations == that.numberOfFailedBulkOperations &&
                    numberOfOperationsIndexed == that.numberOfOperationsIndexed &&
                    /*
                     * ElasticsearchException does not implement equals so we will assume the fetch exceptions are equal if they are equal
                     * up to the key set and their messages.  Note that we are relying on the fact that the fetch exceptions are ordered by
                     * keys.
                     */
                    fetchExceptions.keySet().equals(that.fetchExceptions.keySet()) &&
                    getFetchExceptionMessages(this).equals(getFetchExceptionMessages(that)) &&
                    timeSinceLastFetchMillis == that.timeSinceLastFetchMillis;
        }

        @Override
        public int hashCode() {
            return Objects.hash(
                    leaderIndex,
                    shardId,
                    leaderGlobalCheckpoint,
                    leaderMaxSeqNo,
                    followerGlobalCheckpoint,
                    followerMaxSeqNo,
                    lastRequestedSeqNo,
                    numberOfConcurrentReads,
                    numberOfConcurrentWrites,
                    numberOfQueuedWrites,
                    mappingVersion,
                    totalFetchTimeMillis,
                    numberOfSuccessfulFetches,
                    numberOfFailedFetches,
                    operationsReceived,
                    totalTransferredBytes,
                    numberOfSuccessfulBulkOperations,
                    numberOfFailedBulkOperations,
                    numberOfOperationsIndexed,
                    /*
                     * ElasticsearchException does not implement hash code so we will compute the hash code based on the key set and the
                     * messages. Note that we are relying on the fact that the fetch exceptions are ordered by keys.
                     */
                    fetchExceptions.keySet(),
                    getFetchExceptionMessages(this),
                    timeSinceLastFetchMillis);
        }

        private static List<String> getFetchExceptionMessages(final Status status) {
            return status.fetchExceptions().values().stream().map(ElasticsearchException::getMessage).collect(Collectors.toList());
        }

        public String toString() {
            return Strings.toString(this);
        }

    }

}
