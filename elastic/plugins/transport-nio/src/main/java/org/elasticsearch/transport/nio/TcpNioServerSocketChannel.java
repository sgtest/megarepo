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

package org.elasticsearch.transport.nio;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.nio.AcceptingSelector;
import org.elasticsearch.nio.ChannelFactory;
import org.elasticsearch.nio.NioServerSocketChannel;
import org.elasticsearch.transport.TcpChannel;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.channels.ServerSocketChannel;

/**
 * This is an implementation of {@link NioServerSocketChannel} that adheres to the {@link TcpChannel}
 * interface. As it is a server socket, setting SO_LINGER and sending messages is not supported.
 */
public class TcpNioServerSocketChannel extends NioServerSocketChannel implements TcpChannel {

    private final String profile;

    public TcpNioServerSocketChannel(String profile, ServerSocketChannel socketChannel) throws IOException {
        super(socketChannel);
        this.profile = profile;
    }

    @Override
    public void sendMessage(BytesReference reference, ActionListener<Void> listener) {
        throw new UnsupportedOperationException("Cannot send a message to a server channel.");
    }

    @Override
    public void setSoLinger(int value) throws IOException {
        throw new UnsupportedOperationException("Cannot set SO_LINGER on a server channel.");
    }

    @Override
    public InetSocketAddress getRemoteAddress() {
        return null;
    }

    @Override
    public void close() {
        getContext().closeChannel();
    }

    @Override
    public String getProfile() {
        return profile;
    }

    @Override
    public void addCloseListener(ActionListener<Void> listener) {
        addCloseListener(ActionListener.toBiConsumer(listener));
    }

    @Override
    public String toString() {
        return "TcpNioServerSocketChannel{" +
            "localAddress=" + getLocalAddress() +
            '}';
    }
}
