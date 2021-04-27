/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.spatial.index.mapper;

import org.apache.lucene.document.LatLonShape;
import org.apache.lucene.geo.LatLonGeometry;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.search.IndexOrDocValuesQuery;
import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.Query;
import org.elasticsearch.Version;
import org.elasticsearch.common.Explicit;
import org.elasticsearch.common.geo.GeoShapeUtils;
import org.elasticsearch.common.geo.GeometryParser;
import org.elasticsearch.common.geo.ShapeRelation;
import org.elasticsearch.common.geo.builders.ShapeBuilder.Orientation;
import org.elasticsearch.common.logging.DeprecationCategory;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.geometry.Geometry;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.mapper.AbstractShapeGeometryFieldMapper;
import org.elasticsearch.index.mapper.ContentPath;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.GeoShapeFieldMapper;
import org.elasticsearch.index.mapper.GeoShapeIndexer;
import org.elasticsearch.index.mapper.GeoShapeParser;
import org.elasticsearch.index.mapper.GeoShapeQueryable;
import org.elasticsearch.index.mapper.LegacyGeoShapeFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.Mapper;
import org.elasticsearch.index.mapper.ParseContext;
import org.elasticsearch.index.query.QueryShardException;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.xpack.spatial.index.fielddata.plain.AbstractLatLonShapeIndexFieldData;
import org.elasticsearch.xpack.spatial.search.aggregations.support.GeoShapeValuesSourceType;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

/**
 * Extension of {@link org.elasticsearch.index.mapper.GeoShapeFieldMapper} that supports docValues
 *
 * FieldMapper for indexing {@link LatLonShape}s.
 * <p>
 * Currently Shapes can only be indexed and can only be queried using
 * {@link org.elasticsearch.index.query.GeoShapeQueryBuilder}, consequently
 * a lot of behavior in this Mapper is disabled.
 * <p>
 * Format supported:
 * <p>
 * "field" : {
 * "type" : "polygon",
 * "coordinates" : [
 * [ [100.0, 0.0], [101.0, 0.0], [101.0, 1.0], [100.0, 1.0], [100.0, 0.0] ]
 * ]
 * }
 * <p>
 * or:
 * <p>
 * "field" : "POLYGON ((100.0 0.0, 101.0 0.0, 101.0 1.0, 100.0 1.0, 100.0 0.0))
 */
public class GeoShapeWithDocValuesFieldMapper extends AbstractShapeGeometryFieldMapper<Geometry> {
    public static final String CONTENT_TYPE = "geo_shape";

    private static final DeprecationLogger DEPRECATION_LOGGER = DeprecationLogger.getLogger(GeoShapeFieldMapper.class);

    private static Builder builder(FieldMapper in) {
        return ((GeoShapeWithDocValuesFieldMapper)in).builder;
    }

    public static class Builder extends FieldMapper.Builder {

        final Parameter<Boolean> indexed = Parameter.indexParam(m -> builder(m).indexed.get(), true);
        final Parameter<Boolean> hasDocValues;

        final Parameter<Explicit<Boolean>> ignoreMalformed;
        final Parameter<Explicit<Boolean>> ignoreZValue = ignoreZValueParam(m -> builder(m).ignoreZValue.get());
        final Parameter<Explicit<Boolean>> coerce;
        final Parameter<Explicit<Orientation>> orientation = orientationParam(m -> builder(m).orientation.get());

        final Parameter<Map<String, String>> meta = Parameter.metaParam();

        private final Version version;

        public Builder(String name, Version version, boolean ignoreMalformedByDefault, boolean coerceByDefault) {
            super(name);
            this.version = version;
            this.ignoreMalformed = ignoreMalformedParam(m -> builder(m).ignoreMalformed.get(), ignoreMalformedByDefault);
            this.coerce = coerceParam(m -> builder(m).coerce.get(), coerceByDefault);
            this.hasDocValues
                = Parameter.docValuesParam(m -> builder(m).hasDocValues.get(), Version.V_7_8_0.onOrBefore(version));
        }

        @Override
        protected List<Parameter<?>> getParameters() {
            return Arrays.asList(indexed, hasDocValues, ignoreMalformed, ignoreZValue, coerce, orientation, meta);
        }

        @Override
        public GeoShapeWithDocValuesFieldMapper build(ContentPath contentPath) {
            if (multiFieldsBuilder.hasMultiFields()) {
                DEPRECATION_LOGGER.deprecate(
                    DeprecationCategory.MAPPINGS,
                    "geo_shape_multifields",
                    "Adding multifields to [geo_shape] mappers has no effect and will be forbidden in future"
                );
            }
            GeometryParser geometryParser = new GeometryParser(
                orientation.get().value().getAsBoolean(),
                coerce.get().value(),
                ignoreZValue.get().value());
            GeoShapeParser parser = new GeoShapeParser(geometryParser);
            GeoShapeWithDocValuesFieldType ft = new GeoShapeWithDocValuesFieldType(
                buildFullName(contentPath),
                indexed.get(),
                hasDocValues.get(),
                orientation.get().value(),
                parser,
                meta.get());
            return new GeoShapeWithDocValuesFieldMapper(name, ft,
                multiFieldsBuilder.build(this, contentPath), copyTo.build(),
                new GeoShapeIndexer(orientation.get().value().getAsBoolean(), ft.name()), parser, this);
        }

    }

