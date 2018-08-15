/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.watcher.transport.actions.service;

import org.elasticsearch.action.Action;
import org.elasticsearch.action.support.master.AcknowledgedResponse;


public class WatcherServiceAction extends Action<AcknowledgedResponse> {

    public static final WatcherServiceAction INSTANCE = new WatcherServiceAction();
    public static final String NAME = "cluster:admin/xpack/watcher/service";

    private WatcherServiceAction() {
        super(NAME);
    }

    @Override
    public AcknowledgedResponse newResponse() {
        return new AcknowledgedResponse();
    }
}
