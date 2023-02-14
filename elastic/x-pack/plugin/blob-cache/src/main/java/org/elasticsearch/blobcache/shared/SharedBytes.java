/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.blobcache.shared;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.blobcache.BlobCacheUtils;
import org.elasticsearch.blobcache.common.ByteBufferReference;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.core.AbstractRefCounted;
import org.elasticsearch.core.IOUtils;
import org.elasticsearch.core.SuppressForbidden;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.xpack.searchablesnapshots.preallocate.Preallocate;

import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Map;
import java.util.function.IntConsumer;
import java.util.function.LongConsumer;

public class SharedBytes extends AbstractRefCounted {

    /**
     * Thread local direct byte buffer to aggregate multiple positional writes to the cache file.
     */
    public static final int MAX_BYTES_PER_WRITE = StrictMath.toIntExact(
        ByteSizeValue.parseBytesSizeValue(
            System.getProperty("es.searchable.snapshot.shared_cache.write_buffer.size", "2m"),
            "es.searchable.snapshot.shared_cache.write_buffer.size"
        ).getBytes()
    );
    private static final Logger logger = LogManager.getLogger(SharedBytes.class);

    public static int PAGE_SIZE = 4096;

    private static final String CACHE_FILE_NAME = "shared_snapshot_cache";

    private static final StandardOpenOption[] OPEN_OPTIONS = new StandardOpenOption[] {
        StandardOpenOption.READ,
        StandardOpenOption.WRITE,
        StandardOpenOption.CREATE };

    final int numRegions;
    final long regionSize;

    // TODO: for systems like Windows without true p-write/read support we should split this up into multiple channels since positional
    // operations in #IO are not contention-free there (https://bugs.java.com/bugdatabase/view_bug.do?bug_id=6265734)
    private final FileChannel fileChannel;
    private final Path path;

    private final IntConsumer writeBytes;
    private final IntConsumer readBytes;

    SharedBytes(int numRegions, long regionSize, NodeEnvironment environment, IntConsumer writeBytes, IntConsumer readBytes)
        throws IOException {
        this.numRegions = numRegions;
        this.regionSize = regionSize;
        final long fileSize = numRegions * regionSize;
        Path cacheFile = null;
        if (fileSize > 0) {
            cacheFile = findCacheSnapshotCacheFilePath(environment, fileSize);
            Preallocate.preallocate(cacheFile, fileSize);
            this.fileChannel = FileChannel.open(cacheFile, OPEN_OPTIONS);
            assert this.fileChannel.size() == fileSize : "expected file size " + fileSize + " but was " + fileChannel.size();
        } else {
            this.fileChannel = null;
            for (Path path : environment.nodeDataPaths()) {
                Files.deleteIfExists(path.resolve(CACHE_FILE_NAME));
            }
        }
        this.path = cacheFile;
        this.writeBytes = writeBytes;
        this.readBytes = readBytes;
    }

    /**
     * Tries to find a suitable path to a searchable snapshots shared cache file in the data paths founds in the environment.
     *
     * @return path for the cache file or {@code null} if none could be found
     */
    public static Path findCacheSnapshotCacheFilePath(NodeEnvironment environment, long fileSize) throws IOException {
        assert environment.nodeDataPaths().length == 1;
        Path path = environment.nodeDataPaths()[0];
        Files.createDirectories(path);
        // TODO: be resilient to this check failing and try next path?
        long usableSpace = Environment.getUsableSpace(path);
        Path p = path.resolve(CACHE_FILE_NAME);
        if (Files.exists(p)) {
            usableSpace += Files.size(p);
        }
        // TODO: leave some margin for error here
        if (usableSpace > fileSize) {
            return p;
        } else {
            throw new IOException("Not enough free space for cache file of size [" + fileSize + "] in path [" + path + "]");
        }
    }

    /**
     * Copy {@code length} bytes from {@code input} to {@code fc}, only doing writes aligned along {@link #PAGE_SIZE}.
     *
     * @param fc output cache file reference
     * @param input stream to read from
     * @param fileChannelPos position in {@code fc} to write to
     * @param relativePos relative position in the Lucene file the is read from {@code input}
     * @param length number of bytes to copy
     * @param progressUpdater callback to invoke with the number of copied bytes as they are copied
     * @param buf bytebuffer to use for writing
     * @param cacheFile object that describes the cached file, only used in logging and exception throwing as context information
     * @throws IOException on failure
     */
    public static void copyToCacheFileAligned(
        IO fc,
        InputStream input,
        long fileChannelPos,
        long relativePos,
        long length,
        LongConsumer progressUpdater,
        ByteBuffer buf,
        final Object cacheFile
    ) throws IOException {
        long bytesCopied = 0L;
        long remaining = length;
        while (remaining > 0L) {
            final int bytesRead = BlobCacheUtils.readSafe(input, buf, relativePos, remaining, cacheFile);
            if (buf.hasRemaining()) {
                break;
            }
            long bytesWritten = positionalWrite(fc, fileChannelPos + bytesCopied, buf);
            bytesCopied += bytesWritten;
            progressUpdater.accept(bytesCopied);
            remaining -= bytesRead;
        }
        if (remaining > 0) {
            // ensure that last write is aligned on 4k boundaries (= page size)
            final int remainder = buf.position() % PAGE_SIZE;
            final int adjustment = remainder == 0 ? 0 : PAGE_SIZE - remainder;
            buf.position(buf.position() + adjustment);
            long bytesWritten = positionalWrite(fc, fileChannelPos + bytesCopied, buf);
            bytesCopied += bytesWritten;
            final long adjustedBytesCopied = bytesCopied - adjustment; // adjust to not break RangeFileTracker
            assert adjustedBytesCopied == length;
            progressUpdater.accept(adjustedBytesCopied);
        }
    }

