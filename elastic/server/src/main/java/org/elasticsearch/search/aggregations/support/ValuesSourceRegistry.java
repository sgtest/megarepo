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
package org.elasticsearch.search.aggregations.support;

import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.search.aggregations.AggregationExecutionException;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeValuesSourceBuilder;

import java.util.AbstractMap;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * {@link ValuesSourceRegistry} holds the mapping from {@link ValuesSourceType}s to {@link AggregatorSupplier}s.  DO NOT directly
 * instantiate this class, instead get an already-configured copy from {@link QueryShardContext#getValuesSourceRegistry()}, or (in the case
 * of some test scenarios only) directly from {@link SearchModule#getValuesSourceRegistry()}
 *
 */
public class ValuesSourceRegistry {

    public interface CompositeSupplier {
        // this interface intentionally left blank
    }

    public static final class RegistryKey<T extends CompositeSupplier> {
        private final String name;
        private final Class<T> supplierType;

        public RegistryKey(String name, Class<T> supplierType) {
            this.name = Objects.requireNonNull(name);
            this.supplierType = Objects.requireNonNull(supplierType);
        }

        public String getName() {
            return name;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            RegistryKey that = (RegistryKey) o;
            return name.equals(that.name) && supplierType.equals(that.supplierType);
        }

        @Override
        public int hashCode() {
            return Objects.hash(name, supplierType);
        }
    }

    public static class Builder {
        private final AggregationUsageService.Builder usageServiceBuilder;
        private Map<String, List<Map.Entry<ValuesSourceType, AggregatorSupplier>>> aggregatorRegistry = new HashMap<>();
        private Map<RegistryKey<? extends CompositeSupplier>, List<Map.Entry<ValuesSourceType, CompositeSupplier>>> compositeRegistry =
            new HashMap<>();

        public Builder() {
            this.usageServiceBuilder = new AggregationUsageService.Builder();
        }


        /**
         * Register a ValuesSource to Aggregator mapping. This method registers mappings that only apply to a
         * single {@link ValuesSourceType}
         * @param aggregationName The name of the family of aggregations, typically found via
         *                        {@link ValuesSourceAggregationBuilder#getType()}
         * @param valuesSourceType The ValuesSourceType this mapping applies to.
         * @param aggregatorSupplier An Aggregation-specific specialization of AggregatorSupplier which will construct the mapped aggregator
         *                           from the aggregation standard set of parameters
         */
        public void register(String aggregationName, ValuesSourceType valuesSourceType,
                                          AggregatorSupplier aggregatorSupplier) {
            if (aggregatorRegistry.containsKey(aggregationName) == false) {
                aggregatorRegistry.put(aggregationName, new ArrayList<>());
            }
            aggregatorRegistry.get(aggregationName).add(new AbstractMap.SimpleEntry<>(valuesSourceType, aggregatorSupplier));
            registerUsage(aggregationName, valuesSourceType);
        }

        /**
         * Register a ValuesSource to Aggregator mapping. This version provides a convenience method for mappings that apply to a
         * known list of {@link ValuesSourceType}
         *  @param aggregationName The name of the family of aggregations, typically found via
         *                         {@link ValuesSourceAggregationBuilder#getType()}
         * @param valuesSourceTypes The ValuesSourceTypes this mapping applies to.
         * @param aggregatorSupplier An Aggregation-specific specialization of AggregatorSupplier which will construct the mapped aggregator
         *                           from the aggregation standard set of parameters
         */
        public void register(String aggregationName, List<ValuesSourceType> valuesSourceTypes, AggregatorSupplier aggregatorSupplier) {
            for (ValuesSourceType valuesSourceType : valuesSourceTypes) {
                register(aggregationName, valuesSourceType, aggregatorSupplier);
            }
        }

        /**
         * Register a new key generation function for the
         * {@link org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation}.
         * @param registryKey the subclass of {@link CompositeSupplier} associated with the {@link CompositeValuesSourceBuilder} type this
         *                      mapping is being registered for, paired with the name of the key type.
         * @param valuesSourceType the {@link ValuesSourceType} this mapping applies to
         * @param compositeSupplier A function returning an appropriate
         *                          {@link org.elasticsearch.search.aggregations.bucket.composite.CompositeValuesSourceConfig}
         */
        public <T extends CompositeSupplier> void registerComposite(
            RegistryKey<T> registryKey,
            ValuesSourceType valuesSourceType,
            T compositeSupplier
        ) {
            if (compositeRegistry.containsKey(registryKey) == false) {
                compositeRegistry.put(registryKey, new ArrayList<>());
            }
            compositeRegistry.get(registryKey).add(new AbstractMap.SimpleEntry<>(valuesSourceType, compositeSupplier));
        }

        /**
         * Register a new key generation function for the
         * {@link org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation}.  This is a convenience version to map
         * multiple types to the same supplier.
         * @param registryKey the subclass of {@link CompositeSupplier} associated with the {@link CompositeValuesSourceBuilder} type this
         *                      mapping is being registered for, paired with the name of the key type.
         * @param valuesSourceTypes the {@link ValuesSourceType}s this mapping applies to
         * @param compositeSupplier A function returning an appropriate
         *                          {@link org.elasticsearch.search.aggregations.bucket.composite.CompositeValuesSourceConfig}
         */
        public <T extends CompositeSupplier> void registerComposite(
            RegistryKey<T> registryKey,
            List<ValuesSourceType> valuesSourceTypes,
            T compositeSupplier
        ) {
            for (ValuesSourceType valuesSourceType : valuesSourceTypes) {
                registerComposite(registryKey, valuesSourceType, compositeSupplier);
            }
        }

        public void registerUsage(String aggregationName, ValuesSourceType valuesSourceType) {
            usageServiceBuilder.registerAggregationUsage(aggregationName, valuesSourceType.typeName());
        }

        public void registerUsage(String aggregationName) {
            usageServiceBuilder.registerAggregationUsage(aggregationName);
        }

        public ValuesSourceRegistry build() {
            return new ValuesSourceRegistry(aggregatorRegistry, compositeRegistry, usageServiceBuilder.build());
        }
    }

    private static <K, T> Map<K, Map<ValuesSourceType, T>> copyMap(Map<K, List<Map.Entry<ValuesSourceType, T>>> mutableMap) {
        /*
         Make an immutatble copy of our input map. Since this is write once, read many, we'll spend a bit of extra time to shape this
         into a Map.of(), which is more read optimized than just using a hash map.
         */
        @SuppressWarnings("unchecked")
        Map.Entry<K, Map<ValuesSourceType, T>>[] copiedEntries = new Map.Entry[mutableMap.size()];
        int i = 0;
        for (Map.Entry<K, List<Map.Entry<ValuesSourceType, T>>> entry : mutableMap.entrySet()) {
            K topKey = entry.getKey();
            List<Map.Entry<ValuesSourceType, T>> values = entry.getValue();
            @SuppressWarnings("unchecked")
            Map.Entry<K, Map<ValuesSourceType, T>> newEntry = Map.entry(topKey, Map.ofEntries(values.toArray(new Map.Entry[0])));
            copiedEntries[i++] = newEntry;
        }
        return Map.ofEntries(copiedEntries);
    }

    /** Maps Aggregation names to (ValuesSourceType, Supplier) pairs, keyed by ValuesSourceType */
    private final AggregationUsageService usageService;
    private Map<String, Map<ValuesSourceType, AggregatorSupplier>> aggregatorRegistry;
    private Map<RegistryKey<? extends CompositeSupplier>, Map<ValuesSourceType, CompositeSupplier>> compositeRegistry;

    public ValuesSourceRegistry(Map<String, List<Map.Entry<ValuesSourceType, AggregatorSupplier>>> aggregatorRegistry,
        Map<RegistryKey<? extends CompositeSupplier>, List<Map.Entry<ValuesSourceType, CompositeSupplier>>> compositeRegistry,
                                AggregationUsageService usageService) {
        this.aggregatorRegistry = copyMap(aggregatorRegistry);
        this.compositeRegistry = copyMap(compositeRegistry);
        this.usageService = usageService;
    }

    private AggregatorSupplier findMatchingSuppier(ValuesSourceType valuesSourceType,
                                                   Map<ValuesSourceType, AggregatorSupplier> supportedTypes) {
        return supportedTypes.get(valuesSourceType);
    }

    public boolean isRegistered(String aggregationName) {
        return aggregatorRegistry.containsKey(aggregationName);
    }

    public AggregatorSupplier getAggregator(ValuesSourceConfig valuesSourceConfig, String aggregationName) {
        if (aggregationName != null && aggregatorRegistry.containsKey(aggregationName)) {
            AggregatorSupplier supplier = findMatchingSuppier(
                valuesSourceConfig.valueSourceType(),
                aggregatorRegistry.get(aggregationName)
            );
            if (supplier == null) {
                throw new IllegalArgumentException(
                    valuesSourceConfig.getDescription() + " is not supported for aggregation [" + aggregationName + "]"
                );
            }
            return supplier;
        }
        throw  new AggregationExecutionException("Unregistered Aggregation [" + aggregationName + "]");
    }

    public <T extends CompositeSupplier> T getComposite(RegistryKey<T> registryKey, ValuesSourceConfig config) {
        if (registryKey != null && compositeRegistry.containsKey(registryKey)) {
            CompositeSupplier supplier = compositeRegistry.get(registryKey).get(config.valueSourceType());
            if (supplier == null) {
                throw new IllegalArgumentException(config.getDescription() + " is not supported for composite source [" +
                    registryKey.getName() + "]");
            }
            return (T) supplier; // Safe because we checked the type matched the key at load time
        }
        throw new AggregationExecutionException("Unregistered composite source [" + registryKey.getName() + "]");
    }

    public AggregationUsageService getUsageService() {
        return usageService;
    }
}
