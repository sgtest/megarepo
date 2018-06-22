/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.action.ValidateDetectorAction;

import java.util.function.Supplier;

public class TransportValidateDetectorAction extends HandledTransportAction<ValidateDetectorAction.Request,
        ValidateDetectorAction.Response> {

    @Inject
    public TransportValidateDetectorAction(Settings settings, TransportService transportService, ActionFilters actionFilters) {
        super(settings, ValidateDetectorAction.NAME, transportService, actionFilters,
            (Supplier<ValidateDetectorAction.Request>) ValidateDetectorAction.Request::new);
    }

    @Override
    protected void doExecute(ValidateDetectorAction.Request request, ActionListener<ValidateDetectorAction.Response> listener) {
        listener.onResponse(new ValidateDetectorAction.Response(true));
    }

}
