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

package org.elasticsearch.http.nio;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.ChannelFuture;
import io.netty.channel.ChannelHandler;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelOutboundHandlerAdapter;
import io.netty.channel.ChannelPromise;
import io.netty.channel.embedded.EmbeddedChannel;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.nio.FlushOperation;
import org.elasticsearch.nio.WriteOperation;

import java.nio.ByteBuffer;
import java.util.LinkedList;
import java.util.function.BiConsumer;

public class NettyAdaptor implements AutoCloseable {

    private final EmbeddedChannel nettyChannel;
    private final LinkedList<FlushOperation> flushOperations = new LinkedList<>();

    NettyAdaptor(ChannelHandler... handlers) {
        nettyChannel = new EmbeddedChannel();
        nettyChannel.pipeline().addLast("write_captor", new ChannelOutboundHandlerAdapter() {

            @Override
            public void write(ChannelHandlerContext ctx, Object msg, ChannelPromise promise) {
                // This is a little tricky. The embedded channel will complete the promise once it writes the message
                // to its outbound buffer. We do not want to complete the promise until the message is sent. So we
                // intercept the promise and pass a different promise back to the rest of the pipeline.

                try {
                    ByteBuf message = (ByteBuf) msg;
                    promise.addListener((f) -> message.release());
                    NettyListener listener;
                    if (promise instanceof NettyListener) {
                        listener = (NettyListener) promise;
                    } else {
                        listener = new NettyListener(promise);
                    }
                    flushOperations.add(new FlushOperation(message.nioBuffers(), listener));
                } catch (Exception e) {
                    promise.setFailure(e);
                }
            }
        });
        nettyChannel.pipeline().addLast(handlers);
    }

    @Override
    public void close() throws Exception {
        assert flushOperations.isEmpty() : "Should close outbound operations before calling close";

        ChannelFuture closeFuture = nettyChannel.close();
        // This should be safe as we are not a real network channel
        closeFuture.await();
        if (closeFuture.isSuccess() == false) {
            Throwable cause = closeFuture.cause();
            ExceptionsHelper.dieOnError(cause);
            throw (Exception) cause;
        }
    }

    public void addCloseListener(BiConsumer<Void, Exception> listener) {
        nettyChannel.closeFuture().addListener(f -> {
            if (f.isSuccess()) {
                listener.accept(null, null);
            } else {
                final Throwable cause = f.cause();
                ExceptionsHelper.dieOnError(cause);
                assert cause instanceof Exception;
                listener.accept(null, (Exception) cause);
            }
        });
    }

    public int read(ByteBuffer[] buffers) {
        ByteBuf byteBuf = Unpooled.wrappedBuffer(buffers);
        int initialReaderIndex = byteBuf.readerIndex();
        nettyChannel.writeInbound(byteBuf);
        return byteBuf.readerIndex() - initialReaderIndex;
    }

    public Object pollInboundMessage() {
        return nettyChannel.readInbound();
    }

    public void write(WriteOperation writeOperation) {
        ChannelPromise channelPromise = nettyChannel.newPromise();
        channelPromise.addListener(f -> {
            BiConsumer<Void, Throwable> consumer = writeOperation.getListener();
            if (f.cause() == null) {
                consumer.accept(null, null);
            } else {
                ExceptionsHelper.dieOnError(f.cause());
                consumer.accept(null, f.cause());
            }
        });

        nettyChannel.writeAndFlush(writeOperation.getObject(), new NettyListener(channelPromise));
    }

    public FlushOperation pollOutboundOperation() {
        return flushOperations.pollFirst();
    }

    public int getOutboundCount() {
        return flushOperations.size();
    }
}
