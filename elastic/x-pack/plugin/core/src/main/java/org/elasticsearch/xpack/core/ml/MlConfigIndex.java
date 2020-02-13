/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml;

import org.elasticsearch.Version;
import org.elasticsearch.xpack.core.template.TemplateUtils;

public class MlConfigIndex {

    private static final String MAPPINGS_VERSION_VARIABLE = "xpack.ml.version";

    private MlConfigIndex() {}

    public static String mapping() {
        return TemplateUtils.loadTemplate("/org/elasticsearch/xpack/core/ml/config_index_mappings.json",
            Version.CURRENT.toString(), MAPPINGS_VERSION_VARIABLE);
    }
}
