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

import org.apache.http.Header;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksRequest;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksResponse;

import java.io.IOException;

import static java.util.Collections.emptySet;

/**
 * A wrapper for the {@link RestHighLevelClient} that provides methods for accessing the Tasks API.
 * <p>
 * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/tasks.html">Task Management API on elastic.co</a>
 */
public class TasksClient {
    private final RestHighLevelClient restHighLevelClient;

    TasksClient(RestHighLevelClient restHighLevelClient) {
        this.restHighLevelClient = restHighLevelClient;
    }

    /**
     * Get current tasks using the Task Management API
     * <p>
     * See
     * <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/tasks.html"> Task Management API on elastic.co</a>
     */
    public ListTasksResponse list(ListTasksRequest request, Header... headers) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, RequestConverters::listTasks, ListTasksResponse::fromXContent,
                emptySet(), headers);
    }

    /**
     * Asynchronously get current tasks using the Task Management API
     * <p>
     * See
     * <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/tasks.html"> Task Management API on elastic.co</a>
     */
    public void listAsync(ListTasksRequest request, ActionListener<ListTasksResponse> listener, Header... headers) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, RequestConverters::listTasks, ListTasksResponse::fromXContent,
                listener, emptySet(), headers);
    }
}
