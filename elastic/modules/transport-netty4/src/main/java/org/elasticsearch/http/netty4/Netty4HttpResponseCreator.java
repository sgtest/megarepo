/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.http.netty4;

import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandler;
import io.netty.channel.ChannelHandlerContext;
import io.netty.handler.codec.MessageToMessageEncoder;
import io.netty.handler.codec.compression.JdkZlibEncoder;
import io.netty.handler.codec.http.DefaultHttpContent;
import io.netty.handler.codec.http.DefaultHttpResponse;
import io.netty.handler.codec.http.DefaultLastHttpContent;
import io.netty.handler.codec.http.HttpResponse;
import org.elasticsearch.common.Booleans;
import org.elasticsearch.transport.NettyAllocator;

import java.util.List;

/**
 * Split up large responses to prevent batch compression {@link JdkZlibEncoder} down the pipeline.
 */
@ChannelHandler.Sharable
class Netty4HttpResponseCreator extends MessageToMessageEncoder<Netty4HttpResponse> {

    private static final String DO_NOT_SPLIT = "es.unsafe.do_not_split_http_responses";

    private static final boolean DO_NOT_SPLIT_HTTP_RESPONSES;
    private static final int SPLIT_THRESHOLD;

    static {
        DO_NOT_SPLIT_HTTP_RESPONSES = Booleans.parseBoolean(System.getProperty(DO_NOT_SPLIT), false);
        // Netty will add some header bytes if it compresses this message. So we downsize slightly.
        SPLIT_THRESHOLD = (int) (NettyAllocator.suggestedMaxAllocationSize() * 0.99);
    }

    @Override
    protected void encode(ChannelHandlerContext ctx, Netty4HttpResponse msg, List<Object> out) {
        if (DO_NOT_SPLIT_HTTP_RESPONSES || msg.content().readableBytes() <= SPLIT_THRESHOLD) {
            out.add(msg.retain());
        } else {
            HttpResponse response = new DefaultHttpResponse(msg.protocolVersion(), msg.status(), msg.headers());
            out.add(response);
            ByteBuf content = msg.content();
            while (content.readableBytes() > SPLIT_THRESHOLD) {
                out.add(new DefaultHttpContent(content.readRetainedSlice(SPLIT_THRESHOLD)));
            }
            out.add(new DefaultLastHttpContent(content.readRetainedSlice(content.readableBytes())));
        }
    }
}
