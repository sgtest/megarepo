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

import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public class PreviewDataFrameTransformResponse {

    private static final String PREVIEW = "preview";

    @SuppressWarnings("unchecked")
    public static PreviewDataFrameTransformResponse fromXContent(final XContentParser parser) throws IOException {
        Object previewDocs = parser.map().get(PREVIEW);
        return new PreviewDataFrameTransformResponse((List<Map<String, Object>>) previewDocs);
    }

    private List<Map<String, Object>> docs;

    public PreviewDataFrameTransformResponse(List<Map<String, Object>> docs) {
        this.docs = docs;
    }

    public List<Map<String, Object>> getDocs() {
        return docs;
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == this) {
            return true;
        }

        if (obj == null || obj.getClass() != getClass()) {
            return false;
        }

        PreviewDataFrameTransformResponse other = (PreviewDataFrameTransformResponse) obj;
        return Objects.equals(other.docs, docs);
    }

    @Override
    public int hashCode() {
        return Objects.hashCode(docs);
    }

}