    public static final class GeoShapeWithDocValuesFieldType extends AbstractShapeGeometryFieldType implements GeoShapeQueryable {

        public GeoShapeWithDocValuesFieldType(String name, boolean indexed, boolean hasDocValues,
                                              Orientation orientation, GeoShapeParser parser, Map<String, String> meta) {
            super(name, indexed, false, hasDocValues, false, parser, orientation, meta);
        }

        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
            failIfNoDocValues();
            return new AbstractLatLonShapeIndexFieldData.Builder(name(), GeoShapeValuesSourceType.instance());
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public Query geoShapeQuery(Geometry shape, String fieldName, ShapeRelation relation, SearchExecutionContext context) {
            // CONTAINS queries are not supported by VECTOR strategy for indices created before version 7.5.0 (Lucene 8.3.0)
            if (relation == ShapeRelation.CONTAINS && context.indexVersionCreated().before(Version.V_7_5_0)) {
                throw new QueryShardException(context,
                    ShapeRelation.CONTAINS + " query relation not supported for Field [" + fieldName + "].");
            }
            final LatLonGeometry[] luceneGeometries = GeoShapeUtils.toLuceneGeometry(fieldName, context, shape, relation);
            if (luceneGeometries.length == 0) {
                return new MatchNoDocsQuery();
            }
            Query query = LatLonShape.newGeometryQuery(fieldName, relation.getLuceneRelation(), luceneGeometries);
            if (hasDocValues()) {
                final Query queryDocValues = new LatLonShapeDocValuesQuery(fieldName, relation.getLuceneRelation(), luceneGeometries);
                query =  new IndexOrDocValuesQuery(query, queryDocValues);
            }
            return query;
        }
    }

    @SuppressWarnings("deprecation")
    public static Mapper.TypeParser PARSER = (name, node, parserContext) -> {
        FieldMapper.Builder builder;
        boolean ignoreMalformedByDefault = IGNORE_MALFORMED_SETTING.get(parserContext.getSettings());
        boolean coerceByDefault = COERCE_SETTING.get(parserContext.getSettings());
        if (LegacyGeoShapeFieldMapper.containsDeprecatedParameter(node.keySet())) {
            builder = new LegacyGeoShapeFieldMapper.Builder(
                name,
                parserContext.indexVersionCreated(),
                ignoreMalformedByDefault,
                coerceByDefault);
        } else {
            builder = new GeoShapeWithDocValuesFieldMapper.Builder(
                name,
                parserContext.indexVersionCreated(),
                ignoreMalformedByDefault,
                coerceByDefault);
        }
        builder.parse(name, parserContext, node);
        return builder;
    };

    private final Builder builder;
    private final GeoShapeIndexer indexer;

    public GeoShapeWithDocValuesFieldMapper(String simpleName, MappedFieldType mappedFieldType,
                                            MultiFields multiFields, CopyTo copyTo,
                                            GeoShapeIndexer indexer, GeoShapeParser parser, Builder builder) {
        super(simpleName, mappedFieldType, builder.ignoreMalformed.get(), builder.coerce.get(),
            builder.ignoreZValue.get(), builder.orientation.get(),
            multiFields, copyTo, parser);
        this.builder = builder;
        this.indexer = indexer;
    }

    @Override
    protected void index(ParseContext context, Geometry geometry) throws IOException {
        if (geometry == null) {
            return;
        }
        List<IndexableField> fields = indexer.indexShape(geometry);
        if (fieldType().isSearchable()) {
            context.doc().addAll(fields);
        }
        if (fieldType().hasDocValues()) {
            String name = fieldType().name();
            BinaryGeoShapeDocValuesField docValuesField = (BinaryGeoShapeDocValuesField) context.doc().getByKey(name);
            if (docValuesField == null) {
                docValuesField = new BinaryGeoShapeDocValuesField(name);
                context.doc().addWithKey(name, docValuesField);
            }
            docValuesField.add(fields, geometry);
        } else if (fieldType().isSearchable()) {
            createFieldNamesField(context);
        }
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return new Builder(
            simpleName(),
            builder.version,
            builder.ignoreMalformed.getDefaultValue().value(),
            builder.coerce.getDefaultValue().value()
        ).init(this);
    }

    @Override
    public GeoShapeWithDocValuesFieldType fieldType() {
        return (GeoShapeWithDocValuesFieldType) super.fieldType();
    }

}