    private static int positionalWrite(IO fc, long start, ByteBuffer byteBuffer) throws IOException {
        byteBuffer.flip();
        int written = fc.write(byteBuffer, start);
        assert byteBuffer.hasRemaining() == false;
        byteBuffer.clear();
        return written;
    }

    /**
     * Read {@code length} bytes from given shared bytes at given {@code channelPos} into {@code byteBufferReference} at given
     * {@code relativePos}.
     * @param fc shared bytes channel to read from
     * @param channelPos position in {@code fc} to read from
     * @param relativePos position in {@code byteBufferReference}
     * @param length number of bytes to read
     * @param byteBufferReference buffer reference
     * @param cacheFile cache file reference used for exception messages only
     * @return number of bytes read
     * @throws IOException on failure
     */
    public static int readCacheFile(
        final IO fc,
        long channelPos,
        long relativePos,
        long length,
        final ByteBufferReference byteBufferReference,
        Object cacheFile
    ) throws IOException {
        if (length == 0L) {
            return 0;
        }
        final int bytesRead;
        final ByteBuffer dup = byteBufferReference.tryAcquire(Math.toIntExact(relativePos), Math.toIntExact(length));
        if (dup != null) {
            try {
                bytesRead = fc.read(dup, channelPos);
                if (bytesRead == -1) {
                    BlobCacheUtils.throwEOF(channelPos, dup.remaining(), cacheFile);
                }
            } finally {
                byteBufferReference.release();
            }
        } else {
            // return fake response
            return Math.toIntExact(length);
        }
        return bytesRead;
    }

    @Override
    protected void closeInternal() {
        try {
            IOUtils.close(fileChannel, path == null ? null : () -> Files.deleteIfExists(path));
        } catch (IOException e) {
            logger.warn("Failed to clean up shared bytes file", e);
        }
    }

    private final Map<Integer, IO> ios = ConcurrentCollections.newConcurrentMap();

    public IO getFileChannel(int sharedBytesPos) {
        assert fileChannel != null;
        return ios.compute(sharedBytesPos, (p, io) -> {
            if (io == null || io.tryIncRef() == false) {
                final IO newIO;
                boolean success = false;
                incRef();
                try {
                    newIO = new IO(p);
                    success = true;
                } finally {
                    if (success == false) {
                        decRef();
                    }
                }
                return newIO;
            }
            return io;
        });
    }

    long getPhysicalOffset(long chunkPosition) {
        long physicalOffset = chunkPosition * regionSize;
        assert physicalOffset <= numRegions * regionSize;
        return physicalOffset;
    }

    public final class IO extends AbstractRefCounted {

        private final int sharedBytesPos;
        private final long pageStart;

        private IO(final int sharedBytesPos) {
            this.sharedBytesPos = sharedBytesPos;
            pageStart = getPhysicalOffset(sharedBytesPos);
        }

        @SuppressForbidden(reason = "Use positional reads on purpose")
        public int read(ByteBuffer dst, long position) throws IOException {
            checkOffsets(position, dst.remaining());
            final int bytesRead = fileChannel.read(dst, position);
            readBytes.accept(bytesRead);
            return bytesRead;
        }

        @SuppressForbidden(reason = "Use positional writes on purpose")
        public int write(ByteBuffer src, long position) throws IOException {
            // check if writes are page size aligned for optimal performance
            assert position % PAGE_SIZE == 0;
            assert src.remaining() % PAGE_SIZE == 0;
            checkOffsets(position, src.remaining());
            final int bytesWritten = fileChannel.write(src, position);
            writeBytes.accept(bytesWritten);
            return bytesWritten;
        }

        private void checkOffsets(long position, long length) {
            long pageEnd = pageStart + regionSize;
            if (position < pageStart || position > pageEnd || position + length > pageEnd) {
                assert false;
                throw new IllegalArgumentException("bad access");
            }
        }

        @Override
        protected void closeInternal() {
            ios.remove(sharedBytesPos, this);
            SharedBytes.this.decRef();
        }
    }

    public static ByteSizeValue pageAligned(ByteSizeValue val) {
        final long remainder = val.getBytes() % PAGE_SIZE;
        if (remainder != 0L) {
            return ByteSizeValue.ofBytes(val.getBytes() + PAGE_SIZE - remainder);
        }
        return val;
    }
}
