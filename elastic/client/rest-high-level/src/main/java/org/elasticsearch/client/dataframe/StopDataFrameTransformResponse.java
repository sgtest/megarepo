/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client.dataframe;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.TaskOperationFailure;
import org.elasticsearch.client.core.AcknowledgedTasksResponse;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.List;

public class StopDataFrameTransformResponse extends AcknowledgedTasksResponse {

    private static final String STOPPED = "stopped";

    private static final ConstructingObjectParser<StopDataFrameTransformResponse, Void> PARSER =
            AcknowledgedTasksResponse.generateParser("stop_data_frame_transform_response", StopDataFrameTransformResponse::new, STOPPED);

    public static StopDataFrameTransformResponse fromXContent(final XContentParser parser) throws IOException {
        return PARSER.parse(parser, null);
    }

    public StopDataFrameTransformResponse(boolean stopped, @Nullable List<TaskOperationFailure> taskFailures,
                                          @Nullable List<? extends ElasticsearchException> nodeFailures) {
        super(stopped, taskFailures, nodeFailures);
    }

    public boolean isStopped() {
        return isAcknowledged();
    }
}
