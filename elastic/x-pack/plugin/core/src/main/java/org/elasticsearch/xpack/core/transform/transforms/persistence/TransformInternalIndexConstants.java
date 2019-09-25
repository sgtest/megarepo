/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.core.transform.transforms.persistence;

public final class TransformInternalIndexConstants {

    /* Constants for internal indexes of the transform plugin
     * (defined in core to provide wider access)
     *
     * Increase the version number for every mapping change, see TransformInternalIndex for details
     *
     * Together with increasing the version number please keep the following in sync:
     *
     *    - XPackRestTestConstants
     *    - yaml tests under x-pack/qa/
     *
     * (pro-tip: grep for the constant)
     */

    // internal index
    public static final String INDEX_VERSION = "2";
    public static final String INDEX_PATTERN = ".data-frame-internal-";
    public static final String LATEST_INDEX_VERSIONED_NAME = INDEX_PATTERN + INDEX_VERSION;
    public static final String LATEST_INDEX_NAME = LATEST_INDEX_VERSIONED_NAME;
    public static final String INDEX_NAME_PATTERN = INDEX_PATTERN + "*";

    // audit index
    public static final String AUDIT_TEMPLATE_VERSION = "1";
    public static final String AUDIT_INDEX_PREFIX = ".data-frame-notifications-";
    public static final String AUDIT_INDEX = AUDIT_INDEX_PREFIX + AUDIT_TEMPLATE_VERSION;

    private TransformInternalIndexConstants() {
    }

}
