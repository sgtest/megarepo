/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.repositories.blobstore.testkit;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionListenerResponseHandler;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.StepListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.GroupedActionListener;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.support.ThreadedActionListener;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.blobstore.BlobContainer;
import org.elasticsearch.common.blobstore.BlobPath;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.InputStreamStreamInput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryVerificationException;
import org.elasticsearch.repositories.blobstore.BlobStoreRepository;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskAwareRequest;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportRequestOptions;
import org.elasticsearch.transport.TransportService;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.LongPredicate;
import java.util.stream.Collectors;

import static org.elasticsearch.repositories.blobstore.testkit.SnapshotRepositoryTestKit.humanReadableNanos;

/**
 * Action which instructs a node to write a blob to the blob store and verify that it can be read correctly by other nodes. The other nodes
 * may read the whole blob or just a range; we verify the data that is read by checksum using {@link GetBlobChecksumAction}.
 *
 * The other nodes may optionally be instructed to attempt their read just before the write completes (which may indicate that the blob is
 * not found but must not yield partial data), and may optionally overwrite the blob while the reads are ongoing (which may yield either
 * version of the blob, but again must not yield partial data). Usually, however, we write once and only read after the write completes, and
 * in this case we insist that the read succeeds.
 *
 *
 * <pre>
 *
 * +---------+                           +-------+                               +---------+
 * | Writer  |                           | Repo  |                               | Readers |
 * +---------+                           +-------+                               +---------+
 *      | --------------\                    |                                        |
 *      |-| Write phase |                    |                                        |
 *      | |-------------|                    |                                        |
 *      |                                    |                                        |
 *      | Write blob with random content     |                                        |
 *      |-----------------------------------→|                                        |
 *      |                                    |                                        |
 *      | Read range during write (rarely)   |                                        |
 *      |----------------------------------------------------------------------------→|
 *      |                                    |                                        |
 *      |                                    |                             Read range |
 *      |                                    |←---------------------------------------|
 *      |                                    |                                        |
 *      |                                    | Contents of range, or "not found"      |
 *      |                                    |---------------------------------------→|
 *      |                                    |                                        |
 *      |                               Acknowledge read, including checksum if found |
 *      |←----------------------------------------------------------------------------|
 *      |                                    |                                        |
 *      |                     Write complete |                                        |
 *      |←-----------------------------------|                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      | -------------\                     |                                        |
 *      |-| Read phase |                     |                                        |
 *      | |------------|                     |                                        |
 *      |                                    |                                        |
 *      | Read range [a,b)                   |                                        |
 *      |----------------------------------------------------------------------------→|
 *      |                                    |                                        |
 *      |                                    |                             Read range |
 *      |                                    |←---------------------------------------|
 *      |                                    |                                        |
 *      | Overwrite blob (rarely)            |                                        |
 *      |-----------------------------------→|                                        |
 *      |                                    |                                        |
 *      |                                    | Contents of range                      |
 *      |                                    |---------------------------------------→|
 *      |                                    |                                        |
 *      |                 Overwrite complete |                                        |
 *      |←-----------------------------------|                                        |
 *      |                                    |                                        |
 *      |                                    |               Ack read (with checksum) |
 *      |←----------------------------------------------------------------------------|
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      |                                    |                                        |
 *      | ---------------\                   |                                        |
 *      |-| Verify phase |                   |                                        |
 *      | |--------------|                   |                                        |
 *      |                                    |                                        |
 *      | Confirm checksums                  |                                        |
 *      |------------------                  |                                        |
 *      |                 |                  |                                        |
 *      |←-----------------                  |                                        |
 *      |                                    |                                        |
 *
 * </pre>
 *
 *
 * On success, details of how long everything took are returned. On failure, cancels the remote read tasks to try and avoid consuming
 * unnecessary resources.
 *
 */
public class BlobAnalyzeAction extends ActionType<BlobAnalyzeAction.Response> {

    private static final Logger logger = LogManager.getLogger(BlobAnalyzeAction.class);

    public static final BlobAnalyzeAction INSTANCE = new BlobAnalyzeAction();
    public static final String NAME = "cluster:admin/repository/analyze/blob";

    private BlobAnalyzeAction() {
        super(NAME, Response::new);
    }

    public static class TransportAction extends HandledTransportAction<Request, Response> {

