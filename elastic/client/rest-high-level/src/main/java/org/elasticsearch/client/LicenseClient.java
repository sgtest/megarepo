/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client;

import org.apache.http.HttpEntity;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.Streams;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.protocol.xpack.license.DeleteLicenseRequest;
import org.elasticsearch.protocol.xpack.license.GetLicenseRequest;
import org.elasticsearch.protocol.xpack.license.GetLicenseResponse;
import org.elasticsearch.protocol.xpack.license.PutLicenseRequest;
import org.elasticsearch.protocol.xpack.license.PutLicenseResponse;

import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;

import static java.util.Collections.emptySet;

/**
 * A wrapper for the {@link RestHighLevelClient} that provides methods for
 * accessing the Elastic License-related methods
 * <p>
 * See the <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/licensing-apis.html">
 * X-Pack Licensing APIs on elastic.co</a> for more information.
 */
public final class LicenseClient {

    private final RestHighLevelClient restHighLevelClient;

    LicenseClient(RestHighLevelClient restHighLevelClient) {
        this.restHighLevelClient = restHighLevelClient;
    }

    /**
     * Updates license for the cluster.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public PutLicenseResponse putLicense(PutLicenseRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, RequestConverters::putLicense, options,
            PutLicenseResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously updates license for the cluster.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void putLicenseAsync(PutLicenseRequest request, RequestOptions options, ActionListener<PutLicenseResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, RequestConverters::putLicense, options,
            PutLicenseResponse::fromXContent, listener, emptySet());
    }

    /**
     * Returns the current license for the cluster.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public GetLicenseResponse getLicense(GetLicenseRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequest(request, RequestConverters::getLicense, options,
            response -> new GetLicenseResponse(convertResponseToJson(response)), emptySet());
    }

    /**
     * Asynchronously returns the current license for the cluster cluster.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void getLicenseAsync(GetLicenseRequest request, RequestOptions options, ActionListener<GetLicenseResponse> listener) {
        restHighLevelClient.performRequestAsync(request, RequestConverters::getLicense, options,
            response -> new GetLicenseResponse(convertResponseToJson(response)), listener, emptySet());
    }

    /**
     * Deletes license from the cluster.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public AcknowledgedResponse deleteLicense(DeleteLicenseRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, RequestConverters::deleteLicense, options,
            AcknowledgedResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously deletes license from the cluster.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void deleteLicenseAsync(DeleteLicenseRequest request, RequestOptions options, ActionListener<AcknowledgedResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, RequestConverters::deleteLicense, options,
            AcknowledgedResponse::fromXContent, listener, emptySet());
    }

    /**
     * Converts an entire response into a json string
     *
     * This is useful for responses that we don't parse on the client side, but instead work as string
     * such as in case of the license JSON
     */
    static String convertResponseToJson(Response response) throws IOException {
        HttpEntity entity = response.getEntity();
        if (entity == null) {
            throw new IllegalStateException("Response body expected but not returned");
        }
        if (entity.getContentType() == null) {
            throw new IllegalStateException("Elasticsearch didn't return the [Content-Type] header, unable to parse response body");
        }
        XContentType xContentType = XContentType.fromMediaTypeOrFormat(entity.getContentType().getValue());
        if (xContentType == null) {
            throw new IllegalStateException("Unsupported Content-Type: " + entity.getContentType().getValue());
        }
        if (xContentType == XContentType.JSON) {
            // No changes is required
            return Streams.copyToString(new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8));
        } else {
            // Need to convert into JSON
            try (InputStream stream = response.getEntity().getContent();
                 XContentParser parser = XContentFactory.xContent(xContentType).createParser(NamedXContentRegistry.EMPTY,
                     DeprecationHandler.THROW_UNSUPPORTED_OPERATION, stream)) {
                parser.nextToken();
                XContentBuilder builder = XContentFactory.jsonBuilder();
                builder.copyCurrentStructure(parser);
                return Strings.toString(builder);
            }
        }
    }
}
