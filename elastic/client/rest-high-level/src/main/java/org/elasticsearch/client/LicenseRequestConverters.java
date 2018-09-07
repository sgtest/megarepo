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

import org.apache.http.client.methods.HttpDelete;
import org.apache.http.client.methods.HttpGet;
import org.apache.http.client.methods.HttpPut;
import org.elasticsearch.protocol.xpack.license.DeleteLicenseRequest;
import org.elasticsearch.protocol.xpack.license.GetLicenseRequest;
import org.elasticsearch.protocol.xpack.license.PutLicenseRequest;

public class LicenseRequestConverters {
    static Request putLicense(PutLicenseRequest putLicenseRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("license")
            .build();
        Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        RequestConverters.Params parameters = new RequestConverters.Params(request);
        parameters.withTimeout(putLicenseRequest.timeout());
        parameters.withMasterTimeout(putLicenseRequest.masterNodeTimeout());
        if (putLicenseRequest.isAcknowledge()) {
            parameters.putParam("acknowledge", "true");
        }
        request.setJsonEntity(putLicenseRequest.getLicenseDefinition());
        return request;
    }

    static Request getLicense(GetLicenseRequest getLicenseRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("license")
            .build();
        Request request = new Request(HttpGet.METHOD_NAME, endpoint);
        RequestConverters.Params parameters = new RequestConverters.Params(request);
        parameters.withLocal(getLicenseRequest.local());
        return request;
    }

    static Request deleteLicense(DeleteLicenseRequest deleteLicenseRequest) {
        Request request = new Request(HttpDelete.METHOD_NAME, "/_xpack/license");
        RequestConverters.Params parameters = new RequestConverters.Params(request);
        parameters.withTimeout(deleteLicenseRequest.timeout());
        parameters.withMasterTimeout(deleteLicenseRequest.masterNodeTimeout());
        return request;
    }
}
