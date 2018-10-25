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
import org.apache.http.client.methods.HttpPost;
import org.apache.http.client.methods.HttpPut;
import org.apache.http.entity.ByteArrayEntity;
import org.apache.http.entity.ContentType;
import org.elasticsearch.client.watcher.DeactivateWatchRequest;
import org.elasticsearch.client.watcher.ActivateWatchRequest;
import org.elasticsearch.client.watcher.AckWatchRequest;
import org.elasticsearch.client.watcher.StartWatchServiceRequest;
import org.elasticsearch.client.watcher.StopWatchServiceRequest;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.protocol.xpack.watcher.DeleteWatchRequest;
import org.elasticsearch.protocol.xpack.watcher.PutWatchRequest;

final class WatcherRequestConverters {

    private WatcherRequestConverters() {}

    static Request startWatchService(StartWatchServiceRequest startWatchServiceRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
                .addPathPartAsIs("_xpack")
                .addPathPartAsIs("watcher")
                .addPathPartAsIs("_start")
                .build();

        return new Request(HttpPost.METHOD_NAME, endpoint);
    }

    static Request stopWatchService(StopWatchServiceRequest stopWatchServiceRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
                .addPathPartAsIs("_xpack")
                .addPathPartAsIs("watcher")
                .addPathPartAsIs("_stop")
                .build();

        return new Request(HttpPost.METHOD_NAME, endpoint);
    }

    static Request putWatch(PutWatchRequest putWatchRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("watcher")
            .addPathPartAsIs("watch")
            .addPathPart(putWatchRequest.getId())
            .build();

        Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        RequestConverters.Params params = new RequestConverters.Params(request).withVersion(putWatchRequest.getVersion());
        if (putWatchRequest.isActive() == false) {
            params.putParam("active", "false");
        }
        ContentType contentType = RequestConverters.createContentType(putWatchRequest.xContentType());
        BytesReference source = putWatchRequest.getSource();
        request.setEntity(new ByteArrayEntity(source.toBytesRef().bytes, 0, source.length(), contentType));
        return request;
    }

    static Request deactivateWatch(DeactivateWatchRequest deactivateWatchRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("watcher")
            .addPathPartAsIs("watch")
            .addPathPart(deactivateWatchRequest.getWatchId())
            .addPathPartAsIs("_deactivate")
            .build();
        return new Request(HttpPut.METHOD_NAME, endpoint);
    }

    static Request deleteWatch(DeleteWatchRequest deleteWatchRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("watcher")
            .addPathPartAsIs("watch")
            .addPathPart(deleteWatchRequest.getId())
            .build();

        Request request = new Request(HttpDelete.METHOD_NAME, endpoint);
        return request;
    }

    public static Request ackWatch(AckWatchRequest ackWatchRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("watcher")
            .addPathPartAsIs("watch")
            .addPathPart(ackWatchRequest.getWatchId())
            .addPathPartAsIs("_ack")
            .addCommaSeparatedPathParts(ackWatchRequest.getActionIds())
            .build();
        Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        return request;
    }

    static Request activateWatch(ActivateWatchRequest activateWatchRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack")
            .addPathPartAsIs("watcher")
            .addPathPartAsIs("watch")
            .addPathPart(activateWatchRequest.getWatchId())
            .addPathPartAsIs("_activate")
            .build();
        Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        return request;
    }
}
