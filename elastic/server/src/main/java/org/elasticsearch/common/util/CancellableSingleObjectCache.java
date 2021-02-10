/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.common.util;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ListenableActionFuture;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.util.concurrent.AbstractRefCounted;
import org.elasticsearch.tasks.TaskCancelledException;

import java.util.ArrayList;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;

/**
 * A cache of a single object whose refresh process can be cancelled. The cached value is computed lazily on the first retrieval, and
 * associated with a key which is used to determine its freshness for subsequent retrievals.
 * <p>
 * This is useful for things like computing stats over cluster metadata: the first time stats are requested they are computed, but
 * subsequent calls re-use the computed value as long as they pertain to the same metadata version. If stats are requested for a different
 * metadata version then the cached value is dropped and a new one is computed.
 * <p>
 * Retrievals happen via the async {@link #get} method. If a retrieval is cancelled (e.g. the channel on which to return the stats is
 * closed) then the computation carries on running in case another retrieval for the same key arrives in future. However if all of the
 * retrievals for a key are cancelled <i>and</i> a retrieval occurs for a fresher key then the computation itself is cancelled.
 * <p>
 * Cancellation is based on polling: the {@link #refresh} method checks whether it should abort whenever it is convenient to do so, which in
 * turn checks all the pending retrievals to see whether they have been cancelled.
 *
 * @param <Input> The type of the input to the computation of the cached value.
 * @param <Key>   The key type. The cached value is associated with a key, and subsequent {@link #get} calls compare keys of the given input
 *                value to determine whether the cached value is fresh or not. See {@link #isFresh}.
 * @param <Value> The type of the cached value.
 */
public abstract class CancellableSingleObjectCache<Input, Key, Value> {

    private final AtomicReference<CachedItem> currentCachedItemRef = new AtomicReference<>();

    /**
     * Compute a new value for the cache.
     * <p>
     * If an exception is thrown, or passed to the {@code listener}, then it is passed on to all waiting listeners but it is not cached so
     * that subsequent retrievals will trigger subsequent calls to this method.
     * <p>
     * Implementations of this method should poll for cancellation by running {@code ensureNotCancelled} whenever appropriate. The
     * computation is cancelled if all of the corresponding retrievals have been cancelled <i>and</i> a retrieval has since happened for a
     * fresher key.
     *
     * @param input              The input to this computation, which will be converted to a key and used to determine whether it is
     *                           suitably fresh for future requests too.
     * @param ensureNotCancelled A {@link Runnable} which throws a {@link TaskCancelledException} if the result of the computation is no
     *                           longer needed. On cancellation, notifying the {@code listener} is optional.
     * @param listener           A {@link ActionListener} which should be notified when the computation completes. If the computation fails
     *                           by calling {@link ActionListener#onFailure} then the result is returned to the pending listeners but is not
     *                           cached.
     */
    protected abstract void refresh(Input input, Runnable ensureNotCancelled, ActionListener<Value> listener);

    /**
     * Compute the key for the given input value.
     */
    protected abstract Key getKey(Input input);

    /**
     * Compute whether the {@code currentKey} is fresh enough for a retrieval associated with {@code newKey}.
     *
     * @param currentKey The key of the current (cached or pending) value.
     * @param newKey     The key associated with a new retrival.
     * @return {@code true} if a value computed for {@code currentKey} is fresh enough to satisfy a retrieval for {@code newKey}.
     */
    protected boolean isFresh(Key currentKey, Key newKey) {
        return currentKey.equals(newKey);
    }

    /**
     * Start a retrieval for the value associated with the given {@code input}, and pass it to the given {@code listener}.
     * <p>
     * If a fresh-enough result is available when this method is called then the {@code listener} is notified immediately, on this thread.
     * If a fresh-enough result is already being computed then the {@code listener} is captured and will be notified when the result becomes
     * available, on the thread on which the refresh completes. If no fresh-enough result is either pending or available then this method
     * starts to compute one by calling {@link #refresh} on this thread.
     *
     * @param input       The input to compute the desired value, converted to a {@link Key} to determine if the value that's currently
     *                    cached or pending is fresh enough.
     * @param isCancelled Returns {@code true} if the listener no longer requires the value being computed.
     * @param listener    The listener to notify when the desired value becomes available.
     */
    public final void get(Input input, BooleanSupplier isCancelled, ActionListener<Value> listener) {

        final Key key = getKey(input);

        CachedItem newCachedItem = null;

        do {
            if (isCancelled.getAsBoolean()) {
                listener.onFailure(new TaskCancelledException("task cancelled"));
                return;
            }

            final CachedItem currentCachedItem = currentCachedItemRef.get();
            if (currentCachedItem != null && isFresh(currentCachedItem.getKey(), key)) {
                final boolean listenerAdded = currentCachedItem.addListener(listener, isCancelled);
                if (listenerAdded) {
                    return;
                }

                assert currentCachedItem.refCount() == 0 : currentCachedItem.refCount();
                assert currentCachedItemRef.get() != currentCachedItem;

                // Our item was only just released, possibly cancelled, by another get() with a fresher key. We don't simply retry
                // since that would evict the new item. Instead let's see if it was cancelled or whether it completed properly.
                if (currentCachedItem.getFuture().isDone()) {
                    try {
                        listener.onResponse(currentCachedItem.getFuture().actionGet(0L));
                        return;
                    } catch (TaskCancelledException e) {
                        // previous task was cancelled before completion, therefore we must perform our own one-shot refresh
                    } catch (Exception e) {
                        // either the refresh completed exceptionally or the listener threw an exception; call onFailure() either way
                        listener.onFailure(e);
                        return;
                    }
                } // else it's just about to be cancelled, so we can just retry knowing that it will be removed very soon

                continue;
            }

            if (newCachedItem == null) {
                newCachedItem = new CachedItem(key);
            }

            if (currentCachedItemRef.compareAndSet(currentCachedItem, newCachedItem)) {
                if (currentCachedItem != null) {
                    currentCachedItem.decRef();
                }
                startRefresh(input, newCachedItem);
                final boolean listenerAdded = newCachedItem.addListener(listener, isCancelled);
                assert listenerAdded;
                newCachedItem.decRef();
                return;
            }
            // else the CAS failed because we lost a race to a concurrent retrieval; try again from the top since we expect the race winner
            // to be fresh enough for us and therefore we can just wait for its result.
        } while (true);
    }

