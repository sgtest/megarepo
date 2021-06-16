/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.vectortile.rest;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ParseField;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.core.CheckedFunction;
import org.elasticsearch.geometry.Rectangle;
import org.elasticsearch.index.query.AbstractQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.PipelineAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.geogrid.GeoTileUtils;
import org.elasticsearch.search.aggregations.metrics.AvgAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.CardinalityAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.MaxAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.MinAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.SumAggregationBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.subphase.FieldAndFormat;
import org.elasticsearch.search.sort.SortBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;

import static java.util.Collections.emptyList;
import static java.util.Collections.emptyMap;

/**
 * Transforms a rest request in a vector tile request
 */
class VectorTileRequest {

    protected static final String INDEX_PARAM = "index";
    protected static final String FIELD_PARAM = "field";
    protected static final String Z_PARAM = "z";
    protected static final String X_PARAM = "x";
    protected static final String Y_PARAM = "y";

    protected static final ParseField GRID_PRECISION_FIELD = new ParseField("grid_precision");
    protected static final ParseField GRID_TYPE_FIELD = new ParseField("grid_type");
    protected static final ParseField EXTENT_FIELD = new ParseField("extent");
    protected static final ParseField EXACT_BOUNDS_FIELD = new ParseField("exact_bounds");

    protected enum GRID_TYPE {
        GRID,
        POINT;

        private static GRID_TYPE fromString(String type) {
            switch (type.toLowerCase(Locale.ROOT)) {
                case "grid":
                    return GRID;
                case "point":
                    return POINT;
                default:
                    throw new IllegalArgumentException("Invalid grid type [" + type + "]");
            }
        }
    }

    protected static class Defaults {
        // TODO: Should it be SearchService.DEFAULT_SIZE?
        public static final int SIZE = 10000;
        public static final List<FieldAndFormat> FETCH = emptyList();
        public static final Map<String, Object> RUNTIME_MAPPINGS = emptyMap();
        public static final QueryBuilder QUERY = null;
        public static final AggregatorFactories.Builder AGGS = null;
        public static final List<SortBuilder<?>> SORT = emptyList();
        // TODO: Should it be 0, no aggs by default?
        public static final int GRID_PRECISION = 8;
        public static final GRID_TYPE GRID_TYPE = VectorTileRequest.GRID_TYPE.GRID;
        public static final int EXTENT = 4096;
        public static final boolean EXACT_BOUNDS = false;
    }

    private static final ObjectParser<VectorTileRequest, RestRequest> PARSER;

    static {
        PARSER = new ObjectParser<>("vector-tile");
        PARSER.declareInt(VectorTileRequest::setSize, SearchSourceBuilder.SIZE_FIELD);
        PARSER.declareField(VectorTileRequest::setFieldAndFormats, (p) -> {
            List<FieldAndFormat> fetchFields = new ArrayList<>();
            while ((p.nextToken()) != XContentParser.Token.END_ARRAY) {
                fetchFields.add(FieldAndFormat.fromXContent(p));
            }
            return fetchFields;
        }, SearchSourceBuilder.FETCH_FIELDS_FIELD, ObjectParser.ValueType.OBJECT_ARRAY);
        PARSER.declareField(
            VectorTileRequest::setQueryBuilder,
            (CheckedFunction<XContentParser, QueryBuilder, IOException>) AbstractQueryBuilder::parseInnerQueryBuilder,
            SearchSourceBuilder.QUERY_FIELD,
            ObjectParser.ValueType.OBJECT
        );
        PARSER.declareField(
            VectorTileRequest::setRuntimeMappings,
            XContentParser::map,
            SearchSourceBuilder.RUNTIME_MAPPINGS_FIELD,
            ObjectParser.ValueType.OBJECT
        );
        PARSER.declareField(
            VectorTileRequest::setAggBuilder,
            AggregatorFactories::parseAggregators,
            SearchSourceBuilder.AGGS_FIELD,
            ObjectParser.ValueType.OBJECT
        );
        PARSER.declareField(
            VectorTileRequest::setSortBuilders,
            SortBuilder::fromXContent,
            SearchSourceBuilder.SORT_FIELD,
            ObjectParser.ValueType.OBJECT_ARRAY
        );
        // Specific for vector tiles
        PARSER.declareInt(VectorTileRequest::setGridPrecision, GRID_PRECISION_FIELD);
        PARSER.declareString(VectorTileRequest::setGridType, GRID_TYPE_FIELD);
        PARSER.declareInt(VectorTileRequest::setExtent, EXTENT_FIELD);
        PARSER.declareBoolean(VectorTileRequest::setExactBounds, EXACT_BOUNDS_FIELD);
    }

    static VectorTileRequest parseRestRequest(RestRequest restRequest) throws IOException {
        final VectorTileRequest request = new VectorTileRequest(
            Strings.splitStringByCommaToArray(restRequest.param(INDEX_PARAM)),
            restRequest.param(FIELD_PARAM),
            Integer.parseInt(restRequest.param(Z_PARAM)),
            Integer.parseInt(restRequest.param(X_PARAM)),
            Integer.parseInt(restRequest.param(Y_PARAM))
        );
        if (restRequest.hasContent()) {
            try (XContentParser contentParser = restRequest.contentParser()) {
                PARSER.parse(contentParser, request, restRequest);
            }
        }
        return request;
    }

