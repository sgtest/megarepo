/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.nio;

import org.elasticsearch.common.util.concurrent.AbstractRefCounted;

import java.io.Closeable;
import java.nio.ByteBuffer;

public class Page implements Closeable {

    private final ByteBuffer byteBuffer;
    // This is reference counted as some implementations want to retain the byte pages by calling
    // duplicate. With reference counting we can increment the reference count, return a new page,
    // and safely close the pages independently. The closeable will not be called until each page is
    // released.
    private final RefCountedCloseable refCountedCloseable;

    public Page(ByteBuffer byteBuffer) {
        this(byteBuffer, () -> {});
    }

    public Page(ByteBuffer byteBuffer, Runnable closeable) {
        this(byteBuffer, new RefCountedCloseable(closeable));
    }

    private Page(ByteBuffer byteBuffer, RefCountedCloseable refCountedCloseable) {
        this.byteBuffer = byteBuffer;
        this.refCountedCloseable = refCountedCloseable;
    }

    /**
     * Duplicates this page and increments the reference count. The new page must be closed independently
     * of the original page.
     *
     * @return the new page
     */
    public Page duplicate() {
        refCountedCloseable.incRef();
        return new Page(byteBuffer.duplicate(), refCountedCloseable);
    }

    /**
     * Returns the {@link ByteBuffer} for this page. Modifications to the limits, positions, etc of the
     * buffer will also mutate this page. Call {@link ByteBuffer#duplicate()} to avoid mutating the page.
     *
     * @return the byte buffer
     */
    public ByteBuffer byteBuffer() {
        return byteBuffer;
    }

    @Override
    public void close() {
        refCountedCloseable.decRef();
    }

    private static class RefCountedCloseable extends AbstractRefCounted {

        private final Runnable closeable;

        private RefCountedCloseable(Runnable closeable) {
            super("byte array page");
            this.closeable = closeable;
        }

        @Override
        protected void closeInternal() {
            closeable.run();
        }
    }
}
