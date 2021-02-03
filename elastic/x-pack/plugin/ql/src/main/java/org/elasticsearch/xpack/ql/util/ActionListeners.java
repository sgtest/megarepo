/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ql.util;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.CheckedFunction;

import java.util.function.Consumer;

public class ActionListeners {

    private ActionListeners() {}

    /**
     * Combination of {@link ActionListener#wrap(CheckedConsumer, Consumer)} and {@link ActionListener#map}
     */
    public static <T, Response> ActionListener<Response> map(ActionListener<T> delegate, CheckedFunction<Response, T, Exception> fn) {
        return ActionListener.wrap(r -> delegate.onResponse(fn.apply(r)), delegate::onFailure);
    }
}
