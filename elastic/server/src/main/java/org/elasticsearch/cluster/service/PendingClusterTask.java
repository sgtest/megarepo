/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.service;

import org.elasticsearch.common.Priority;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.text.Text;
import org.elasticsearch.common.unit.TimeValue;

import java.io.IOException;

public class PendingClusterTask implements Writeable {

    private long insertOrder;
    private Priority priority;
    private Text source;
    private long timeInQueue;
    private boolean executing;

    public PendingClusterTask(StreamInput in) throws IOException {
        insertOrder = in.readVLong();
        priority = Priority.readFrom(in);
        source = in.readText();
        timeInQueue = in.readLong();
        executing = in.readBoolean();
    }

    public PendingClusterTask(long insertOrder, Priority priority, Text source, long timeInQueue, boolean executing) {
        assert timeInQueue >= 0 : "got a negative timeInQueue [" + timeInQueue + "]";
        assert insertOrder >= 0 : "got a negative insertOrder [" + insertOrder + "]";
        this.insertOrder = insertOrder;
        this.priority = priority;
        this.source = source;
        this.timeInQueue = timeInQueue;
        this.executing = executing;
    }

    public long getInsertOrder() {
        return insertOrder;
    }

    public Priority getPriority() {
        return priority;
    }

    public Text getSource() {
        return source;
    }

    public long getTimeInQueueInMillis() {
        return timeInQueue;
    }

    public TimeValue getTimeInQueue() {
        return new TimeValue(getTimeInQueueInMillis());
    }

    public boolean isExecuting() {
        return executing;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeVLong(insertOrder);
        Priority.writeTo(priority, out);
        out.writeText(source);
        out.writeLong(timeInQueue);
        out.writeBoolean(executing);
    }
}
