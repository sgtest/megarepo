/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.sql.proto;

import org.elasticsearch.common.unit.TimeValue;

import java.time.ZoneId;

/**
 * Sql protocol defaults and end-points shared between JDBC and REST protocol implementations
 */
public final class Protocol {
    /**
     * The attribute names used in the protocol request/response objects.
     */
    // requests
    public static final String QUERY_NAME = "query";
    public static final String CURSOR_NAME = "cursor"; /* request/reply */
    public static final String TIME_ZONE_NAME = "time_zone";
    public static final String FETCH_SIZE_NAME = "fetch_size";
    public static final String REQUEST_TIMEOUT_NAME = "request_timeout";
    public static final String PAGE_TIMEOUT_NAME = "page_timeout";
    public static final String FILTER_NAME = "filter";
    public static final String MODE_NAME = "mode";
    public static final String CLIENT_ID_NAME = "client_id";
    public static final String VERSION_NAME = "version";
    public static final String COLUMNAR_NAME = "columnar";
    public static final String BINARY_FORMAT_NAME = "binary_format";
    public static final String FIELD_MULTI_VALUE_LENIENCY_NAME = "field_multi_value_leniency";
    public static final String INDEX_INCLUDE_FROZEN_NAME = "index_include_frozen";
    public static final String RUNTIME_MAPPINGS_NAME = "runtime_mappings";
    // params
    public static final String PARAMS_NAME = "params";
    public static final String PARAMS_TYPE_NAME = "type";
    public static final String PARAMS_VALUE_NAME = "value";
    // responses
    public static final String COLUMNS_NAME = "columns";
    public static final String ROWS_NAME = "rows";

    public static final ZoneId TIME_ZONE = ZoneId.of("Z");

    /**
     * Global choice for the default fetch size.
     */
    public static final int FETCH_SIZE = 1000;
    public static final TimeValue REQUEST_TIMEOUT = TimeValue.timeValueSeconds(90);
    public static final TimeValue PAGE_TIMEOUT = TimeValue.timeValueSeconds(45);
    public static final boolean FIELD_MULTI_VALUE_LENIENCY = false;
    public static final boolean INDEX_INCLUDE_FROZEN = false;

    /*
     * Using the Boolean object here so that SqlTranslateRequest to set this to null (since it doesn't need a "columnar" or
     * binary parameter).
     * See {@code SqlTranslateRequest.toXContent}
     */
    public static final Boolean COLUMNAR = Boolean.FALSE;
    public static final Boolean BINARY_COMMUNICATION = null;

    /*
     * URL parameters
     */
    public static final String URL_PARAM_FORMAT = "format";
    public static final String URL_PARAM_DELIMITER = "delimiter";

    /**
     * SQL-related endpoints
     */
    public static final String CLEAR_CURSOR_REST_ENDPOINT = "/_sql/close";
    public static final String SQL_QUERY_REST_ENDPOINT = "/_sql";
    public static final String SQL_TRANSLATE_REST_ENDPOINT = "/_sql/translate";
    public static final String SQL_STATS_REST_ENDPOINT = "/_sql/stats";
}
