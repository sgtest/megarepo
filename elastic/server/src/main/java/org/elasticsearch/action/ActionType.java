/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action;

import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.common.io.stream.Writeable;

/**
 * A generic action. Should strive to make it a singleton.
 */
public class ActionType<Response extends ActionResponse> {

    private final String name;
    private final Writeable.Reader<Response> responseReader;

    public static <T extends ActionResponse> ActionType<T> localOnly(String name) {
        return new ActionType<>(name, Writeable.Reader.localOnly());
    }

    public static ActionType<ActionResponse.Empty> emptyResponse(String name) {
        return new ActionType<>(name, in -> ActionResponse.Empty.INSTANCE);
    }

    public static ActionType<AcknowledgedResponse> acknowledgedResponse(String name) {
        return new ActionType<>(name, AcknowledgedResponse::readFrom);
    }

    /**
     * @param name The name of the action, must be unique across actions.
     * @param responseReader A reader for the response type
     */
    public ActionType(String name, Writeable.Reader<Response> responseReader) {
        this.name = name;
        this.responseReader = responseReader;
    }

    /**
     * The name of the action. Must be unique across actions.
     */
    public String name() {
        return this.name;
    }

    /**
     * Get a reader that can read a response from a {@link org.elasticsearch.common.io.stream.StreamInput}.
     */
    public Writeable.Reader<Response> getResponseReader() {
        return responseReader;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof ActionType<?> actionType && name.equals(actionType.name);
    }

    @Override
    public int hashCode() {
        return name.hashCode();
    }

    @Override
    public String toString() {
        return name;
    }
}
