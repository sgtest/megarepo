/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.ingest.common;

import java.util.Map;

/**
 * Provides definitions for Painless methods that expose ingest processor
 * functionality. Must also be explicitly allowed in:
 *
 * modules/ingest-common/src/main/resources/org/elasticsearch/ingest/common/processors_whitelist.txt
 */
public final class Processors {

    /**
     * Uses {@link BytesProcessor} to return the number of bytes in a
     * human-readable byte string such as <code>1kb</code>.
     *
     * @param value human-readable byte string
     * @return number of bytes
     */
    public static long bytes(String value) {
        return BytesProcessor.apply(value);
    }

    /**
     * Uses {@link LowercaseProcessor} to convert a string to its lowercase
     * equivalent.
     *
     * @param value string to convert
     * @return lowercase equivalent
     */
    public static String lowercase(String value) {
        return LowercaseProcessor.apply(value);
    }

    /**
     * Uses {@link UppercaseProcessor} to convert a string to its uppercase
     * equivalent.
     *
     * @param value string to convert
     * @return uppercase equivalent
     */
    public static String uppercase(String value) {
        return UppercaseProcessor.apply(value);
    }

    /**
     * Uses {@link JsonProcessor} to convert a JSON string to a structured JSON
     * object.
     *
     * @param fieldValue JSON string
     * @return structured JSON object
     */
    public static Object json(Object fieldValue) {
        return JsonProcessor.apply(fieldValue);
    }

    /**
     * Uses {@link JsonProcessor} to convert a JSON string to a structured JSON
     * object.
     *
     * @param map map that contains the JSON string and will receive the
     *            structured JSON content
     * @param field key that identifies the entry in <code>map</code> that
     *             contains the JSON string
     */
    public static void json(Map<String, Object> map, String field) {
        JsonProcessor.apply(map, field);
    }

    /**
     * Uses {@link URLDecodeProcessor} to URL-decode a string.
     *
     * @param value string to decode
     * @return URL-decoded value
     */
    public static String urlDecode(String value) {
        return URLDecodeProcessor.apply(value);
    }

}