        private static final Logger logger = BlobAnalyzeAction.logger;

        private final RepositoriesService repositoriesService;
        private final TransportService transportService;

        @Inject
        public TransportAction(TransportService transportService, ActionFilters actionFilters, RepositoriesService repositoriesService) {
            super(NAME, transportService, actionFilters, Request::new, ThreadPool.Names.SNAPSHOT);
            this.repositoriesService = repositoriesService;
            this.transportService = transportService;
        }

        @Override
        protected void doExecute(Task task, Request request, ActionListener<Response> listener) {
            final Repository repository = repositoriesService.repository(request.getRepositoryName());
            if (repository instanceof BlobStoreRepository == false) {
                throw new IllegalArgumentException("repository [" + request.getRepositoryName() + "] is not a blob-store repository");
            }
            if (repository.isReadOnly()) {
                throw new IllegalArgumentException("repository [" + request.getRepositoryName() + "] is read-only");
            }
            final BlobStoreRepository blobStoreRepository = (BlobStoreRepository) repository;
            final BlobPath path = blobStoreRepository.basePath().add(request.blobPath);
            final BlobContainer blobContainer = blobStoreRepository.blobStore().blobContainer(path);

            logger.trace("handling [{}]", request);

            assert task instanceof CancellableTask;
            new BlobAnalysis(transportService, (CancellableTask) task, request, blobStoreRepository, blobContainer, listener).run();
        }
    }

    /**
     * The atomic write API is based around a {@link BytesReference} which uses {@code int} for lengths and offsets and so on, so we can
     * only use it to write a blob with size at most {@link Integer#MAX_VALUE}:
     */
    static final long MAX_ATOMIC_WRITE_SIZE = Integer.MAX_VALUE;

    /**
     * Analysis on a single blob, performing the write(s) and orchestrating the read(s).
     */
    static class BlobAnalysis {
        private final TransportService transportService;
        private final CancellableTask task;
        private final BlobAnalyzeAction.Request request;
        private final BlobStoreRepository repository;
        private final BlobContainer blobContainer;
        private final ActionListener<BlobAnalyzeAction.Response> listener;
        private final Random random;
        private final boolean checksumWholeBlob;
        private final long checksumStart;
        private final long checksumEnd;
        private final List<DiscoveryNode> earlyReadNodes;
        private final List<DiscoveryNode> readNodes;
        private final GroupedActionListener<NodeResponse> readNodesListener;
        private final StepListener<WriteDetails> write1Step = new StepListener<>();
        private final StepListener<WriteDetails> write2Step = new StepListener<>();

        BlobAnalysis(
            TransportService transportService,
            CancellableTask task,
            BlobAnalyzeAction.Request request,
            BlobStoreRepository repository,
            BlobContainer blobContainer,
            ActionListener<BlobAnalyzeAction.Response> listener
        ) {
            this.transportService = transportService;
            this.task = task;
            this.request = request;
            this.repository = repository;
            this.blobContainer = blobContainer;
            this.listener = listener;
            this.random = new Random(this.request.seed);

            checksumWholeBlob = random.nextBoolean();
            if (checksumWholeBlob) {
                checksumStart = 0L;
                checksumEnd = request.targetLength;
            } else {
                checksumStart = randomLongBetween(0L, request.targetLength);
                checksumEnd = randomLongBetween(checksumStart + 1, request.targetLength + 1);
            }

            final ArrayList<DiscoveryNode> nodes = new ArrayList<>(request.nodes); // copy for shuffling purposes
            if (request.readEarly) {
                Collections.shuffle(nodes, random);
                earlyReadNodes = nodes.stream().limit(request.earlyReadNodeCount).collect(Collectors.toList());
            } else {
                earlyReadNodes = List.of();
            }
            Collections.shuffle(nodes, random);
            readNodes = nodes.stream().limit(request.readNodeCount).collect(Collectors.toList());

            final StepListener<Collection<NodeResponse>> readsCompleteStep = new StepListener<>();
            readNodesListener = new GroupedActionListener<>(
                new ThreadedActionListener<>(logger, transportService.getThreadPool(), ThreadPool.Names.SNAPSHOT, readsCompleteStep, false),
                earlyReadNodes.size() + readNodes.size()
            );

            // The order is important in this chain: if writing fails then we may never even start all the reads, and we want to cancel
            // any read tasks that were started, but the reads step only fails after all the reads have completed so there's no need to
            // cancel anything.
            write1Step.whenComplete(
                write1Details -> write2Step.whenComplete(
                    write2Details -> readsCompleteStep.whenComplete(
                        responses -> onReadsComplete(responses, write1Details, write2Details),
                        this::cleanUpAndReturnFailure
                    ),
                    this::cancelReadsCleanUpAndReturnFailure
                ),
                this::cancelReadsCleanUpAndReturnFailure
            );
        }

