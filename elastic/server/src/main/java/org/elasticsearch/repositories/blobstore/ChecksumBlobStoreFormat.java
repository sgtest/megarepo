/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.repositories.blobstore;

import org.apache.lucene.codecs.CodecUtil;
import org.apache.lucene.index.CorruptIndexException;
import org.apache.lucene.index.IndexFormatTooNewException;
import org.apache.lucene.index.IndexFormatTooOldException;
import org.apache.lucene.store.ByteBuffersDataInput;
import org.apache.lucene.store.ByteBuffersIndexInput;
import org.apache.lucene.store.IndexInput;
import org.apache.lucene.store.OutputStreamIndexOutput;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.CheckedFunction;
import org.elasticsearch.common.blobstore.BlobContainer;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressorFactory;
import org.elasticsearch.common.io.stream.ReleasableBytesStreamOutput;
import org.elasticsearch.common.lucene.store.ByteArrayIndexInput;
import org.elasticsearch.common.lucene.store.IndexOutputOutputStream;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.core.internal.io.Streams;
import org.elasticsearch.gateway.CorruptStateException;
import org.elasticsearch.snapshots.SnapshotInfo;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;

/**
 * Snapshot metadata file format used in v2.0 and above
 */
public final class ChecksumBlobStoreFormat<T extends ToXContent> {

    // Serialization parameters to specify correct context for metadata serialization
    public static final ToXContent.Params SNAPSHOT_ONLY_FORMAT_PARAMS;

    static {
        Map<String, String> snapshotOnlyParams = new HashMap<>();
        // when metadata is serialized certain elements of the metadata shouldn't be included into snapshot
        // exclusion of these elements is done by setting Metadata.CONTEXT_MODE_PARAM to Metadata.CONTEXT_MODE_SNAPSHOT
        snapshotOnlyParams.put(Metadata.CONTEXT_MODE_PARAM, Metadata.CONTEXT_MODE_SNAPSHOT);
        // serialize SnapshotInfo using the SNAPSHOT mode
        snapshotOnlyParams.put(SnapshotInfo.CONTEXT_MODE_PARAM, SnapshotInfo.CONTEXT_MODE_SNAPSHOT);
        SNAPSHOT_ONLY_FORMAT_PARAMS = new ToXContent.MapParams(snapshotOnlyParams);
    }

    // The format version
    public static final int VERSION = 1;

    private static final int BUFFER_SIZE = 4096;

    private final String codec;

    private final String blobNameFormat;

    private final CheckedFunction<XContentParser, T, IOException> reader;

    /**
     * @param codec          codec name
     * @param blobNameFormat format of the blobname in {@link String#format} format
     * @param reader         prototype object that can deserialize T from XContent
     */
    public ChecksumBlobStoreFormat(String codec, String blobNameFormat, CheckedFunction<XContentParser, T, IOException> reader) {
        this.reader = reader;
        this.blobNameFormat = blobNameFormat;
        this.codec = codec;
    }

    /**
     * Reads and parses the blob with given name, applying name translation using the {link #blobName} method
     *
     * @param blobContainer blob container
     * @param name          name to be translated into
     * @return parsed blob object
     */
    public T read(BlobContainer blobContainer, String name, NamedXContentRegistry namedXContentRegistry,
                  BigArrays bigArrays) throws IOException {
        String blobName = blobName(name);
        try (ReleasableBytesStreamOutput out = new ReleasableBytesStreamOutput(bigArrays);
             InputStream in = blobContainer.readBlob(blobName)) {
            Streams.copy(in, out, false);
            return deserialize(blobName, namedXContentRegistry, out.bytes());
        }
    }

    public String blobName(String name) {
        return String.format(Locale.ROOT, blobNameFormat, name);
    }

    public T deserialize(String blobName, NamedXContentRegistry namedXContentRegistry, BytesReference bytes) throws IOException {
        final String resourceDesc = "ChecksumBlobStoreFormat.readBlob(blob=\"" + blobName + "\")";
        try {
            final IndexInput indexInput = bytes.length() > 0 ? new ByteBuffersIndexInput(
                    new ByteBuffersDataInput(Arrays.asList(BytesReference.toByteBuffers(bytes))), resourceDesc)
                    : new ByteArrayIndexInput(resourceDesc, BytesRef.EMPTY_BYTES);
            CodecUtil.checksumEntireFile(indexInput);
            CodecUtil.checkHeader(indexInput, codec, VERSION, VERSION);
            long filePointer = indexInput.getFilePointer();
            long contentSize = indexInput.length() - CodecUtil.footerLength() - filePointer;
            try (XContentParser parser = XContentHelper.createParser(namedXContentRegistry, LoggingDeprecationHandler.INSTANCE,
                bytes.slice((int) filePointer, (int) contentSize), XContentType.SMILE)) {
                return reader.apply(parser);
            }
        } catch (CorruptIndexException | IndexFormatTooOldException | IndexFormatTooNewException ex) {
            // we trick this into a dedicated exception with the original stacktrace
            throw new CorruptStateException(ex);
        }
    }

    /**
     * Writes blob with resolving the blob name using {@link #blobName} method.
     * <p>
     * The blob will optionally by compressed.
     *
     * @param obj                 object to be serialized
     * @param blobContainer       blob container
     * @param name                blob name
     * @param compress            whether to use compression
     */
    public void write(T obj, BlobContainer blobContainer, String name, boolean compress, BigArrays bigArrays) throws IOException {
        final String blobName = blobName(name);
        serialize(obj, blobName, compress, bigArrays, bytes -> blobContainer.writeBlob(blobName, bytes, false));
    }

    public void serialize(final T obj, final String blobName, final boolean compress, BigArrays bigArrays,
                          CheckedConsumer<BytesReference, IOException> consumer) throws IOException {
        try (ReleasableBytesStreamOutput outputStream = new ReleasableBytesStreamOutput(bigArrays)) {
            try (OutputStreamIndexOutput indexOutput = new OutputStreamIndexOutput(
                    "ChecksumBlobStoreFormat.writeBlob(blob=\"" + blobName + "\")", blobName,
                    org.elasticsearch.common.io.Streams.noCloseStream(outputStream), BUFFER_SIZE)) {
                CodecUtil.writeHeader(indexOutput, codec, VERSION);
                try (OutputStream indexOutputOutputStream = new IndexOutputOutputStream(indexOutput) {
                    @Override
                    public void close() {
                        // this is important since some of the XContentBuilders write bytes on close.
                        // in order to write the footer we need to prevent closing the actual index input.
                    }
                }; XContentBuilder builder = XContentFactory.contentBuilder(XContentType.SMILE,
                        compress ? CompressorFactory.COMPRESSOR.threadLocalOutputStream(indexOutputOutputStream)
                                : indexOutputOutputStream)) {
                    builder.startObject();
                    obj.toXContent(builder, SNAPSHOT_ONLY_FORMAT_PARAMS);
                    builder.endObject();
                }
                CodecUtil.writeFooter(indexOutput);
            }
            consumer.accept(outputStream.bytes());
        }
    }
}
