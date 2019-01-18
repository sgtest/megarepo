/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.test.rest;


import org.apache.http.util.EntityUtils;
import org.elasticsearch.Version;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.MlMetaIndex;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndex;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndexFields;
import org.elasticsearch.xpack.core.ml.notifications.AuditorField;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicReference;

public final class XPackRestTestHelper {

    public static final List<String> ML_PRE_V660_TEMPLATES = Collections.unmodifiableList(
            Arrays.asList(AuditorField.NOTIFICATIONS_INDEX,
                    MlMetaIndex.INDEX_NAME,
                    AnomalyDetectorsIndexFields.STATE_INDEX_PREFIX,
                    AnomalyDetectorsIndex.jobResultsIndexPrefix()));

    public static final List<String> ML_POST_V660_TEMPLATES = Collections.unmodifiableList(
            Arrays.asList(AuditorField.NOTIFICATIONS_INDEX,
                    MlMetaIndex.INDEX_NAME,
                    AnomalyDetectorsIndexFields.STATE_INDEX_PREFIX,
                    AnomalyDetectorsIndex.jobResultsIndexPrefix(),
                    AnomalyDetectorsIndex.configIndexName()));

    private XPackRestTestHelper() {
    }

    /**
     * For each template name wait for the template to be created and
     * for the template version to be equal to the master node version.
     *
     * @param client            The rest client
     * @param templateNames     Names of the templates to wait for
     * @throws InterruptedException If the wait is interrupted
     */
    public static void waitForTemplates(RestClient client, List<String> templateNames) throws InterruptedException {
        AtomicReference<Version> masterNodeVersion = new AtomicReference<>();
        ESTestCase.awaitBusy(() -> {
            String response;
            try {
                Request request = new Request("GET", "/_cat/nodes");
                request.addParameter("h", "master,version");
                response = EntityUtils.toString(client.performRequest(request).getEntity());
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
            for (String line : response.split("\n")) {
                if (line.startsWith("*")) {
                    masterNodeVersion.set(Version.fromString(line.substring(2).trim()));
                    return true;
                }
            }
            return false;
        });

        for (String template : templateNames) {
            ESTestCase.awaitBusy(() -> {
                Map<?, ?> response;
                try {
                    String string = EntityUtils.toString(client.performRequest(new Request("GET", "/_template/" + template)).getEntity());
                    response = XContentHelper.convertToMap(JsonXContent.jsonXContent, string, false);
                } catch (ResponseException e) {
                    if (e.getResponse().getStatusLine().getStatusCode() == 404) {
                        return false;
                    }
                    throw new RuntimeException(e);
                } catch (IOException e) {
                    throw new RuntimeException(e);
                }
                Map<?, ?> templateDefinition = (Map<?, ?>) response.get(template);
                return Version.fromId((Integer) templateDefinition.get("version")).equals(masterNodeVersion.get());
            });
        }
    }
}