        void run() {
            writeRandomBlob(
                request.readEarly || (request.targetLength <= MAX_ATOMIC_WRITE_SIZE && random.nextBoolean()),
                true,
                this::doReadBeforeWriteComplete,
                write1Step
            );

            if (request.writeAndOverwrite) {
                assert request.targetLength <= MAX_ATOMIC_WRITE_SIZE : "oversized atomic write";
                write1Step.whenComplete(ignored -> writeRandomBlob(true, false, this::doReadAfterWrite, write2Step), ignored -> {});
            } else {
                write2Step.onResponse(null);
                doReadAfterWrite();
            }
        }

        private void writeRandomBlob(boolean atomic, boolean failIfExists, Runnable onLastRead, StepListener<WriteDetails> stepListener) {
            assert atomic == false || request.targetLength <= MAX_ATOMIC_WRITE_SIZE : "oversized atomic write";
            final RandomBlobContent content = new RandomBlobContent(
                request.getRepositoryName(),
                random.nextLong(),
                task::isCancelled,
                onLastRead
            );
            final AtomicLong throttledNanos = new AtomicLong();

            if (logger.isTraceEnabled()) {
                logger.trace("writing blob [atomic={}, failIfExists={}] for [{}]", atomic, failIfExists, request.getDescription());
            }
            final long startNanos = System.nanoTime();
            ActionListener.completeWith(stepListener, () -> {

                // TODO push some of this writing logic down into the blob container implementation.
                // E.g. for S3 blob containers we would like to choose somewhat more randomly between single-part and multi-part uploads,
                // rather than relying on the usual distinction based on the size of the blob.

                if (atomic || (request.targetLength <= MAX_ATOMIC_WRITE_SIZE && random.nextBoolean())) {
                    final RandomBlobContentBytesReference bytesReference = new RandomBlobContentBytesReference(
                        content,
                        Math.toIntExact(request.getTargetLength())
                    ) {
                        @Override
                        public StreamInput streamInput() throws IOException {
                            return new InputStreamStreamInput(
                                repository.maybeRateLimitSnapshots(super.streamInput(), throttledNanos::addAndGet)
                            );
                        }
                    };
                    if (atomic) {
                        blobContainer.writeBlobAtomic(request.blobName, bytesReference, failIfExists);
                    } else {
                        blobContainer.writeBlob(request.blobName, bytesReference, failIfExists);
                    }
                } else {
                    blobContainer.writeBlob(
                        request.blobName,
                        repository.maybeRateLimitSnapshots(
                            new RandomBlobContentStream(content, request.getTargetLength()),
                            throttledNanos::addAndGet
                        ),
                        request.targetLength,
                        failIfExists
                    );
                }
                final long elapsedNanos = System.nanoTime() - startNanos;
                final long checksum = content.getChecksum(checksumStart, checksumEnd);
                if (logger.isTraceEnabled()) {
                    logger.trace("finished writing blob for [{}], got checksum [{}]", request.getDescription(), checksum);
                }
                return new WriteDetails(request.targetLength, elapsedNanos, throttledNanos.get(), checksum);
            });
        }

        private void doReadBeforeWriteComplete() {
            if (earlyReadNodes.isEmpty() == false) {
                if (logger.isTraceEnabled()) {
                    logger.trace("sending read request to [{}] for [{}] before write complete", earlyReadNodes, request.getDescription());
                }
                readOnNodes(earlyReadNodes, true);
            }
        }

        private void doReadAfterWrite() {
            if (logger.isTraceEnabled()) {
                logger.trace("sending read request to [{}] for [{}] after write complete", readNodes, request.getDescription());
            }
            readOnNodes(readNodes, false);
        }

