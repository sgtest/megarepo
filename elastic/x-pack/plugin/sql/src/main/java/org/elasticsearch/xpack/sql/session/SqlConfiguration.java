/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.session;

import org.elasticsearch.Version;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.xpack.sql.proto.Mode;
import org.elasticsearch.xpack.sql.proto.SqlVersion;

import java.time.ZoneId;
import java.util.Map;

// Typed object holding properties for a given query
public class SqlConfiguration extends org.elasticsearch.xpack.ql.session.Configuration {

    private final int pageSize;
    private final TimeValue requestTimeout;
    private final TimeValue pageTimeout;
    private final Mode mode;
    private final String clientId;
    private final SqlVersion version;
    private final boolean multiValueFieldLeniency;
    private final boolean includeFrozenIndices;

    @Nullable
    private QueryBuilder filter;

    @Nullable
    private Map<String, Object> runtimeMappings;

    public SqlConfiguration(ZoneId zi, int pageSize, TimeValue requestTimeout, TimeValue pageTimeout, QueryBuilder filter,
                         Map<String, Object> runtimeMappings,
                         Mode mode, String clientId, SqlVersion version,
                         String username, String clusterName,
                         boolean multiValueFieldLeniency,
                         boolean includeFrozen) {

        super(zi, username, clusterName);

        this.pageSize = pageSize;
        this.requestTimeout = requestTimeout;
        this.pageTimeout = pageTimeout;
        this.filter = filter;
        this.runtimeMappings = runtimeMappings;
        this.mode = mode == null ? Mode.PLAIN : mode;
        this.clientId = clientId;
        this.version = version != null ? version : SqlVersion.fromId(Version.CURRENT.id);
        this.multiValueFieldLeniency = multiValueFieldLeniency;
        this.includeFrozenIndices = includeFrozen;
    }

    public int pageSize() {
        return pageSize;
    }

    public TimeValue requestTimeout() {
        return requestTimeout;
    }

    public TimeValue pageTimeout() {
        return pageTimeout;
    }

    public QueryBuilder filter() {
        return filter;
    }

    public Map<String, Object> runtimeMappings() {
        return runtimeMappings;
    }

    public Mode mode() {
        return mode;
    }

    public String clientId() {
        return clientId;
    }

    public boolean multiValueFieldLeniency() {
        return multiValueFieldLeniency;
    }

    public boolean includeFrozen() {
        return includeFrozenIndices;
    }

    public SqlVersion version() {
        return version;
    }
}
