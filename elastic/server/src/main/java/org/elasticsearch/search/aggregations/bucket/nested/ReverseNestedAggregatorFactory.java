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

package org.elasticsearch.search.aggregations.bucket.nested;

import org.elasticsearch.index.mapper.ObjectMapper;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.NonCollectingAggregator;
import org.elasticsearch.search.internal.SearchContext;

import java.io.IOException;
import java.util.Map;

public class ReverseNestedAggregatorFactory extends AggregatorFactory {

    private final boolean unmapped;
    private final ObjectMapper parentObjectMapper;

    public ReverseNestedAggregatorFactory(String name, boolean unmapped, ObjectMapper parentObjectMapper,
                                          QueryShardContext queryShardContext, AggregatorFactory parent,
                                          AggregatorFactories.Builder subFactories,
                                          Map<String, Object> metadata) throws IOException {
        super(name, queryShardContext, parent, subFactories, metadata);
        this.unmapped = unmapped;
        this.parentObjectMapper = parentObjectMapper;
    }

    @Override
    public Aggregator createInternal(SearchContext searchContext,
                                        Aggregator parent,
                                        boolean collectsFromSingleBucket,
                                        Map<String, Object> metadata) throws IOException {
        if (unmapped) {
            return new Unmapped(name, searchContext, parent, metadata);
        } else {
            return new ReverseNestedAggregator(name, factories, parentObjectMapper,
                searchContext, parent, metadata);
        }
    }

    private static final class Unmapped extends NonCollectingAggregator {

        Unmapped(String name,
                    SearchContext context,
                    Aggregator parent,
                    Map<String, Object> metadata) throws IOException {
            super(name, context, parent, metadata);
        }

        @Override
        public InternalAggregation buildEmptyAggregation() {
            return new InternalReverseNested(name, 0, buildEmptySubAggregations(), metadata());
        }
    }
}
