/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.spatial;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.license.License;
import org.elasticsearch.license.TestUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.search.aggregations.bucket.geogrid.GeoHashGridAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.geogrid.GeoTileGridAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.GeoCentroidAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.GeoCentroidAggregatorSupplier;
import org.elasticsearch.search.aggregations.metrics.GeoGridAggregatorSupplier;
import org.elasticsearch.search.aggregations.support.ValuesSourceRegistry;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.xpack.spatial.search.aggregations.support.GeoShapeValuesSourceType;

import java.util.Arrays;
import java.util.List;
import java.util.function.Consumer;

import static org.hamcrest.Matchers.equalTo;

public class SpatialPluginTests extends ESTestCase {

    public void testGeoCentroidLicenseCheck() {
        for (License.OperationMode operationMode : License.OperationMode.values()) {
            SpatialPlugin plugin = getPluginWithOperationMode(operationMode);
            ValuesSourceRegistry.Builder registryBuilder = new ValuesSourceRegistry.Builder();
            List<Consumer<ValuesSourceRegistry.Builder>> registrar = plugin.getAggregationExtentions();
            registrar.forEach(c -> c.accept(registryBuilder));
            ValuesSourceRegistry registry = registryBuilder.build();
            GeoCentroidAggregatorSupplier centroidSupplier = (GeoCentroidAggregatorSupplier) registry.getAggregator(
                GeoShapeValuesSourceType.instance(), GeoCentroidAggregationBuilder.NAME);
            if (License.OperationMode.TRIAL != operationMode &&
                    License.OperationMode.compare(operationMode, License.OperationMode.GOLD) < 0) {
                ElasticsearchSecurityException exception = expectThrows(ElasticsearchSecurityException.class,
                    () -> centroidSupplier.build(null, null, null, null, null));
                assertThat(exception.getMessage(),
                    equalTo("current license is non-compliant for [geo_centroid aggregation on geo_shape fields]"));
            }
        }
    }

    public void testGeoGridLicenseCheck() {
        for (String builderName : Arrays.asList(GeoHashGridAggregationBuilder.NAME, GeoTileGridAggregationBuilder.NAME)) {
            for (License.OperationMode operationMode : License.OperationMode.values()) {
                SpatialPlugin plugin = getPluginWithOperationMode(operationMode);
                ValuesSourceRegistry.Builder registryBuilder = new ValuesSourceRegistry.Builder();
                List<Consumer<ValuesSourceRegistry.Builder>> registrar = plugin.getAggregationExtentions();
                registrar.forEach(c -> c.accept(registryBuilder));
                ValuesSourceRegistry registry = registryBuilder.build();
                GeoGridAggregatorSupplier supplier = (GeoGridAggregatorSupplier) registry.getAggregator(
                    GeoShapeValuesSourceType.instance(), builderName);
                if (License.OperationMode.TRIAL != operationMode &&
                    License.OperationMode.compare(operationMode, License.OperationMode.GOLD) < 0) {
                    ElasticsearchSecurityException exception = expectThrows(ElasticsearchSecurityException.class,
                        () -> supplier.build(null, null, null, 0, null,
                            0,0,  null, null, null));
                    assertThat(exception.getMessage(),
                        equalTo("current license is non-compliant for [" + builderName + " aggregation on geo_shape fields]"));
                }
            }
        }
    }

    private SpatialPlugin getPluginWithOperationMode(License.OperationMode operationMode) {
        return new SpatialPlugin() {
            protected XPackLicenseState getLicenseState() {
                TestUtils.UpdatableLicenseState licenseState = new TestUtils.UpdatableLicenseState();
                licenseState.update(operationMode, true, VersionUtils.randomVersion(random()));
                return licenseState;
            }
        };
    }
}
