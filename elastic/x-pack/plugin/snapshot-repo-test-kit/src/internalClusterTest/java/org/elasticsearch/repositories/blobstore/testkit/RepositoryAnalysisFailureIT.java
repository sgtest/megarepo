/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.repositories.blobstore.testkit;

import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.cluster.metadata.RepositoryMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.blobstore.BlobContainer;
import org.elasticsearch.common.blobstore.BlobMetadata;
import org.elasticsearch.common.blobstore.BlobPath;
import org.elasticsearch.common.blobstore.BlobStore;
import org.elasticsearch.common.blobstore.DeleteResult;
import org.elasticsearch.common.blobstore.support.PlainBlobMetadata;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.common.util.concurrent.CountDown;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.env.Environment;
import org.elasticsearch.indices.recovery.RecoverySettings;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.RepositoryPlugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryMissingException;
import org.elasticsearch.repositories.RepositoryVerificationException;
import org.elasticsearch.repositories.blobstore.BlobStoreRepository;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.snapshots.AbstractSnapshotIntegTestCase;
import org.elasticsearch.xpack.core.LocalStateCompositeXPackPlugin;
import org.junit.Before;

import java.io.ByteArrayInputStream;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.FileAlreadyExistsException;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.anEmptyMap;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.nullValue;

public class RepositoryAnalysisFailureIT extends AbstractSnapshotIntegTestCase {

    private DisruptableBlobStore blobStore;

    @Before
    public void suppressConsistencyChecks() {
        disableRepoConsistencyCheck("repository is not used for snapshots");
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return List.of(TestPlugin.class, LocalStateCompositeXPackPlugin.class, SnapshotRepositoryTestKit.class);
    }

    @Before
    public void createBlobStore() {
        createRepositoryNoVerify("test-repo", TestPlugin.DISRUPTABLE_REPO_TYPE);

        blobStore = new DisruptableBlobStore();
        for (final RepositoriesService repositoriesService : internalCluster().getInstances(RepositoriesService.class)) {
            try {
                ((DisruptableRepository) repositoriesService.repository("test-repo")).setBlobStore(blobStore);
            } catch (RepositoryMissingException e) {
                // it's only present on voting masters and data nodes
            }
        }
    }

    public void testSuccess() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.blobCount(1);
        request.maxBlobSize(new ByteSizeValue(10L));

