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

package org.elasticsearch.nio;

import java.io.IOException;
import java.nio.channels.NetworkChannel;
import java.nio.channels.SelectableChannel;
import java.nio.channels.SelectionKey;
import java.util.concurrent.CompletableFuture;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

/**
 * Implements the logic related to interacting with a java.nio channel. For example: registering with a
 * selector, managing the selection key, closing, etc is implemented by this class or its subclasses.
 *
 * @param <S> the type of channel
 */
public abstract class ChannelContext<S extends SelectableChannel & NetworkChannel> {

    protected final S rawChannel;
    private final Consumer<Exception> exceptionHandler;
    private final CompletableFuture<Void> closeContext = new CompletableFuture<>();
    private volatile SelectionKey selectionKey;

    ChannelContext(S rawChannel, Consumer<Exception> exceptionHandler) {
        this.rawChannel = rawChannel;
        this.exceptionHandler = exceptionHandler;
    }

    protected void register() throws IOException {
        setSelectionKey(rawChannel.register(getSelector().rawSelector(), 0));
    }

    SelectionKey getSelectionKey() {
        return selectionKey;
    }

    // Protected for tests
    protected void setSelectionKey(SelectionKey selectionKey) {
        this.selectionKey = selectionKey;
    }

    /**
     * This method cleans up any context resources that need to be released when a channel is closed. It
     * should only be called by the selector thread.
     *
     * @throws IOException during channel / context close
     */
    public void closeFromSelector() throws IOException {
        if (closeContext.isDone() == false) {
            try {
                rawChannel.close();
                closeContext.complete(null);
            } catch (Exception e) {
                closeContext.completeExceptionally(e);
                throw e;
            }
        }
    }

    /**
     * Add a listener that will be called when the channel is closed.
     *
     * @param listener to be called
     */
    public void addCloseListener(BiConsumer<Void, Throwable> listener) {
        closeContext.whenComplete(listener);
    }

    public boolean isOpen() {
        return closeContext.isDone() == false;
    }

    void handleException(Exception e) {
        exceptionHandler.accept(e);
    }

    /**
     * Schedules a channel to be closed by the selector event loop with which it is registered.
     *
     * If the channel is open and the state can be transitioned to closed, the close operation will
     * be scheduled with the event loop.
     *
     * Depending on the underlying protocol of the channel, a close operation might simply close the socket
     * channel or may involve reading and writing messages.
     */
    public abstract void closeChannel();

    public abstract ESSelector getSelector();

    public abstract NioChannel getChannel();

}
