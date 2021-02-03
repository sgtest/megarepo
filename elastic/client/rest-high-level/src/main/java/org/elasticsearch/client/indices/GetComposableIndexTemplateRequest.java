/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.client.indices;

import org.elasticsearch.client.TimedRequest;
import org.elasticsearch.client.Validatable;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.unit.TimeValue;

/**
 * A request to read the content of index templates
 */
public class GetComposableIndexTemplateRequest implements Validatable {

    private final String name;

    private TimeValue masterNodeTimeout = TimedRequest.DEFAULT_MASTER_NODE_TIMEOUT;
    private boolean local = false;

    /**
     * Create a request to read the content of index template. If no template name is provided, all templates will
     * be read
     *
     * @param name the name of template to read
     */
    public GetComposableIndexTemplateRequest(String name) {
        this.name = name;
    }

    /**
     * @return the name of index template this request is requesting
     */
    public String name() {
        return name;
    }

    /**
     * @return the timeout for waiting for the master node to respond
     */
    public TimeValue getMasterNodeTimeout() {
        return masterNodeTimeout;
    }

    public void setMasterNodeTimeout(@Nullable TimeValue masterNodeTimeout) {
        this.masterNodeTimeout = masterNodeTimeout;
    }

    public void setMasterNodeTimeout(String masterNodeTimeout) {
        final TimeValue timeValue = TimeValue.parseTimeValue(masterNodeTimeout, getClass().getSimpleName() + ".masterNodeTimeout");
        setMasterNodeTimeout(timeValue);
    }

    /**
     * @return true if this request is to read from the local cluster state, rather than the master node - false otherwise
     */
    public boolean isLocal() {
        return local;
    }

    public void setLocal(boolean local) {
        this.local = local;
    }
}
