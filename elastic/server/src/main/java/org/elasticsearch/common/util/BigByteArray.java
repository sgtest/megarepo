/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.common.util;

import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.BytesRefIterator;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.core.Streams;

import java.io.IOException;
import java.io.InputStream;
import java.util.Arrays;

import static org.elasticsearch.common.util.BigLongArray.writePages;
import static org.elasticsearch.common.util.PageCacheRecycler.BYTE_PAGE_SIZE;
import static org.elasticsearch.common.util.PageCacheRecycler.PAGE_SIZE_IN_BYTES;

/**
 * Byte array abstraction able to support more than 2B values. This implementation slices data into fixed-sized blocks of
 * configurable length.
 */
final class BigByteArray extends AbstractBigByteArray implements ByteArray {

    private static final BigByteArray ESTIMATOR = new BigByteArray(0, BigArrays.NON_RECYCLING_INSTANCE, false);

    /** Constructor. */
    BigByteArray(long size, BigArrays bigArrays, boolean clearOnResize) {
        super(BYTE_PAGE_SIZE, bigArrays, clearOnResize, size);
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        writePages(out, size, pages, Byte.BYTES);
    }

    @Override
    public byte get(long index) {
        final int pageIndex = pageIndex(index);
        final int indexInPage = indexInPage(index);
        return pages[pageIndex][indexInPage];
    }

    @Override
    public byte set(long index, byte value) {
        final int pageIndex = pageIndex(index);
        final int indexInPage = indexInPage(index);
        final byte[] page = getPageForWriting(pageIndex);
        final byte ret = page[indexInPage];
        page[indexInPage] = value;
        return ret;
    }

    @Override
    public boolean get(long index, int len, BytesRef ref) {
        assert index + len <= size();
        if (len == 0) {
            ref.length = 0;
            return false;
        }
        int pageIndex = pageIndex(index);
        final int indexInPage = indexInPage(index);
        if (indexInPage + len <= pageSize()) {
            ref.bytes = pages[pageIndex];
            ref.offset = indexInPage;
            ref.length = len;
            return false;
        } else {
            ref.bytes = new byte[len];
            ref.offset = 0;
            ref.length = pageSize() - indexInPage;
            System.arraycopy(pages[pageIndex], indexInPage, ref.bytes, 0, ref.length);
            do {
                ++pageIndex;
                final int copyLength = Math.min(pageSize(), len - ref.length);
                System.arraycopy(pages[pageIndex], 0, ref.bytes, ref.length, copyLength);
                ref.length += copyLength;
            } while (ref.length < len);
            return true;
        }
    }

    @Override
    public void set(long index, byte[] buf, int offset, int len) {
        assert index + len <= size();
        int pageIndex = pageIndex(index);
        final int indexInPage = indexInPage(index);
        if (indexInPage + len <= pageSize()) {
            System.arraycopy(buf, offset, getPageForWriting(pageIndex), indexInPage, len);
        } else {
            int copyLen = pageSize() - indexInPage;
            System.arraycopy(buf, offset, getPageForWriting(pageIndex), indexInPage, copyLen);
            do {
                ++pageIndex;
                offset += copyLen;
                len -= copyLen;
                copyLen = Math.min(len, pageSize());
                System.arraycopy(buf, offset, getPageForWriting(pageIndex), 0, copyLen);
            } while (len > copyLen);
        }
    }

    @Override
    public void fill(long fromIndex, long toIndex, byte value) {
        if (fromIndex > toIndex) {
            throw new IllegalArgumentException();
        }
        final int fromPage = pageIndex(fromIndex);
        final int toPage = pageIndex(toIndex - 1);
        if (fromPage == toPage) {
            Arrays.fill(getPageForWriting(fromPage), indexInPage(fromIndex), indexInPage(toIndex - 1) + 1, value);
        } else {
            Arrays.fill(getPageForWriting(fromPage), indexInPage(fromIndex), pages[fromPage].length, value);
            for (int i = fromPage + 1; i < toPage; ++i) {
                Arrays.fill(getPageForWriting(i), value);
            }
            Arrays.fill(getPageForWriting(toPage), 0, indexInPage(toIndex - 1) + 1, value);
        }
    }

    @Override
    public boolean hasArray() {
        return false;
    }

    @Override
    public byte[] array() {
        assert false;
        throw new UnsupportedOperationException();
    }

    @Override
    public BytesRefIterator iterator() {
        return new BytesRefIterator() {
            int i = 0;
            long remained = size;

            @Override
            public BytesRef next() {
                if (remained == 0) {
                    return null;
                }
                byte[] page = pages[i++];
                int len = Math.toIntExact(Math.min(page.length, remained));
                remained -= len;
                return new BytesRef(page, 0, len);
            }
        };
    }

    @Override
    public void fillWith(InputStream in) throws IOException {
        for (int i = 0; i < pages.length - 1; i++) {
            Streams.readFully(in, getPageForWriting(i), 0, PAGE_SIZE_IN_BYTES);
        }
        Streams.readFully(in, getPageForWriting(pages.length - 1), 0, Math.toIntExact(size - (pages.length - 1L) * PAGE_SIZE_IN_BYTES));
    }

    @Override
    protected int numBytesPerElement() {
        return 1;
    }

    /** Estimates the number of bytes that would be consumed by an array of the given size. */
    public static long estimateRamBytes(final long size) {
        return ESTIMATOR.ramBytesEstimated(size);
    }

}
