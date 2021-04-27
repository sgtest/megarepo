/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.repositories.gcs;

import org.elasticsearch.common.blobstore.BlobContainer;
import org.elasticsearch.common.blobstore.BlobMetadata;
import org.elasticsearch.common.blobstore.BlobPath;
import org.elasticsearch.common.blobstore.BlobStoreException;
import org.elasticsearch.common.blobstore.DeleteResult;
import org.elasticsearch.common.blobstore.support.AbstractBlobContainer;
import org.elasticsearch.common.bytes.BytesReference;

import java.io.IOException;
import java.io.InputStream;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

class GoogleCloudStorageBlobContainer extends AbstractBlobContainer {

    private final GoogleCloudStorageBlobStore blobStore;
    private final String path;

    GoogleCloudStorageBlobContainer(BlobPath path, GoogleCloudStorageBlobStore blobStore) {
        super(path);
        this.blobStore = blobStore;
        this.path = path.buildAsString();
    }

    @Override
    public boolean blobExists(String blobName) {
        try {
            return blobStore.blobExists(buildKey(blobName));
        } catch (Exception e) {
            throw new BlobStoreException("Failed to check if blob [" + blobName + "] exists", e);
        }
    }

    @Override
    public Map<String, BlobMetadata> listBlobs() throws IOException {
        return blobStore.listBlobs(path);
    }

    @Override
    public Map<String, BlobContainer> children() throws IOException {
        return blobStore.listChildren(path());
    }

    @Override
    public Map<String, BlobMetadata> listBlobsByPrefix(String prefix) throws IOException {
        return blobStore.listBlobsByPrefix(path, prefix);
    }

    @Override
    public InputStream readBlob(String blobName) throws IOException {
        return blobStore.readBlob(buildKey(blobName));
    }

    @Override
    public InputStream readBlob(final String blobName, final long position, final long length) throws IOException {
        return blobStore.readBlob(buildKey(blobName), position, length);
    }

    @Override
    public void writeBlob(String blobName, InputStream inputStream, long blobSize, boolean failIfAlreadyExists) throws IOException {
        blobStore.writeBlob(buildKey(blobName), inputStream, blobSize, failIfAlreadyExists);
    }

    @Override
    public void writeBlob(String blobName, BytesReference bytes, boolean failIfAlreadyExists) throws IOException {
        blobStore.writeBlob(buildKey(blobName), bytes, failIfAlreadyExists);
    }

    @Override
    public void writeBlobAtomic(String blobName, BytesReference bytes, boolean failIfAlreadyExists) throws IOException {
        writeBlob(blobName, bytes, failIfAlreadyExists);
    }

    @Override
    public DeleteResult delete() throws IOException {
        return blobStore.deleteDirectory(path().buildAsString());
    }

    @Override
    public void deleteBlobsIgnoringIfNotExists(List<String> blobNames) throws IOException {
        blobStore.deleteBlobsIgnoringIfNotExists(blobNames.stream().map(this::buildKey).collect(Collectors.toList()));
    }

    private String buildKey(String blobName) {
        assert blobName != null;
        return path + blobName;
    }
}
