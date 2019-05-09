/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.test.rest;

import java.util.List;

public final class XPackRestTestConstants {

    // Watcher constants:
    public static final String INDEX_TEMPLATE_VERSION = "9";
    public static final String HISTORY_TEMPLATE_NAME_NO_ILM = ".watch-history-no-ilm-" + INDEX_TEMPLATE_VERSION;
    public static final String TRIGGERED_TEMPLATE_NAME = ".triggered_watches";
    public static final String WATCHES_TEMPLATE_NAME = ".watches";

    public static final String[] TEMPLATE_NAMES_NO_ILM = new String[] {
        HISTORY_TEMPLATE_NAME_NO_ILM, TRIGGERED_TEMPLATE_NAME, WATCHES_TEMPLATE_NAME
    };

    // ML constants:
    public static final String ML_META_INDEX_NAME = ".ml-meta";
    public static final String AUDITOR_NOTIFICATIONS_INDEX = ".ml-notifications";
    public static final String CONFIG_INDEX = ".ml-config";
    public static final String RESULTS_INDEX_PREFIX = ".ml-anomalies-";
    public static final String STATE_INDEX_PREFIX = ".ml-state";
    public static final String RESULTS_INDEX_DEFAULT = "shared";

    public static final List<String> ML_POST_V660_TEMPLATES =
        List.of(AUDITOR_NOTIFICATIONS_INDEX, ML_META_INDEX_NAME, STATE_INDEX_PREFIX, RESULTS_INDEX_PREFIX, CONFIG_INDEX);

    private XPackRestTestConstants() {
    }
}
