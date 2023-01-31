/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.support;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.transport.TransportChannel;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.transport.TransportResponse;

import static org.elasticsearch.core.Strings.format;

public final class ChannelActionListener<Response extends TransportResponse, Request extends TransportRequest>
    implements
        ActionListener<Response> {

    private static final Logger logger = LogManager.getLogger(ChannelActionListener.class);

    private final TransportChannel channel;
    private final Request request;
    private final String actionName;

    public ChannelActionListener(TransportChannel channel, String actionName, Request request) {
        this.channel = channel;
        this.request = request;
        this.actionName = actionName;
    }

    @Override
    public void onResponse(Response response) {
        ActionListener.run(this, l -> l.channel.sendResponse(response));
    }

    @Override
    public void onFailure(Exception e) {
        try {
            channel.sendResponse(e);
        } catch (Exception sendException) {
            sendException.addSuppressed(e);
            logger.warn(() -> format("Failed to send error response for action [%s] and request [%s]", actionName, request), sendException);
        }
    }

    @Override
    public String toString() {
        return "ChannelActionListener{" + channel + "}{" + request + "}{" + actionName + "}";
    }
}
