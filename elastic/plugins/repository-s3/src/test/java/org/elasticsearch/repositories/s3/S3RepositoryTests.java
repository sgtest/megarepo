/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.repositories.s3;

import com.amazonaws.services.s3.AbstractAmazonS3;

import org.elasticsearch.cluster.metadata.RepositoryMetaData;
import org.elasticsearch.common.component.AbstractLifecycleComponent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.repositories.RepositoryException;
import org.elasticsearch.test.ESTestCase;
import org.hamcrest.Matchers;
import java.io.IOException;
import java.util.Collections;
import java.util.Map;

import static org.hamcrest.Matchers.containsString;

public class S3RepositoryTests extends ESTestCase {

    private static class DummyS3Client extends AbstractAmazonS3 {

        @Override
        public boolean doesBucketExist(String bucketName) {
            return true;
        }

        @Override
        public void shutdown() {
            // TODO check is closed
        }
    }

    private static class DummyS3Service extends AbstractLifecycleComponent implements AwsS3Service {
        DummyS3Service() {
            super(Settings.EMPTY);
        }
        @Override
        protected void doStart() {}
        @Override
        protected void doStop() {}
        @Override
        protected void doClose() {}
        @Override
        public AmazonS3Reference client(String clientName) {
            return new AmazonS3Reference(new DummyS3Client());
        }

        @Override
        public Map<String, S3ClientSettings> refreshAndClearCache(Map<String, S3ClientSettings> clientsSettings) {
            return Collections.emptyMap();
        }

        @Override
        public void close() {
        }
    }

    public void testInvalidChunkBufferSizeSettings() throws IOException {
        // chunk < buffer should fail
        final Settings s1 = bufferAndChunkSettings(10, 5);
        final Exception e1 = expectThrows(RepositoryException.class,
                () -> new S3Repository(getRepositoryMetaData(s1), Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service()));
        assertThat(e1.getMessage(), containsString("chunk_size (5mb) can't be lower than buffer_size (10mb)"));
        // chunk > buffer should pass
        final Settings s2 = bufferAndChunkSettings(5, 10);
        new S3Repository(getRepositoryMetaData(s2), Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service()).close();
        // chunk = buffer should pass
        final Settings s3 = bufferAndChunkSettings(5, 5);
        new S3Repository(getRepositoryMetaData(s3), Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service()).close();
        // buffer < 5mb should fail
        final Settings s4 = bufferAndChunkSettings(4, 10);
        final IllegalArgumentException e2 = expectThrows(IllegalArgumentException.class,
                () -> new S3Repository(getRepositoryMetaData(s4), Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service())
                        .close());
        assertThat(e2.getMessage(), containsString("failed to parse value [4mb] for setting [buffer_size], must be >= [5mb]"));
        final Settings s5 = bufferAndChunkSettings(5, 6000000);
        final IllegalArgumentException e3 = expectThrows(IllegalArgumentException.class,
                () -> new S3Repository(getRepositoryMetaData(s5), Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service())
                        .close());
        assertThat(e3.getMessage(), containsString("failed to parse value [6000000mb] for setting [chunk_size], must be <= [5tb]"));
    }

    private Settings bufferAndChunkSettings(long buffer, long chunk) {
        return Settings.builder()
                .put(S3Repository.BUFFER_SIZE_SETTING.getKey(), new ByteSizeValue(buffer, ByteSizeUnit.MB).getStringRep())
                .put(S3Repository.CHUNK_SIZE_SETTING.getKey(), new ByteSizeValue(chunk, ByteSizeUnit.MB).getStringRep())
                .build();
    }

    private RepositoryMetaData getRepositoryMetaData(Settings settings) {
        return new RepositoryMetaData("dummy-repo", "mock", Settings.builder().put(settings).build());
    }

    public void testBasePathSetting() throws IOException {
        final RepositoryMetaData metadata = new RepositoryMetaData("dummy-repo", "mock", Settings.builder()
                .put(S3Repository.BASE_PATH_SETTING.getKey(), "foo/bar").build());
        try (S3Repository s3repo = new S3Repository(metadata, Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service())) {
            assertEquals("foo/bar/", s3repo.basePath().buildAsString());
        }
    }

    public void testDefaultBufferSize() throws IOException {
        final RepositoryMetaData metadata = new RepositoryMetaData("dummy-repo", "mock", Settings.EMPTY);
        try (S3Repository s3repo = new S3Repository(metadata, Settings.EMPTY, NamedXContentRegistry.EMPTY, new DummyS3Service())) {
            final long defaultBufferSize = ((S3BlobStore) s3repo.blobStore()).bufferSizeInBytes();
            assertThat(defaultBufferSize, Matchers.lessThanOrEqualTo(100L * 1024 * 1024));
            assertThat(defaultBufferSize, Matchers.greaterThanOrEqualTo(5L * 1024 * 1024));
        }
    }
}
