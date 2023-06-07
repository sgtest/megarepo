/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.search.action;

import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.xpack.application.utils.LicenseUtils;

import java.io.IOException;

public abstract class SearchApplicationRestHandler extends BaseRestHandler {
    protected final XPackLicenseState licenseState;

    protected SearchApplicationRestHandler(XPackLicenseState licenseState) {
        this.licenseState = licenseState;
    }

    protected final BaseRestHandler.RestChannelConsumer prepareRequest(RestRequest request, NodeClient client) throws IOException {
        RestChannelConsumer consumer = innerPrepareRequest(request, client);
        if (LicenseUtils.supportedLicense(this.licenseState)) {
            return consumer;
        } else {
            return channel -> channel.sendResponse(new RestResponse(channel, LicenseUtils.newComplianceException(this.licenseState)));
        }
    }

    /**
     * Implementers should implement this method as they normally would for
     * {@link BaseRestHandler#prepareRequest(RestRequest, NodeClient)} and ensure that all request
     * parameters are consumed prior to returning a value. The returned value is not guaranteed to
     * be executed unless search applications are available in the current license.
     */
    protected abstract RestChannelConsumer innerPrepareRequest(RestRequest request, NodeClient client) throws IOException;
}
