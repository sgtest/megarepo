/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.action.user;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.security.action.user.ChangePasswordAction;
import org.elasticsearch.xpack.core.security.action.user.ChangePasswordRequest;
import org.elasticsearch.xpack.core.security.action.user.ChangePasswordResponse;
import org.elasticsearch.xpack.core.security.user.AnonymousUser;
import org.elasticsearch.xpack.core.security.user.SystemUser;
import org.elasticsearch.xpack.core.security.user.XPackUser;
import org.elasticsearch.xpack.security.authc.esnative.NativeUsersStore;

public class TransportChangePasswordAction extends HandledTransportAction<ChangePasswordRequest, ChangePasswordResponse> {

    private final NativeUsersStore nativeUsersStore;

    @Inject
    public TransportChangePasswordAction(Settings settings, ThreadPool threadPool, TransportService transportService,
                                         ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver,
                                         NativeUsersStore nativeUsersStore) {
        super(settings, ChangePasswordAction.NAME, threadPool, transportService, actionFilters, indexNameExpressionResolver,
                ChangePasswordRequest::new);
        this.nativeUsersStore = nativeUsersStore;
    }

    @Override
    protected void doExecute(ChangePasswordRequest request, ActionListener<ChangePasswordResponse> listener) {
        final String username = request.username();
        if (AnonymousUser.isAnonymousUsername(username, settings)) {
            listener.onFailure(new IllegalArgumentException("user [" + username + "] is anonymous and cannot be modified via the API"));
            return;
        } else if (SystemUser.NAME.equals(username) || XPackUser.NAME.equals(username)) {
            listener.onFailure(new IllegalArgumentException("user [" + username + "] is internal"));
            return;
        }
        nativeUsersStore.changePassword(request, new ActionListener<Void>() {
            @Override
            public void onResponse(Void v) {
                listener.onResponse(new ChangePasswordResponse());
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }
        });
    }
}
