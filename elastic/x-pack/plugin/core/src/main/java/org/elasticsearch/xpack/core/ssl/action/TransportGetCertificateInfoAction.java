/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ssl.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.core.ssl.cert.CertificateInfo;

import java.io.IOException;
import java.security.GeneralSecurityException;
import java.util.Collection;

public class TransportGetCertificateInfoAction extends HandledTransportAction<GetCertificateInfoAction.Request,
        GetCertificateInfoAction.Response> {

    private final SSLService sslService;

    @Inject
    public TransportGetCertificateInfoAction(Settings settings, ThreadPool threadPool,
                                             TransportService transportService, ActionFilters actionFilters,
                                             IndexNameExpressionResolver indexNameExpressionResolver,
                                             SSLService sslService) {
        super(settings, GetCertificateInfoAction.NAME, threadPool, transportService, actionFilters,
                indexNameExpressionResolver, GetCertificateInfoAction.Request::new);
        this.sslService = sslService;
    }

    @Override
    protected void doExecute(GetCertificateInfoAction.Request request,
                             ActionListener<GetCertificateInfoAction.Response> listener) {
        try {
            Collection<CertificateInfo> certificates = sslService.getLoadedCertificates();
            listener.onResponse(new GetCertificateInfoAction.Response(certificates));
        } catch (GeneralSecurityException | IOException e) {
            listener.onFailure(e);
        }
    }
}