    private final String[] indexes;
    private final String field;
    private final int x;
    private final int y;
    private final int z;
    private final Rectangle bbox;
    private QueryBuilder queryBuilder = Defaults.QUERY;
    private Map<String, Object> runtimeMappings = Defaults.RUNTIME_MAPPINGS;
    private int gridPrecision = Defaults.GRID_PRECISION;
    private GRID_TYPE gridType = Defaults.GRID_TYPE;
    private int size = Defaults.SIZE;
    private int extent = Defaults.EXTENT;
    private AggregatorFactories.Builder aggBuilder = Defaults.AGGS;
    private List<FieldAndFormat> fields = Defaults.FETCH;
    private List<SortBuilder<?>> sortBuilders = Defaults.SORT;
    private boolean exact_bounds = Defaults.EXACT_BOUNDS;

    private VectorTileRequest(String[] indexes, String field, int z, int x, int y) {
        this.indexes = indexes;
        this.field = field;
        this.z = z;
        this.x = x;
        this.y = y;
        // This should validate that z/x/y is a valid combination
        this.bbox = GeoTileUtils.toBoundingBox(x, y, z);
    }

    public String[] getIndexes() {
        return indexes;
    }

    public String getField() {
        return field;
    }

    public int getX() {
        return x;
    }

    public int getY() {
        return y;
    }

    public int getZ() {
        return z;
    }

    public Rectangle getBoundingBox() {
        return bbox;
    }

    public int getExtent() {
        return extent;
    }

    private void setExtent(int extent) {
        if (extent < 0) {
            throw new IllegalArgumentException("[extent] parameter cannot be negative, found [" + extent + "]");
        }
        this.extent = extent;
    }

    public boolean getExactBounds() {
        return exact_bounds;
    }

    private void setExactBounds(boolean exact_bounds) {
        this.exact_bounds = exact_bounds;
    }

    public List<FieldAndFormat> getFieldAndFormats() {
        return fields;
    }

    private void setFieldAndFormats(List<FieldAndFormat> fields) {
        this.fields = fields;
    }

    public QueryBuilder getQueryBuilder() {
        return queryBuilder;
    }

    private void setQueryBuilder(QueryBuilder queryBuilder) {
        // TODO: validation
        this.queryBuilder = queryBuilder;
    }

    public Map<String, Object> getRuntimeMappings() {
        return runtimeMappings;
    }

    private void setRuntimeMappings(Map<String, Object> runtimeMappings) {
        this.runtimeMappings = runtimeMappings;
    }

    public int getGridPrecision() {
        return gridPrecision;
    }

    private void setGridPrecision(int gridPrecision) {
        if (gridPrecision < 0) {
            throw new IllegalArgumentException("[gridPrecision] parameter cannot be negative, found [" + gridPrecision + "]");
        }
        if (gridPrecision > 8) {
            throw new IllegalArgumentException("[gridPrecision] parameter cannot be bigger than 8, found [" + gridPrecision + "]");
        }
        this.gridPrecision = gridPrecision;
    }

    public GRID_TYPE getGridType() {
        return gridType;
    }

    private void setGridType(String gridType) {
        this.gridType = GRID_TYPE.fromString(gridType);
    }

    public int getSize() {
        return size;
    }

    private void setSize(int size) {
        if (size < 0) {
            throw new IllegalArgumentException("[size] parameter cannot be negative, found [" + size + "]");
        }
        this.size = size;
    }

    public AggregatorFactories.Builder getAggBuilder() {
        return aggBuilder;
    }

    private void setAggBuilder(AggregatorFactories.Builder aggBuilder) {
        for (AggregationBuilder aggregation : aggBuilder.getAggregatorFactories()) {
            final String type = aggregation.getType();
            switch (type) {
                case MinAggregationBuilder.NAME:
                case MaxAggregationBuilder.NAME:
                case AvgAggregationBuilder.NAME:
                case SumAggregationBuilder.NAME:
                case CardinalityAggregationBuilder.NAME:
                    break;
                default:
                    // top term and percentile should be supported
                    throw new IllegalArgumentException("Unsupported aggregation of type [" + type + "]");
            }
        }
        for (PipelineAggregationBuilder aggregation : aggBuilder.getPipelineAggregatorFactories()) {
            // should not have pipeline aggregations
            final String type = aggregation.getType();
            throw new IllegalArgumentException("Unsupported pipeline aggregation of type [" + type + "]");
        }
        this.aggBuilder = aggBuilder;
    }

    public List<SortBuilder<?>> getSortBuilders() {
        return sortBuilders;
    }

    private void setSortBuilders(List<SortBuilder<?>> sortBuilders) {
        this.sortBuilders = sortBuilders;
    }
}
