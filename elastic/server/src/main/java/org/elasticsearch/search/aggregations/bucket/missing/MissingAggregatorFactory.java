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

package org.elasticsearch.search.aggregations.bucket.missing;

import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.CardinalityUpperBound;
import org.elasticsearch.search.aggregations.support.AggregationContext;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.aggregations.support.ValuesSourceAggregatorFactory;
import org.elasticsearch.search.aggregations.support.ValuesSourceConfig;
import org.elasticsearch.search.aggregations.support.ValuesSourceRegistry;

import java.io.IOException;
import java.util.Map;

public class MissingAggregatorFactory extends ValuesSourceAggregatorFactory {

    private final MissingAggregatorSupplier aggregatorSupplier;

    public static void registerAggregators(ValuesSourceRegistry.Builder builder) {
        builder.register(MissingAggregationBuilder.REGISTRY_KEY, CoreValuesSourceType.ALL_CORE, MissingAggregator::new, true);
    }

    public MissingAggregatorFactory(String name, ValuesSourceConfig config, AggregationContext context,
                                    AggregatorFactory parent, AggregatorFactories.Builder subFactoriesBuilder,
                                    Map<String, Object> metadata,
                                    MissingAggregatorSupplier aggregatorSupplier) throws IOException {
        super(name, config, context, parent, subFactoriesBuilder, metadata);
        this.aggregatorSupplier = aggregatorSupplier;
    }

    @Override
    protected MissingAggregator createUnmapped(Aggregator parent, Map<String, Object> metadata) throws IOException {
        return new MissingAggregator(name, factories, config, context, parent, CardinalityUpperBound.NONE, metadata);
    }

    @Override
    protected Aggregator doCreateInternal(Aggregator parent,
                                          CardinalityUpperBound cardinality,
                                          Map<String, Object> metadata) throws IOException {
        return aggregatorSupplier
            .build(name, factories, config, context, parent, cardinality, metadata);
    }
}
