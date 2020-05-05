/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.eql.parser;

import java.time.ZoneId;
import java.util.List;

import static java.util.Collections.emptyList;
import static org.elasticsearch.xpack.eql.action.RequestDefaults.FIELD_EVENT_CATEGORY;
import static org.elasticsearch.xpack.eql.action.RequestDefaults.FIELD_TIMESTAMP;
import static org.elasticsearch.xpack.eql.action.RequestDefaults.FIELD_IMPLICIT_JOIN_KEY;

public class ParserParams {

    private final ZoneId zoneId;
    private String fieldEventCategory = FIELD_EVENT_CATEGORY;
    private String fieldTimestamp = FIELD_TIMESTAMP;
    private String implicitJoinKey = FIELD_IMPLICIT_JOIN_KEY;
    private List<Object> queryParams = emptyList();

    public ParserParams(ZoneId zoneId) {
        this.zoneId = zoneId;
    }
    
    public String fieldEventCategory() {
        return fieldEventCategory;
    }

    public ParserParams fieldEventCategory(String fieldEventCategory) {
        this.fieldEventCategory = fieldEventCategory;
        return this;
    }

    public String fieldTimestamp() {
        return fieldTimestamp;
    }

    public ParserParams fieldTimestamp(String fieldTimestamp) {
        this.fieldTimestamp = fieldTimestamp;
        return this;
    }

    public String implicitJoinKey() {
        return implicitJoinKey;
    }

    public ParserParams implicitJoinKey(String implicitJoinKey) {
        this.implicitJoinKey = implicitJoinKey;
        return this;
    }

    public List<Object> params() {
        return queryParams;
    }

    public ParserParams params(List<Object> params) {
        this.queryParams = params;
        return this;
    }

    public ZoneId zoneId() {
        return zoneId;
    }
}
