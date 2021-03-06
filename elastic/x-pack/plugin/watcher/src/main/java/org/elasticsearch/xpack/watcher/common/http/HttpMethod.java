/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.watcher.common.http;

import java.util.Locale;

public enum HttpMethod {

    HEAD("HEAD"),
    GET("GET"),
    POST("POST"),
    PUT("PUT"),
    DELETE("DELETE");

    private final String method;

    HttpMethod(String method) {
        this.method = method;
    }

    public String method() {
        return method;
    }

    public static HttpMethod parse(String value) {
        value = value.toUpperCase(Locale.ROOT);
        return switch (value) {
            case "HEAD" -> HEAD;
            case "GET" -> GET;
            case "POST" -> POST;
            case "PUT" -> PUT;
            case "DELETE" -> DELETE;
            default -> throw new IllegalArgumentException("unsupported http method [" + value + "]");
        };
    }

    public String value() {
        return name().toLowerCase(Locale.ROOT);
    }
}
