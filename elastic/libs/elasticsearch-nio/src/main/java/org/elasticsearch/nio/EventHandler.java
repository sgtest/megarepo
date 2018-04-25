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

import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;

import java.io.IOException;
import java.nio.channels.Selector;

public abstract class EventHandler {

    protected final Logger logger;

    EventHandler(Logger logger) {
        this.logger = logger;
    }

    /**
     * This method handles an IOException that was thrown during a call to {@link Selector#select(long)}.
     *
     * @param exception the exception
     */
    protected void selectException(IOException exception) {
        logger.warn(new ParameterizedMessage("io exception during select [thread={}]", Thread.currentThread().getName()), exception);
    }

    /**
     * This method handles an IOException that was thrown during a call to {@link Selector#close()}.
     *
     * @param exception the exception
     */
    protected void closeSelectorException(IOException exception) {
        logger.warn(new ParameterizedMessage("io exception while closing selector [thread={}]", Thread.currentThread().getName()),
            exception);
    }

    /**
     * This method handles an exception that was uncaught during a select loop.
     *
     * @param exception that was uncaught
     */
    protected void uncaughtException(Exception exception) {
        Thread thread = Thread.currentThread();
        thread.getUncaughtExceptionHandler().uncaughtException(thread, exception);
    }

    /**
     * This method handles the closing of an NioChannel
     *
     * @param context that should be closed
     */
    protected void handleClose(ChannelContext<?> context) {
        try {
            context.closeFromSelector();
        } catch (IOException e) {
            closeException(context, e);
        }
        assert context.isOpen() == false : "Should always be done as we are on the selector thread";
    }

    /**
     * This method is called when an attempt to close a channel throws an exception.
     *
     * @param context that was being closed
     * @param exception that occurred
     */
    protected void closeException(ChannelContext<?> context, Exception exception) {
        logger.debug(() -> new ParameterizedMessage("exception while closing channel: {}", context.getChannel()), exception);
    }

    /**
     * This method is called when handling an event from a channel fails due to an unexpected exception.
     * An example would be if checking ready ops on a {@link java.nio.channels.SelectionKey} threw
     * {@link java.nio.channels.CancelledKeyException}.
     *
     * @param channel that caused the exception
     * @param exception that was thrown
     */
    protected void genericChannelException(ChannelContext<?> channel, Exception exception) {
        logger.debug(() -> new ParameterizedMessage("exception while handling event for channel: {}", channel.getChannel()), exception);
    }
}
