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
import java.nio.channels.ClosedChannelException;
import java.nio.channels.SelectionKey;
import java.nio.channels.Selector;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.stream.Collectors;

/**
 * Selector implementation that handles {@link NioServerSocketChannel}. It's main piece of functionality is
 * accepting new channels.
 */
public class AcceptingSelector extends ESSelector {

    private final AcceptorEventHandler eventHandler;
    private final ConcurrentLinkedQueue<NioServerSocketChannel> newChannels = new ConcurrentLinkedQueue<>();

    public AcceptingSelector(AcceptorEventHandler eventHandler) throws IOException {
        super(eventHandler);
        this.eventHandler = eventHandler;
    }

    public AcceptingSelector(AcceptorEventHandler eventHandler, Selector selector) throws IOException {
        super(eventHandler, selector);
        this.eventHandler = eventHandler;
    }

    @Override
    void processKey(SelectionKey selectionKey) {
        ServerChannelContext channelContext = (ServerChannelContext) selectionKey.attachment();
        if (selectionKey.isAcceptable()) {
            try {
                eventHandler.acceptChannel(channelContext);
            } catch (IOException e) {
                eventHandler.acceptException(channelContext, e);
            }
        }
    }

    @Override
    void preSelect() {
        setUpNewServerChannels();
    }

    @Override
    void cleanup() {
        channelsToClose.addAll(newChannels.stream().map(NioServerSocketChannel::getContext).collect(Collectors.toList()));
    }

    /**
     * Schedules a NioServerSocketChannel to be registered with this selector. The channel will by queued and
     * eventually registered next time through the event loop.
     *
     * @param serverSocketChannel the channel to register
     */
    public void scheduleForRegistration(NioServerSocketChannel serverSocketChannel) {
        newChannels.add(serverSocketChannel);
        ensureSelectorOpenForEnqueuing(newChannels, serverSocketChannel);
        wakeup();
    }

    private void setUpNewServerChannels() {
        NioServerSocketChannel newChannel;
        while ((newChannel = this.newChannels.poll()) != null) {
            ServerChannelContext context = newChannel.getContext();
            assert context.getSelector() == this : "The channel must be registered with the selector with which it was created";
            try {
                if (context.isOpen()) {
                    eventHandler.handleRegistration(context);
                } else {
                    eventHandler.registrationException(context, new ClosedChannelException());
                }
            } catch (Exception e) {
                eventHandler.registrationException(context, e);
            }
        }
    }
}