        final RepositoryAnalyzeAction.Response response = analyseRepository(request);
        assertThat(response.status(), equalTo(RestStatus.OK));
    }

    public void testFailsOnReadError() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        final CountDown countDown = new CountDown(between(1, request.getBlobCount()));
        blobStore.setDisruption(new Disruption() {
            @Override
            public byte[] onRead(byte[] actualContents, long position, long length) throws IOException {
                if (countDown.countDown()) {
                    throw new IOException("simulated");
                }
                return actualContents;
            }
        });

        final Exception exception = expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
        final IOException ioException = (IOException) ExceptionsHelper.unwrap(exception, IOException.class);
        assert ioException != null : exception;
        assertThat(ioException.getMessage(), equalTo("simulated"));
    }

    public void testFailsOnNotFoundAfterWrite() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));
        request.rareActionProbability(0.0); // not found on an early read or an overwrite is ok

        final CountDown countDown = new CountDown(between(1, request.getBlobCount()));

        blobStore.setDisruption(new Disruption() {
            @Override
            public byte[] onRead(byte[] actualContents, long position, long length) {
                if (countDown.countDown()) {
                    return null;
                }
                return actualContents;
            }
        });

        expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
    }

    public void testFailsOnChecksumMismatch() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        final CountDown countDown = new CountDown(between(1, request.getBlobCount()));

        blobStore.setDisruption(new Disruption() {
            @Override
            public byte[] onRead(byte[] actualContents, long position, long length) {
                final byte[] disruptedContents = actualContents == null ? null : Arrays.copyOf(actualContents, actualContents.length);
                if (actualContents != null && countDown.countDown()) {
                    // CRC32 should always detect a single bit flip
                    disruptedContents[Math.toIntExact(position + randomLongBetween(0, length - 1))] ^= 1 << between(0, 7);
                }
                return disruptedContents;
            }
        });

        expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
    }

    public void testFailsOnWriteException() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        final CountDown countDown = new CountDown(between(1, request.getBlobCount()));

        blobStore.setDisruption(new Disruption() {

            @Override
            public void onWrite() throws IOException {
                if (countDown.countDown()) {
                    throw new IOException("simulated");
                }
            }

        });

        final Exception exception = expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
        final IOException ioException = (IOException) ExceptionsHelper.unwrap(exception, IOException.class);
        assert ioException != null : exception;
        assertThat(ioException.getMessage(), equalTo("simulated"));
    }

    public void testFailsOnIncompleteListing() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        blobStore.setDisruption(new Disruption() {

            @Override
            public Map<String, BlobMetadata> onList(Map<String, BlobMetadata> actualListing) {
                final HashMap<String, BlobMetadata> listing = new HashMap<>(actualListing);
                listing.keySet().iterator().remove();
                return listing;
            }

        });

        expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
    }

    public void testFailsOnListingException() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        final CountDown countDown = new CountDown(1);
        blobStore.setDisruption(new Disruption() {

            @Override
            public Map<String, BlobMetadata> onList(Map<String, BlobMetadata> actualListing) throws IOException {
                if (countDown.countDown()) {
                    throw new IOException("simulated");
                }
                return actualListing;
            }
        });

        expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
    }

    public void testFailsOnDeleteException() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        blobStore.setDisruption(new Disruption() {
            @Override
            public void onDelete() throws IOException {
                throw new IOException("simulated");
            }
        });

        expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
    }

    public void testFailsOnIncompleteDelete() {
        final RepositoryAnalyzeAction.Request request = new RepositoryAnalyzeAction.Request("test-repo");
        request.maxBlobSize(new ByteSizeValue(10L));

        blobStore.setDisruption(new Disruption() {

            volatile boolean isDeleted;

            @Override
            public void onDelete() {
                isDeleted = true;
            }

            @Override
            public Map<String, BlobMetadata> onList(Map<String, BlobMetadata> actualListing) {
                if (isDeleted) {
                    assertThat(actualListing, anEmptyMap());
                    return Collections.singletonMap("leftover", new PlainBlobMetadata("leftover", 1));
                } else {
                    return actualListing;
                }
            }
        });

        expectThrows(RepositoryVerificationException.class, () -> analyseRepository(request));
    }

    private RepositoryAnalyzeAction.Response analyseRepository(RepositoryAnalyzeAction.Request request) {
        return client().execute(RepositoryAnalyzeAction.INSTANCE, request).actionGet(30L, TimeUnit.SECONDS);
    }

    public static class TestPlugin extends Plugin implements RepositoryPlugin {

        static final String DISRUPTABLE_REPO_TYPE = "disruptable";

        @Override
        public Map<String, Repository.Factory> getRepositories(
            Environment env,
            NamedXContentRegistry namedXContentRegistry,
            ClusterService clusterService,
            BigArrays bigArrays,
            RecoverySettings recoverySettings
        ) {
            return Map.of(
                DISRUPTABLE_REPO_TYPE,
                metadata -> new DisruptableRepository(
                    metadata,
                    namedXContentRegistry,
                    clusterService,
                    bigArrays,
                    recoverySettings,
                    new BlobPath()
                )
            );
        }
    }

    static class DisruptableRepository extends BlobStoreRepository {

        private final AtomicReference<BlobStore> blobStoreRef = new AtomicReference<>();

        DisruptableRepository(
            RepositoryMetadata metadata,
            NamedXContentRegistry namedXContentRegistry,
            ClusterService clusterService,
            BigArrays bigArrays,
            RecoverySettings recoverySettings,
            BlobPath basePath
        ) {
            super(metadata, namedXContentRegistry, clusterService, bigArrays, recoverySettings, basePath);
        }

        void setBlobStore(BlobStore blobStore) {
            assertTrue(blobStoreRef.compareAndSet(null, blobStore));
        }

        @Override
        protected BlobStore createBlobStore() {
            final BlobStore blobStore = blobStoreRef.get();
            assertNotNull(blobStore);
            return blobStore;
        }
    }

    static class DisruptableBlobStore implements BlobStore {

        @Nullable // if deleted
        private DisruptableBlobContainer blobContainer;

        private Disruption disruption = Disruption.NONE;

        @Override
        public BlobContainer blobContainer(BlobPath path) {
            synchronized (this) {
                if (blobContainer == null) {
                    blobContainer = new DisruptableBlobContainer(path, this::deleteContainer, disruption);
                }
                return blobContainer;
            }
        }

        private void deleteContainer(DisruptableBlobContainer container) {
            blobContainer = null;
        }

        @Override
        public void close() {}

        public void setDisruption(Disruption disruption) {
            assertThat("cannot change disruption while blob container exists", blobContainer, nullValue());
            this.disruption = disruption;
        }
    }

    interface Disruption {

        Disruption NONE = new Disruption() {
        };

        default byte[] onRead(byte[] actualContents, long position, long length) throws IOException {
            return actualContents;
        }

        default void onWrite() throws IOException {}

        default Map<String, BlobMetadata> onList(Map<String, BlobMetadata> actualListing) throws IOException {
            return actualListing;
        }

        default void onDelete() throws IOException {}
    }

    static class DisruptableBlobContainer implements BlobContainer {

        private final BlobPath path;
        private final Consumer<DisruptableBlobContainer> deleteContainer;
        private final Disruption disruption;
        private final Map<String, byte[]> blobs = ConcurrentCollections.newConcurrentMap();

        DisruptableBlobContainer(BlobPath path, Consumer<DisruptableBlobContainer> deleteContainer, Disruption disruption) {
            this.path = path;
            this.deleteContainer = deleteContainer;
            this.disruption = disruption;
        }

        @Override
        public BlobPath path() {
            return path;
        }

        @Override
        public boolean blobExists(String blobName) {
            return blobs.containsKey(blobName);
        }

        @Override
        public InputStream readBlob(String blobName) throws IOException {
            final byte[] actualContents = blobs.get(blobName);
            final byte[] disruptedContents = disruption.onRead(actualContents, 0L, actualContents == null ? 0L : actualContents.length);
            if (disruptedContents == null) {
                throw new FileNotFoundException(blobName + " not found");
            }
            return new ByteArrayInputStream(disruptedContents);
        }

        @Override
        public InputStream readBlob(String blobName, long position, long length) throws IOException {
            final byte[] actualContents = blobs.get(blobName);
            final byte[] disruptedContents = disruption.onRead(actualContents, position, length);
            if (disruptedContents == null) {
                throw new FileNotFoundException(blobName + " not found");
            }
            final int truncatedLength = Math.toIntExact(Math.min(length, disruptedContents.length - position));
            return new ByteArrayInputStream(disruptedContents, Math.toIntExact(position), truncatedLength);
        }

        @Override
        public void writeBlob(String blobName, InputStream inputStream, long blobSize, boolean failIfAlreadyExists) throws IOException {
            writeBlobAtomic(blobName, inputStream, failIfAlreadyExists);
        }

        @Override
        public void writeBlob(String blobName, BytesReference bytes, boolean failIfAlreadyExists) throws IOException {
            writeBlob(blobName, bytes.streamInput(), bytes.length(), failIfAlreadyExists);
        }

        @Override
        public void writeBlobAtomic(String blobName, BytesReference bytes, boolean failIfAlreadyExists) throws IOException {
            writeBlobAtomic(blobName, bytes.streamInput(), failIfAlreadyExists);
        }

        private void writeBlobAtomic(String blobName, InputStream inputStream, boolean failIfAlreadyExists) throws IOException {
            if (failIfAlreadyExists && blobs.get(blobName) != null) {
                throw new FileAlreadyExistsException(blobName);
            }

            final byte[] contents = inputStream.readAllBytes();
            disruption.onWrite();
            blobs.put(blobName, contents);
        }

        @Override
        public DeleteResult delete() throws IOException {
            disruption.onDelete();
            deleteContainer.accept(this);
            final DeleteResult deleteResult = new DeleteResult(blobs.size(), blobs.values().stream().mapToLong(b -> b.length).sum());
            blobs.clear();
            return deleteResult;
        }

        @Override
        public void deleteBlobsIgnoringIfNotExists(List<String> blobNames) {
            blobs.keySet().removeAll(blobNames);
        }

        @Override
        public Map<String, BlobMetadata> listBlobs() throws IOException {
            return disruption.onList(
                blobs.entrySet()
                    .stream()
                    .collect(Collectors.toMap(Map.Entry::getKey, e -> new PlainBlobMetadata(e.getKey(), e.getValue().length)))
            );
        }

        @Override
        public Map<String, BlobContainer> children() {
            return Map.of();
        }

        @Override
        public Map<String, BlobMetadata> listBlobsByPrefix(String blobNamePrefix) throws IOException {
            final Map<String, BlobMetadata> blobMetadataByName = listBlobs();
            blobMetadataByName.keySet().removeIf(s -> s.startsWith(blobNamePrefix) == false);
            return blobMetadataByName;
        }
    }

}
