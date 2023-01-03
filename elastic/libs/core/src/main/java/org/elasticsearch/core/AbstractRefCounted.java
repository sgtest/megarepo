/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.core;

import java.util.Objects;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * A basic {@link RefCounted} implementation that is initialized with a ref count of 1 and calls {@link #closeInternal()} once it reaches
 * a 0 ref count.
 */
public abstract class AbstractRefCounted implements RefCounted {
    public static final String ALREADY_CLOSED_MESSAGE = "already closed, can't increment ref count";

    private final AtomicInteger refCount = new AtomicInteger(1);

    protected AbstractRefCounted() {}

    @Override
    public final void incRef() {
        if (tryIncRef() == false) {
            alreadyClosed();
        }
    }

    @Override
    public final boolean tryIncRef() {
        do {
            int i = refCount.get();
            if (i > 0) {
                if (refCount.compareAndSet(i, i + 1)) {
                    touch();
                    return true;
                }
            } else {
                return false;
            }
        } while (true);
    }

    @Override
    public final boolean decRef() {
        touch();
        int i = refCount.decrementAndGet();
        assert i >= 0 : "invalid decRef call: already closed";
        if (i == 0) {
            try {
                closeInternal();
            } catch (Exception e) {
                assert false : e;
                throw e;
            }
            return true;
        }
        return false;
    }

    @Override
    public final boolean hasReferences() {
        return refCount.get() > 0;
    }

    /**
     * Called whenever the ref count is incremented or decremented. Can be overridden to record access to the instance for debugging
     * purposes.
     */
    protected void touch() {}

    protected void alreadyClosed() {
        final int currentRefCount = refCount.get();
        assert currentRefCount == 0 : currentRefCount;
        throw new IllegalStateException(ALREADY_CLOSED_MESSAGE);
    }

    /**
     * Returns the current reference count.
     */
    public final int refCount() {
        return this.refCount.get();
    }

    /**
     * Method that is invoked once the reference count reaches zero.
     * Implementations of this method must handle all exceptions and may not throw any exceptions.
     */
    protected abstract void closeInternal();

    /**
     * Construct an {@link AbstractRefCounted} which runs the given {@link Runnable} when all references are released.
     */
    public static AbstractRefCounted of(Runnable onClose) {
        Objects.requireNonNull(onClose);
        return new AbstractRefCounted() {
            @Override
            protected void closeInternal() {
                onClose.run();
            }

            @Override
            public String toString() {
                return "refCounted[" + onClose + "]";
            }
        };
    }

}
