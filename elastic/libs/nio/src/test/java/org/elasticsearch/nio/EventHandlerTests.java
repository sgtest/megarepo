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

import org.elasticsearch.test.ESTestCase;
import org.junit.Before;

import java.io.IOException;
import java.net.ServerSocket;
import java.nio.channels.CancelledKeyException;
import java.nio.channels.SelectionKey;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.util.ArrayList;
import java.util.Collections;
import java.util.function.Consumer;

import static org.mockito.Matchers.same;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class EventHandlerTests extends ESTestCase {

    private Consumer<Exception> channelExceptionHandler;
    private Consumer<Exception> genericExceptionHandler;

    private ReadWriteHandler readWriteHandler;
    private EventHandler handler;
    private DoNotRegisterSocketContext context;
    private DoNotRegisterServerContext serverContext;
    private ChannelFactory<NioServerSocketChannel, NioSocketChannel> channelFactory;
    private RoundRobinSupplier<NioSelector> selectorSupplier;

    @Before
    @SuppressWarnings("unchecked")
    public void setUpHandler() throws IOException {
        channelExceptionHandler = mock(Consumer.class);
        genericExceptionHandler = mock(Consumer.class);
        readWriteHandler = mock(ReadWriteHandler.class);
        channelFactory = mock(ChannelFactory.class);
        NioSelector selector = mock(NioSelector.class);
        ArrayList<NioSelector> selectors = new ArrayList<>();
        selectors.add(selector);
        selectorSupplier = new RoundRobinSupplier<>(selectors.toArray(new NioSelector[0]));
        handler = new EventHandler(genericExceptionHandler, selectorSupplier);

        SocketChannel rawChannel = mock(SocketChannel.class);
        when(rawChannel.finishConnect()).thenReturn(true);
        NioSocketChannel channel = new NioSocketChannel(rawChannel);
        context = new DoNotRegisterSocketContext(channel, selector, channelExceptionHandler, readWriteHandler);
        channel.setContext(context);
        handler.handleRegistration(context);

        ServerSocketChannel serverSocketChannel = mock(ServerSocketChannel.class);
        when(serverSocketChannel.socket()).thenReturn(mock(ServerSocket.class));
        NioServerSocketChannel serverChannel = new NioServerSocketChannel(serverSocketChannel);
        serverContext = new DoNotRegisterServerContext(serverChannel, mock(NioSelector.class), mock(Consumer.class));
        serverChannel.setContext(serverContext);

        when(selector.isOnCurrentThread()).thenReturn(true);
    }

    public void testRegisterCallsContext() throws IOException {
        NioSocketChannel channel = mock(NioSocketChannel.class);
        SocketChannelContext channelContext = mock(SocketChannelContext.class);
        when(channel.getContext()).thenReturn(channelContext);
        when(channelContext.getSelectionKey()).thenReturn(new TestSelectionKey(0));
        handler.handleRegistration(channelContext);
        verify(channelContext).register();
    }

    public void testRegisterNonServerAddsOP_CONNECTAndOP_READInterest() throws IOException {
        SocketChannelContext context = mock(SocketChannelContext.class);
        when(context.getSelectionKey()).thenReturn(new TestSelectionKey(0));
        handler.handleRegistration(context);
        assertEquals(SelectionKey.OP_READ | SelectionKey.OP_CONNECT, context.getSelectionKey().interestOps());
    }

    public void testRegisterAddsAttachment() throws IOException {
        ChannelContext<?> context = randomBoolean() ?  mock(SocketChannelContext.class) : mock(ServerChannelContext.class);
        when(context.getSelectionKey()).thenReturn(new TestSelectionKey(0));
        handler.handleRegistration(context);
        assertEquals(context, context.getSelectionKey().attachment());
    }

    public void testHandleServerRegisterSetsOP_ACCEPTInterest() throws IOException {
        assertNull(serverContext.getSelectionKey());

        handler.handleRegistration(serverContext);

        assertEquals(SelectionKey.OP_ACCEPT, serverContext.getSelectionKey().interestOps());
    }

    public void testHandleAcceptCallsChannelFactory() throws IOException {
        NioSocketChannel childChannel = new NioSocketChannel(mock(SocketChannel.class));
        NioSocketChannel nullChannel = null;
        when(channelFactory.acceptNioChannel(same(serverContext), same(selectorSupplier))).thenReturn(childChannel, nullChannel);

        handler.acceptChannel(serverContext);

        verify(channelFactory, times(2)).acceptNioChannel(same(serverContext), same(selectorSupplier));
    }

    public void testHandleAcceptCallsServerAcceptCallback() throws IOException {
        NioSocketChannel childChannel = new NioSocketChannel(mock(SocketChannel.class));
        SocketChannelContext childContext = mock(SocketChannelContext.class);
        childChannel.setContext(childContext);
        ServerChannelContext serverChannelContext = mock(ServerChannelContext.class);
        when(channelFactory.acceptNioChannel(same(serverContext), same(selectorSupplier))).thenReturn(childChannel);

        handler.acceptChannel(serverChannelContext);

        verify(serverChannelContext).acceptChannels(selectorSupplier);
    }

    public void testAcceptExceptionCallsExceptionHandler() throws IOException {
        ServerChannelContext serverChannelContext = mock(ServerChannelContext.class);
        IOException exception = new IOException();
        handler.acceptException(serverChannelContext, exception);

        verify(serverChannelContext).handleException(exception);
    }

    public void testRegisterWithPendingWritesAddsOP_CONNECTAndOP_READAndOP_WRITEInterest() throws IOException {
        FlushReadyWrite flushReadyWrite = mock(FlushReadyWrite.class);
        when(readWriteHandler.writeToBytes(flushReadyWrite)).thenReturn(Collections.singletonList(flushReadyWrite));
        context.queueWriteOperation(flushReadyWrite);
        handler.handleRegistration(context);
        assertEquals(SelectionKey.OP_READ | SelectionKey.OP_CONNECT | SelectionKey.OP_WRITE, context.getSelectionKey().interestOps());
    }

    public void testRegistrationExceptionCallsExceptionHandler() throws IOException {
        CancelledKeyException exception = new CancelledKeyException();
        handler.registrationException(context, exception);
        verify(channelExceptionHandler).accept(exception);
    }

    public void testConnectDoesNotRemoveOP_CONNECTInterestIfIncomplete() throws IOException {
        SelectionKeyUtils.setConnectAndReadInterested(context.getSelectionKey());
        handler.handleConnect(context);
        assertEquals(SelectionKey.OP_READ, context.getSelectionKey().interestOps());
    }

    public void testConnectRemovesOP_CONNECTInterestIfComplete() throws IOException {
        SelectionKeyUtils.setConnectAndReadInterested(context.getSelectionKey());
        handler.handleConnect(context);
        assertEquals(SelectionKey.OP_READ, context.getSelectionKey().interestOps());
    }

    public void testConnectExceptionCallsExceptionHandler() throws IOException {
        IOException exception = new IOException();
        handler.connectException(context, exception);
        verify(channelExceptionHandler).accept(exception);
    }

    public void testHandleReadDelegatesToContext() throws IOException {
        SocketChannelContext context = mock(SocketChannelContext.class);
        handler.handleRead(context);
        verify(context).read();
    }

    public void testReadExceptionCallsExceptionHandler() {
        IOException exception = new IOException();
        handler.readException(context, exception);
        verify(channelExceptionHandler).accept(exception);
    }

    public void testWriteExceptionCallsExceptionHandler() {
        IOException exception = new IOException();
        handler.writeException(context, exception);
        verify(channelExceptionHandler).accept(exception);
    }

    public void testPostHandlingCallWillCloseTheChannelIfReady() throws IOException {
        NioSocketChannel channel = mock(NioSocketChannel.class);
        SocketChannelContext context = mock(SocketChannelContext.class);

        when(channel.getContext()).thenReturn(context);
        when(context.selectorShouldClose()).thenReturn(true);
        handler.postHandling(context);

        verify(context).closeFromSelector();
    }

    public void testPostHandlingCallWillNotCloseTheChannelIfNotReady() throws IOException {
        SocketChannelContext context = mock(SocketChannelContext.class);
        when(context.getSelectionKey()).thenReturn(new TestSelectionKey(SelectionKey.OP_READ | SelectionKey.OP_WRITE));
        when(context.selectorShouldClose()).thenReturn(false);

        NioSocketChannel channel = mock(NioSocketChannel.class);
        when(channel.getContext()).thenReturn(context);

        handler.postHandling(context);

        verify(context, times(0)).closeFromSelector();
    }

    public void testPostHandlingWillAddWriteIfNecessary() throws IOException {
        TestSelectionKey selectionKey = new TestSelectionKey(SelectionKey.OP_READ);
        SocketChannelContext context = mock(SocketChannelContext.class);
        when(context.getSelectionKey()).thenReturn(selectionKey);
        when(context.readyForFlush()).thenReturn(true);

        NioSocketChannel channel = mock(NioSocketChannel.class);
        when(channel.getContext()).thenReturn(context);

        assertEquals(SelectionKey.OP_READ, selectionKey.interestOps());
        handler.postHandling(context);
        assertEquals(SelectionKey.OP_READ | SelectionKey.OP_WRITE, selectionKey.interestOps());
    }

    public void testPostHandlingWillRemoveWriteIfNecessary() throws IOException {
        TestSelectionKey key = new TestSelectionKey(SelectionKey.OP_READ | SelectionKey.OP_WRITE);
        SocketChannelContext context = mock(SocketChannelContext.class);
        when(context.getSelectionKey()).thenReturn(key);
        when(context.readyForFlush()).thenReturn(false);

        NioSocketChannel channel = mock(NioSocketChannel.class);
        when(channel.getContext()).thenReturn(context);


        assertEquals(SelectionKey.OP_READ | SelectionKey.OP_WRITE, key.interestOps());
        handler.postHandling(context);
        assertEquals(SelectionKey.OP_READ, key.interestOps());
    }

    public void testListenerExceptionCallsGenericExceptionHandler() throws IOException {
        RuntimeException listenerException = new RuntimeException();
        handler.taskException(listenerException);
        verify(genericExceptionHandler).accept(listenerException);
    }

    private class DoNotRegisterSocketContext extends BytesChannelContext {


        DoNotRegisterSocketContext(NioSocketChannel channel, NioSelector selector, Consumer<Exception> exceptionHandler,
                                   ReadWriteHandler handler) {
            super(channel, selector, exceptionHandler, handler, InboundChannelBuffer.allocatingInstance());
        }

        @Override
        public void register() {
            setSelectionKey(new TestSelectionKey(0));
        }
    }

    private class DoNotRegisterServerContext extends ServerChannelContext {


        @SuppressWarnings("unchecked")
        DoNotRegisterServerContext(NioServerSocketChannel channel, NioSelector selector, Consumer<NioSocketChannel> acceptor) {
            super(channel, channelFactory, selector, acceptor, mock(Consumer.class));
        }

        @Override
        public void register() {
            setSelectionKey(new TestSelectionKey(0));
        }
    }
}