        private void readOnNodes(List<DiscoveryNode> nodes, boolean beforeWriteComplete) {
            for (DiscoveryNode node : nodes) {
                if (task.isCancelled()) {
                    // record dummy response since we're already on the path to failure
                    readNodesListener.onResponse(
                        new NodeResponse(node, beforeWriteComplete, GetBlobChecksumAction.Response.BLOB_NOT_FOUND)
                    );
                } else {
                    // no need for extra synchronization after checking if we were cancelled a couple of lines ago -- we haven't notified
                    // the outer listener yet so any bans on the children are still in place
                    final GetBlobChecksumAction.Request blobChecksumRequest = getBlobChecksumRequest();
                    transportService.sendChildRequest(
                        node,
                        GetBlobChecksumAction.NAME,
                        blobChecksumRequest,
                        task,
                        TransportRequestOptions.EMPTY,
                        new ActionListenerResponseHandler<>(new ActionListener<>() {
                            @Override
                            public void onResponse(GetBlobChecksumAction.Response response) {
                                readNodesListener.onResponse(makeNodeResponse(node, beforeWriteComplete, response));
                            }

                            @Override
                            public void onFailure(Exception e) {
                                readNodesListener.onFailure(
                                    new RepositoryVerificationException(
                                        request.getRepositoryName(),
                                        "["
                                            + blobChecksumRequest
                                            + "] ("
                                            + (beforeWriteComplete ? "before" : "after")
                                            + " write complete) failed on node ["
                                            + node
                                            + "]",
                                        e
                                    )
                                );

                            }
                        }, GetBlobChecksumAction.Response::new)
                    );
                }
            }
        }

        private GetBlobChecksumAction.Request getBlobChecksumRequest() {
            return new GetBlobChecksumAction.Request(
                request.getRepositoryName(),
                request.getBlobPath(),
                request.getBlobName(),
                checksumStart,
                checksumWholeBlob ? 0L : checksumEnd
            );
        }

        private NodeResponse makeNodeResponse(DiscoveryNode node, boolean beforeWriteComplete, GetBlobChecksumAction.Response response) {
            logger.trace(
                "received read response [{}] from [{}] for [{}] [beforeWriteComplete={}]",
                response,
                node,
                request.getDescription(),
                beforeWriteComplete
            );
            return new NodeResponse(node, beforeWriteComplete, response);
        }

        private void cancelReadsCleanUpAndReturnFailure(Exception exception) {
            transportService.getTaskManager().cancelTaskAndDescendants(task, "task failed", false, ActionListener.wrap(() -> {}));
            cleanUpAndReturnFailure(exception);
        }

        private void cleanUpAndReturnFailure(Exception exception) {
            if (logger.isTraceEnabled()) {
                logger.trace(new ParameterizedMessage("analysis failed [{}] cleaning up", request.getDescription()), exception);
            }
            try {
                blobContainer.deleteBlobsIgnoringIfNotExists(List.of(request.blobName));
            } catch (IOException ioException) {
                exception.addSuppressed(ioException);
                logger.warn(
                    new ParameterizedMessage(
                        "failure during post-failure cleanup while analysing repository [{}], you may need to manually remove [{}/{}]",
                        request.getRepositoryName(),
                        request.getBlobPath(),
                        request.getBlobName()
                    ),
                    exception
                );
            }
            listener.onFailure(
                new RepositoryVerificationException(
                    request.getRepositoryName(),
                    "failure processing [" + request.getDescription() + "]",
                    exception
                )
            );
        }

