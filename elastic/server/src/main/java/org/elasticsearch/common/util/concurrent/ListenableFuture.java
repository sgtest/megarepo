/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.common.util.concurrent;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRunnable;
import org.elasticsearch.action.support.ContextPreservingActionListener;
import org.elasticsearch.common.collect.Tuple;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.TimeUnit;

/**
 * A future implementation that allows for the result to be passed to listeners waiting for
 * notification. This is useful for cases where a computation is requested many times
 * concurrently, but really only needs to be performed a single time. Once the computation
 * has been performed the registered listeners will be notified by submitting a runnable
 * for execution in the provided {@link ExecutorService}. If the computation has already
 * been performed, a request to add a listener will simply result in execution of the listener
 * on the calling thread.
 */
public final class ListenableFuture<V> extends BaseFuture<V> implements ActionListener<V> {

    private volatile boolean done = false;
    private final List<Tuple<ActionListener<V>, ExecutorService>> listeners = new ArrayList<>();


    /**
     * Adds a listener to this future. If the future has not yet completed, the listener will be
     * notified of a response or exception in a runnable submitted to the ExecutorService provided.
     * If the future has completed, the listener will be notified immediately without forking to
     * a different thread.
     */
    public void addListener(ActionListener<V> listener, ExecutorService executor) {
        addListener(listener, executor, null);
    }

    /**
     * Adds a listener to this future. If the future has not yet completed, the listener will be
     * notified of a response or exception in a runnable submitted to the ExecutorService provided.
     * If the future has completed, the listener will be notified immediately without forking to
     * a different thread.
     *
     * It will apply the provided ThreadContext (if not null) when executing the listening.
     */
    public void addListener(ActionListener<V> listener, ExecutorService executor, ThreadContext threadContext) {
        if (done) {
            // run the callback directly, we don't hold the lock and don't need to fork!
            notifyListener(listener, EsExecutors.newDirectExecutorService());
        } else {
            final boolean run;
            // check done under lock since it could have been modified and protect modifications
            // to the list under lock
            synchronized (this) {
                if (done) {
                    run = true;
                } else {
                    final ActionListener<V> wrappedListener;
                    if (threadContext == null) {
                        wrappedListener = listener;
                    } else {
                        wrappedListener = ContextPreservingActionListener.wrapPreservingContext(listener, threadContext);
                    }
                    listeners.add(new Tuple<>(wrappedListener, executor));
                    run = false;
                }
            }

            if (run) {
                // run the callback directly, we don't hold the lock and don't need to fork!
                notifyListener(listener, EsExecutors.newDirectExecutorService());
            }
        }
    }

    @Override
    protected synchronized void done(boolean ignored) {
        done = true;
        listeners.forEach(t -> notifyListener(t.v1(), t.v2()));
        // release references to any listeners as we no longer need them and will live
        // much longer than the listeners in most cases
        listeners.clear();
    }

    private void notifyListener(ActionListener<V> listener, ExecutorService executorService) {
        try {
            executorService.execute(new ActionRunnable<>(listener) {
                @Override
                protected void doRun() {
                    // call get in a non-blocking fashion as we could be on a network thread
                    // or another thread like the scheduler, which we should never block!
                    V value = FutureUtils.get(ListenableFuture.this, 0L, TimeUnit.NANOSECONDS);
                    listener.onResponse(value);
                }

                @Override
                public String toString() {
                    return "ListenableFuture notification";
                }
            });
        } catch (Exception e) {
            listener.onFailure(e);
        }
    }

    @Override
    public void onResponse(V v) {
        final boolean set = set(v);
        if (set == false) {
            throw new IllegalStateException("did not set value, value or exception already set?");
        }
    }

    @Override
    public void onFailure(Exception e) {
        final boolean set = setException(e);
        if (set == false) {
            throw new IllegalStateException("did not set exception, value already set or exception already set?");
        }
    }
}
