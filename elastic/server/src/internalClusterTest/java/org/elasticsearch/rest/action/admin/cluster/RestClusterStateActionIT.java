/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest.action.admin.cluster;

import org.elasticsearch.client.Request;
import org.elasticsearch.test.ESIntegTestCase;

import java.io.IOException;

public class RestClusterStateActionIT extends ESIntegTestCase {

    @Override
    protected boolean addMockHttpTransport() {
        return false;
    }

    public void testInfiniteTimeOut() throws IOException {
        final var request = new Request("GET", "/_cluster/state/none");
        request.addParameter("master_timeout", "-1");
        getRestClient().performRequest(request);
    }
}