        private void onReadsComplete(Collection<NodeResponse> responses, WriteDetails write1Details, @Nullable WriteDetails write2Details) {
            if (task.isCancelled()) {
                cleanUpAndReturnFailure(
                    new RepositoryVerificationException(request.getRepositoryName(), "cancelled during checksum verification")
                );
                return;
            }

            final long checksumLength = checksumEnd - checksumStart;
            final String expectedChecksumDescription;
            final LongPredicate checksumPredicate;
            if (write2Details == null) {
                checksumPredicate = l -> l == write1Details.checksum;
                expectedChecksumDescription = Long.toString(write1Details.checksum);
            } else {
                checksumPredicate = l -> l == write1Details.checksum || l == write2Details.checksum;
                expectedChecksumDescription = write1Details.checksum + " or " + write2Details.checksum;
            }

            RepositoryVerificationException failure = null;
            for (final NodeResponse nodeResponse : responses) {
                final GetBlobChecksumAction.Response response = nodeResponse.response;
                final RepositoryVerificationException nodeFailure;
                if (response.isNotFound()) {
                    if (request.readEarly) {
                        nodeFailure = null; // "not found" is legitimate iff we tried to read it before the write completed
                    } else {
                        nodeFailure = new RepositoryVerificationException(
                            request.getRepositoryName(),
                            "node [" + nodeResponse.node + "] reported blob not found after it was written"
                        );
                    }
                } else {
                    final long actualChecksum = response.getChecksum();
                    if (response.getBytesRead() == checksumLength && checksumPredicate.test(actualChecksum)) {
                        nodeFailure = null; // checksum ok
                    } else {
                        nodeFailure = new RepositoryVerificationException(
                            request.getRepositoryName(),
                            "node ["
                                + nodeResponse.node
                                + "] failed during analysis: expected to read ["
                                + checksumStart
                                + "-"
                                + checksumEnd
                                + "], ["
                                + checksumLength
                                + "] bytes, with checksum ["
                                + expectedChecksumDescription
                                + "] but read ["
                                + response
                                + "]"
                        );
                    }
                }

                if (nodeFailure != null) {
                    if (failure == null) {
                        failure = nodeFailure;
                    } else {
                        failure.addSuppressed(nodeFailure);
                    }
                }
            }
            if (failure != null) {
                cleanUpAndReturnFailure(failure);
                return;
            }

            final long overwriteElapsedNanos = write2Details == null ? 0L : write2Details.elapsedNanos;
            final long overwriteThrottledNanos = write2Details == null ? 0L : write2Details.throttledNanos;
            listener.onResponse(
                new Response(
                    transportService.getLocalNode().getId(),
                    transportService.getLocalNode().getName(),
                    request.blobName,
                    request.targetLength,
                    request.readEarly,
                    request.writeAndOverwrite,
                    checksumStart,
                    checksumEnd,
                    write1Details.elapsedNanos,
                    overwriteElapsedNanos,
                    write1Details.throttledNanos + overwriteThrottledNanos,
                    responses.stream()
                        .map(
                            nr -> new ReadDetail(
                                nr.node.getId(),
                                nr.node.getName(),
                                nr.beforeWriteComplete,
                                nr.response.isNotFound(),
                                nr.response.getFirstByteNanos(),
                                nr.response.getElapsedNanos(),
                                nr.response.getThrottleNanos()
                            )
                        )
                        .collect(Collectors.toList())
                )
            );
        }

        /**
         * @return random non-negative long in [min, max)
         */
        private long randomLongBetween(long min, long max) {
            assert 0 <= min && min <= max;
            final long range = max - min;
            return range == 0 ? min : min + (random.nextLong() & Long.MAX_VALUE) % range;
        }
    }

    private static class NodeResponse {
        final DiscoveryNode node;
        final boolean beforeWriteComplete;
        final GetBlobChecksumAction.Response response;

        NodeResponse(DiscoveryNode node, boolean beforeWriteComplete, GetBlobChecksumAction.Response response) {
            this.node = node;
            this.beforeWriteComplete = beforeWriteComplete;
            this.response = response;
        }
    }

    private static class WriteDetails {
        final long bytesWritten;
        final long elapsedNanos;
        final long throttledNanos;
        final long checksum;

        private WriteDetails(long bytesWritten, long elapsedNanos, long throttledNanos, long checksum) {
            this.bytesWritten = bytesWritten;
            this.elapsedNanos = elapsedNanos;
            this.throttledNanos = throttledNanos;
            this.checksum = checksum;
        }
    }

    public static class Request extends ActionRequest implements TaskAwareRequest {
        private final String repositoryName;
        private final String blobPath;
        private final String blobName;

        private final long targetLength;
        private final long seed;
        private final List<DiscoveryNode> nodes;
        private final int readNodeCount;
        private final int earlyReadNodeCount;
        private final boolean readEarly;
        private final boolean writeAndOverwrite;

