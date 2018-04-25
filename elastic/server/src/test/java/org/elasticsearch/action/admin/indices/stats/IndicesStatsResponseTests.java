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

package org.elasticsearch.action.admin.indices.stats;

import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.ESTestCase;

import java.util.Collections;

import static org.hamcrest.CoreMatchers.containsString;
import static org.hamcrest.object.HasToString.hasToString;


public class IndicesStatsResponseTests extends ESTestCase {

    public void testInvalidLevel() {
        final IndicesStatsResponse response = new IndicesStatsResponse();
        final String level = randomAlphaOfLength(16);
        final ToXContent.Params params = new ToXContent.MapParams(Collections.singletonMap("level", level));
        final IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> response.toXContent(JsonXContent.contentBuilder(), params));
        assertThat(
            e,
            hasToString(containsString("level parameter must be one of [cluster] or [indices] or [shards] but was [" + level + "]")));
    }

}
