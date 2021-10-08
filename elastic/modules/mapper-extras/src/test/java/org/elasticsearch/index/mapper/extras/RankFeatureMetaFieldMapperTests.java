/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper.extras;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.xcontent.XContentFactory;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.index.mapper.DocumentMapper;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.Mapping;
import org.elasticsearch.index.mapper.SourceToParse;
import org.elasticsearch.index.mapper.extras.MapperExtrasPlugin;
import org.elasticsearch.index.mapper.extras.RankFeatureMetaFieldMapper;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.test.ESSingleNodeTestCase;
import org.hamcrest.CoreMatchers;
import org.junit.Before;

import java.util.Collection;

public class RankFeatureMetaFieldMapperTests extends ESSingleNodeTestCase {

    MapperService mapperService;

    @Before
    public void setup() {
        mapperService = createIndex("test").mapperService();
    }

    @Override
    protected Collection<Class<? extends Plugin>> getPlugins() {
        return pluginList(MapperExtrasPlugin.class);
    }

    public void testBasics() throws Exception {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
                .startObject("properties").startObject("field").field("type", "rank_feature").endObject().endObject()
                .endObject().endObject());

        Mapping parsedMapping = mapperService.parseMapping("type", new CompressedXContent(mapping));
        assertEquals(mapping, parsedMapping.toCompressedXContent().toString());
        assertNotNull(parsedMapping.getMetadataMapperByClass(RankFeatureMetaFieldMapper.class));
    }

    /**
     * Check that meta-fields are picked from plugins (in this case MapperExtrasPlugin),
     * and parsing of a document fails if the document contains these meta-fields.
     */
    public void testDocumentParsingFailsOnMetaField() throws Exception {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("_doc").endObject().endObject());
        DocumentMapper mapper = mapperService.merge("_doc", new CompressedXContent(mapping), MapperService.MergeReason.MAPPING_UPDATE);
        String rfMetaField = RankFeatureMetaFieldMapper.CONTENT_TYPE;
        BytesReference bytes = BytesReference.bytes(XContentFactory.jsonBuilder().startObject().field(rfMetaField, 0).endObject());
        MapperParsingException e = expectThrows(MapperParsingException.class, () ->
            mapper.parse(new SourceToParse("test", "1", bytes, XContentType.JSON)));
        assertThat(e.getCause().getMessage(),
            CoreMatchers.containsString("Field ["+ rfMetaField + "] is a metadata field and cannot be added inside a document."));
    }
}
