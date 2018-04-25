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
import java.net.InetSocketAddress;
import java.nio.channels.SocketChannel;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;

public class NioSocketChannel extends NioChannel {

    private final InetSocketAddress remoteAddress;
    private final AtomicBoolean contextSet = new AtomicBoolean(false);
    private final SocketChannel socketChannel;
    private SocketChannelContext context;

    public NioSocketChannel(SocketChannel socketChannel) throws IOException {
        super(socketChannel);
        this.socketChannel = socketChannel;
        this.remoteAddress = (InetSocketAddress) socketChannel.getRemoteAddress();
    }

    public void setContext(SocketChannelContext context) {
        if (contextSet.compareAndSet(false, true)) {
            this.context = context;
        } else {
            throw new IllegalStateException("Context on this channel were already set. It should only be once.");
        }
    }

    @Override
    public SocketChannel getRawChannel() {
        return socketChannel;
    }

    @Override
    public SocketChannelContext getContext() {
        return context;
    }

    public InetSocketAddress getRemoteAddress() {
        return remoteAddress;
    }

    public void addConnectListener(BiConsumer<Void, Throwable> listener) {
        context.addConnectListener(listener);
    }

    @Override
    public String toString() {
        return "NioSocketChannel{" +
            "localAddress=" + getLocalAddress() +
            ", remoteAddress=" + remoteAddress +
            '}';
    }
}
