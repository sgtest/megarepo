/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */


package org.elasticsearch.xpack.vectors.mapper;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.IndexService;
import org.elasticsearch.index.mapper.ContentPath;
import org.elasticsearch.index.mapper.DocumentMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.SourceToParse;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.test.ESSingleNodeTestCase;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.xpack.core.LocalStateCompositeXPackPlugin;
import org.elasticsearch.xpack.vectors.Vectors;

import java.util.Collection;

import static org.hamcrest.Matchers.containsString;

public class SparseVectorFieldMapperTests extends ESSingleNodeTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> getPlugins() {
        return pluginList(Vectors.class, LocalStateCompositeXPackPlugin.class);
    }

    // this allows to set indexVersion as it is a private setting
    @Override
    protected boolean forbidPrivateIndexSettings() {
        return false;
    }

    public void testValueFetcherIsNotSupported() {
        SparseVectorFieldMapper.Builder builder = new SparseVectorFieldMapper.Builder("field");
        MappedFieldType fieldMapper = builder.build(new ContentPath()).fieldType();
        UnsupportedOperationException exc = expectThrows(UnsupportedOperationException.class,
            () -> fieldMapper.valueFetcher(null, null));
        assertEquals(SparseVectorFieldMapper.ERROR_MESSAGE_7X, exc.getMessage());
    }

    public void testSparseVectorWith8xIndex() throws Exception {
        Version version = VersionUtils.randomVersionBetween(random(), Version.V_8_0_0, Version.CURRENT);
        Settings settings = Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, version)
            .build();

        IndexService indexService = createIndex("index", settings);
        MapperService mapperService = indexService.mapperService();

        BytesReference mapping = BytesReference.bytes(XContentFactory.jsonBuilder()
            .startObject()
                .startObject("_doc")
                    .startObject("properties")
                        .startObject("my-vector").field("type", "sparse_vector")
                        .endObject()
                    .endObject()
                .endObject()
            .endObject());

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () ->
            mapperService.parse(MapperService.SINGLE_MAPPING_NAME, new CompressedXContent(mapping)));
        assertThat(e.getMessage(), containsString(SparseVectorFieldMapper.ERROR_MESSAGE));
    }

    public void testSparseVectorWith7xIndex() throws Exception {
        Version version = VersionUtils.randomPreviousCompatibleVersion(random(), Version.V_8_0_0);
        Settings settings = Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, version)
            .build();

        IndexService indexService = createIndex("index", settings);
        MapperService mapperService = indexService.mapperService();

        BytesReference mapping = BytesReference.bytes(XContentFactory.jsonBuilder()
            .startObject()
                .startObject("_doc")
                    .startObject("properties")
                        .startObject("my-vector").field("type", "sparse_vector")
                        .endObject()
                    .endObject()
                .endObject()
            .endObject());

        DocumentMapper mapper = mapperService.parse(MapperService.SINGLE_MAPPING_NAME, new CompressedXContent(mapping));
        assertWarnings(SparseVectorFieldMapper.ERROR_MESSAGE_7X);

        // Check that new vectors cannot be indexed.
        int[] indexedDims = {65535, 50, 2};
        float[] indexedValues = {0.5f, 1800f, -34567.11f};
        BytesReference source = BytesReference.bytes(XContentFactory.jsonBuilder()
                .startObject()
                    .startObject("my-vector")
                        .field(Integer.toString(indexedDims[0]), indexedValues[0])
                        .field(Integer.toString(indexedDims[1]), indexedValues[1])
                        .field(Integer.toString(indexedDims[2]), indexedValues[2])
                    .endObject()
                .endObject());

        MapperParsingException indexException = expectThrows(MapperParsingException.class, () ->
            mapper.parse(new SourceToParse("index", "id", source, XContentType.JSON)));
        assertThat(indexException.getCause().getMessage(), containsString(SparseVectorFieldMapper.ERROR_MESSAGE));
    }
}