    private void startRefresh(Input input, CachedItem cachedItem) {
        try {
            refresh(input, cachedItem::ensureNotCancelled, cachedItem.getFuture());
        } catch (Exception e) {
            cachedItem.getFuture().onFailure(e);
        }
    }

    /**
     * An item in the cache, representing a single invocation of {@link #refresh}.
     * <p>
     * This item is ref-counted so that it can be cancelled if it becomes irrelevant. References are held by:
     * <ul>
     *     <li>Every listener that is waiting for the result, released on cancellation. There's no need to release on completion because
     *     there's nothing to cancel once the refresh has completed.</li>
     *     <li>The cache itself, released once this item is no longer the current one in the cache, either because it failed or because a
     *     fresher computation was started.</li>
     *     <li>The process that adds the first listener, released once the first listener is added.</li>
     * </ul>
     */
    private final class CachedItem extends AbstractRefCounted {

        private final Key key;
        private final ListenableActionFuture<Value> future = new ListenableActionFuture<>();
        private final CancellationChecks cancellationChecks = new CancellationChecks();

        CachedItem(Key key) {
            super("cached item");
            this.key = key;
            incRef(); // start with a refcount of 2 so we're not closed while adding the first listener
            this.future.addListener(new ActionListener<>() {
                @Override
                public void onResponse(Value value) {
                    cancellationChecks.clear();
                }

                @Override
                public void onFailure(Exception e) {
                    cancellationChecks.clear();
                    // Do not cache this failure
                    if (currentCachedItemRef.compareAndSet(CachedItem.this, null)) {
                        // Release reference held by the cache, so that concurrent calls to addListener() fail and retry. Not totally
                        // necessary, we could also fail those listeners as if they'd been added slightly sooner, but it makes the ref
                        // counting easier to document.
                        decRef();
                    }
                }
            });
        }

        Key getKey() {
            return key;
        }

        ListenableActionFuture<Value> getFuture() {
            return future;
        }

        boolean addListener(ActionListener<Value> listener, BooleanSupplier isCancelled) {
            if (tryIncRef()) {
                if (future.isDone()) {
                    // No need to bother with ref counting & cancellation any more, just complete the listener.
                    // We know it wasn't cancelled because there are still references.
                    ActionListener.completeWith(listener, () -> future.actionGet(0L));
                } else {
                    // Refresh is still pending; it's not cancelled because there are still references.
                    future.addListener(listener);
                    final AtomicBoolean released = new AtomicBoolean();
                    cancellationChecks.add(() -> {
                        if (released.get() == false && isCancelled.getAsBoolean() && released.compareAndSet(false, true)) {
                            decRef();
                        }
                    });
                }
                return true;
            } else {
                return false;
            }
        }

        void ensureNotCancelled() {
            cancellationChecks.runAll();
            if (refCount() == 0) {
                throw new TaskCancelledException("task cancelled");
            }
        }

        @Override
        protected void closeInternal() {
            // Complete the future (and hence all its listeners) with an exception if it hasn't already been completed.
            future.onFailure(new TaskCancelledException("task cancelled"));
        }
    }

    private static final class CancellationChecks {
        @Nullable // if cleared
        private ArrayList<Runnable> checks = new ArrayList<>();

        synchronized void clear() {
            checks = null;
        }

        synchronized void add(Runnable check) {
            if (checks != null) {
                checks.add(check);
            }
        }

        void runAll() {
            // It's ok not to run all the checks so there's no need for a completely synchronized iteration.
            final int count;
            synchronized (this) {
                if (checks == null) {
                    return;
                }
                count = checks.size();
            }
            for (int i = 0; i < count; i++) {
                final Runnable cancellationCheck;
                synchronized (this) {
                    if (checks == null) {
                        return;
                    }
                    cancellationCheck = checks.get(i);
                }
                cancellationCheck.run();
            }
        }
    }
}