        Request(
            String repositoryName,
            String blobPath,
            String blobName,
            long targetLength,
            long seed,
            List<DiscoveryNode> nodes,
            int readNodeCount,
            int earlyReadNodeCount,
            boolean readEarly,
            boolean writeAndOverwrite
        ) {
            assert 0 < targetLength;
            assert targetLength <= MAX_ATOMIC_WRITE_SIZE || (readEarly == false && writeAndOverwrite == false) : "oversized atomic write";
            this.repositoryName = repositoryName;
            this.blobPath = blobPath;
            this.blobName = blobName;
            this.targetLength = targetLength;
            this.seed = seed;
            this.nodes = nodes;
            this.readNodeCount = readNodeCount;
            this.earlyReadNodeCount = earlyReadNodeCount;
            this.readEarly = readEarly;
            this.writeAndOverwrite = writeAndOverwrite;
        }

        Request(StreamInput in) throws IOException {
            super(in);
            repositoryName = in.readString();
            blobPath = in.readString();
            blobName = in.readString();
            targetLength = in.readVLong();
            seed = in.readLong();
            nodes = in.readList(DiscoveryNode::new);
            readNodeCount = in.readVInt();
            earlyReadNodeCount = in.readVInt();
            readEarly = in.readBoolean();
            writeAndOverwrite = in.readBoolean();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(repositoryName);
            out.writeString(blobPath);
            out.writeString(blobName);
            out.writeVLong(targetLength);
            out.writeLong(seed);
            out.writeList(nodes);
            out.writeVInt(readNodeCount);
            out.writeVInt(earlyReadNodeCount);
            out.writeBoolean(readEarly);
            out.writeBoolean(writeAndOverwrite);
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public String getDescription() {
            return "blob analysis ["
                + repositoryName
                + ":"
                + blobPath
                + "/"
                + blobName
                + ", length="
                + targetLength
                + ", seed="
                + seed
                + ", readEarly="
                + readEarly
                + ", writeAndOverwrite="
                + writeAndOverwrite
                + "]";
        }

        @Override
        public String toString() {
            return "BlobAnalyzeAction.Request{" + getDescription() + "}";
        }

        @Override
        public Task createTask(long id, String type, String action, TaskId parentTaskId, Map<String, String> headers) {
            return new CancellableTask(id, type, action, getDescription(), parentTaskId, headers) {
                @Override
                public boolean shouldCancelChildrenOnCancellation() {
                    return true;
                }
            };
        }

        public String getRepositoryName() {
            return repositoryName;
        }

        public String getBlobPath() {
            return blobPath;
        }

        public String getBlobName() {
            return blobName;
        }

        public long getTargetLength() {
            return targetLength;
        }

        public long getSeed() {
            return seed;
        }

    }

    public static class Response extends ActionResponse implements ToXContentObject {

        private final String nodeId;
        private final String nodeName;
        private final String blobName;
        private final long blobLength;
        private final boolean readEarly;
        private final boolean overwrite;
        private final long checksumStart;
        private final long checksumEnd;

        private final long writeElapsedNanos;
        private final long overwriteElapsedNanos;
        private final long writeThrottledNanos;
        private final List<ReadDetail> readDetails;

        public Response(
            String nodeId,
            String nodeName,
            String blobName,
            long blobLength,
            boolean readEarly,
            boolean overwrite,
            long checksumStart,
            long checksumEnd,
            long writeElapsedNanos,
            long overwriteElapsedNanos,
            long writeThrottledNanos,
            List<ReadDetail> readDetails
        ) {
            this.nodeId = nodeId;
            this.nodeName = nodeName;
            this.blobName = blobName;
            this.blobLength = blobLength;
            this.readEarly = readEarly;
            this.overwrite = overwrite;
            this.checksumStart = checksumStart;
            this.checksumEnd = checksumEnd;
            this.writeElapsedNanos = writeElapsedNanos;
            this.overwriteElapsedNanos = overwriteElapsedNanos;
            this.writeThrottledNanos = writeThrottledNanos;
            this.readDetails = readDetails;
        }

