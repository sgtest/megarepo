/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.transport;

import org.elasticsearch.Version;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.util.concurrent.ThreadContext;

/**
 * Represents a transport message sent over the network. Subclasses implement serialization and
 * deserialization.
 */
public abstract class NetworkMessage {

    protected final Version version;
    protected final Writeable threadContext;
    protected final long requestId;
    protected final byte status;

    NetworkMessage(ThreadContext threadContext, Version version, byte status, long requestId) {
        this.threadContext = threadContext.captureAsWriteable();
        this.version = version;
        this.requestId = requestId;
        this.status = status;
    }

    public Version getVersion() {
        return version;
    }

    public long getRequestId() {
        return requestId;
    }

    boolean isCompress() {
        return TransportStatus.isCompress(status);
    }

    boolean isResponse() {
        return TransportStatus.isRequest(status) == false;
    }

    boolean isRequest() {
        return TransportStatus.isRequest(status);
    }

    boolean isHandshake() {
        return TransportStatus.isHandshake(status);
    }

    boolean isError() {
        return TransportStatus.isError(status);
    }
}
