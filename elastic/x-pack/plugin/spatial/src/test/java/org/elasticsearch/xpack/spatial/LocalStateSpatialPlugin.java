/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.spatial;

import org.apache.lucene.util.LuceneTestCase;
import org.elasticsearch.license.License;
import org.elasticsearch.license.TestUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.test.VersionUtils;

/**
 * This class overrides the {@link SpatialPlugin} in order
 * to provide the integration test clusters a hook into a real
 * {@link XPackLicenseState}. In the cases that this is used, the
 * actual license's operation mode is not important
 */
public class LocalStateSpatialPlugin extends SpatialPlugin {
    protected XPackLicenseState getLicenseState() {
        TestUtils.UpdatableLicenseState licenseState = new TestUtils.UpdatableLicenseState();
        License.OperationMode operationMode = License.OperationMode.TRIAL;
        licenseState.update(operationMode, true, VersionUtils.randomVersion(LuceneTestCase.random()));
        return licenseState;
    }
}