        public Response(StreamInput in) throws IOException {
            super(in);
            nodeId = in.readString();
            nodeName = in.readString();
            blobName = in.readString();
            blobLength = in.readVLong();
            readEarly = in.readBoolean();
            overwrite = in.readBoolean();
            checksumStart = in.readVLong();
            checksumEnd = in.readVLong();
            writeElapsedNanos = in.readVLong();
            overwriteElapsedNanos = in.readVLong();
            writeThrottledNanos = in.readVLong();
            readDetails = in.readList(ReadDetail::new);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeString(nodeId);
            out.writeString(nodeName);
            out.writeString(blobName);
            out.writeVLong(blobLength);
            out.writeBoolean(readEarly);
            out.writeBoolean(overwrite);
            out.writeVLong(checksumStart);
            out.writeVLong(checksumEnd);
            out.writeVLong(writeElapsedNanos);
            out.writeVLong(overwriteElapsedNanos);
            out.writeVLong(writeThrottledNanos);
            out.writeList(readDetails);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();

            builder.startObject("blob");
            builder.field("name", blobName);
            builder.humanReadableField("size_bytes", "size", new ByteSizeValue(blobLength));
            builder.field("read_start", checksumStart);
            builder.field("read_end", checksumEnd);
            builder.field("read_early", readEarly);
            builder.field("overwritten", overwrite);
            builder.endObject();

            builder.startObject("writer_node");
            builder.field("id", nodeId);
            builder.field("name", nodeName);
            builder.endObject();

            humanReadableNanos(builder, "write_elapsed_nanos", "write_elapsed", writeElapsedNanos);
            if (overwrite) {
                humanReadableNanos(builder, "overwrite_elapsed_nanos", "overwrite_elapsed", overwriteElapsedNanos);
            }
            humanReadableNanos(builder, "write_throttled_nanos", "write_throttled", writeThrottledNanos);

            builder.startArray("reads");
            for (ReadDetail readDetail : readDetails) {
                readDetail.toXContent(builder, params);
            }
            builder.endArray();

            builder.endObject();
            return builder;
        }

        long getWriteBytes() {
            return blobLength + (overwrite ? blobLength : 0L);
        }

        long getWriteThrottledNanos() {
            return writeThrottledNanos;
        }

        long getWriteElapsedNanos() {
            return writeElapsedNanos + overwriteElapsedNanos;
        }

        List<ReadDetail> getReadDetails() {
            return readDetails;
        }

        long getChecksumBytes() {
            return checksumEnd - checksumStart;
        }
    }

    public static class ReadDetail implements Writeable, ToXContentFragment {

        private final String nodeId;
        private final String nodeName;
        private final boolean beforeWriteComplete;
        private final boolean isNotFound;
        private final long firstByteNanos;
        private final long throttleNanos;
        private final long elapsedNanos;

        public ReadDetail(
            String nodeId,
            String nodeName,
            boolean beforeWriteComplete,
            boolean isNotFound,
            long firstByteNanos,
            long elapsedNanos,
            long throttleNanos
        ) {
            this.nodeId = nodeId;
            this.nodeName = nodeName;
            this.beforeWriteComplete = beforeWriteComplete;
            this.isNotFound = isNotFound;
            this.firstByteNanos = firstByteNanos;
            this.throttleNanos = throttleNanos;
            this.elapsedNanos = elapsedNanos;
        }

        public ReadDetail(StreamInput in) throws IOException {
            nodeId = in.readString();
            nodeName = in.readString();
            beforeWriteComplete = in.readBoolean();
            isNotFound = in.readBoolean();
            firstByteNanos = in.readVLong();
            throttleNanos = in.readVLong();
            elapsedNanos = in.readVLong();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeString(nodeId);
            out.writeString(nodeName);
            out.writeBoolean(beforeWriteComplete);
            out.writeBoolean(isNotFound);
            out.writeVLong(firstByteNanos);
            out.writeVLong(throttleNanos);
            out.writeVLong(elapsedNanos);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();

            builder.startObject("node");
            builder.field("id", nodeId);
            builder.field("name", nodeName);
            builder.endObject();

            if (beforeWriteComplete) {
                builder.field("before_write_complete", true);
            }

            if (isNotFound) {
                builder.field("found", false);
            } else {
                builder.field("found", true);
                humanReadableNanos(builder, "first_byte_time_nanos", "first_byte_time", firstByteNanos);
                humanReadableNanos(builder, "elapsed_nanos", "elapsed", elapsedNanos);
                humanReadableNanos(builder, "throttled_nanos", "throttled", throttleNanos);
            }

            builder.endObject();
            return builder;
        }

        long getFirstByteNanos() {
            return firstByteNanos;
        }

        long getThrottledNanos() {
            return throttleNanos;
        }

        long getElapsedNanos() {
            return elapsedNanos;
        }
    }

}
