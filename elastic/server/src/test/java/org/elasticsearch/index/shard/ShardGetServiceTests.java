/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.index.shard;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.VersionType;
import org.elasticsearch.index.engine.Engine;
import org.elasticsearch.index.engine.EngineTestCase;
import org.elasticsearch.index.engine.VersionConflictEngineException;
import org.elasticsearch.index.get.GetResult;
import org.elasticsearch.index.mapper.RoutingFieldMapper;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;

import java.io.IOException;
import java.nio.charset.StandardCharsets;

import static org.elasticsearch.index.seqno.SequenceNumbers.UNASSIGNED_PRIMARY_TERM;
import static org.elasticsearch.index.seqno.SequenceNumbers.UNASSIGNED_SEQ_NO;

public class ShardGetServiceTests extends IndexShardTestCase {

    public void testGetForUpdate() throws IOException {
        Settings settings = Settings.builder().put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)

            .build();
        IndexMetadata metadata = IndexMetadata.builder("test")
            .putMapping("{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metadata.getIndex(), 0), true, "n1", metadata, null);
        recoverShardFromStore(primary);
        Engine.IndexResult test = indexDoc(primary, "test", "0", "{\"foo\" : \"bar\"}");
        assertTrue(primary.getEngine().refreshNeeded());
        GetResult testGet = primary.getService().getForUpdate("0", UNASSIGNED_SEQ_NO, UNASSIGNED_PRIMARY_TERM);
        assertFalse(testGet.getFields().containsKey(RoutingFieldMapper.NAME));
        assertEquals(new String(testGet.source(), StandardCharsets.UTF_8), "{\"foo\" : \"bar\"}");
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 1); // we refreshed
        }

        Engine.IndexResult test1 = indexDoc(primary, "1", "{\"foo\" : \"baz\"}",  XContentType.JSON, "foobar");
        assertTrue(primary.getEngine().refreshNeeded());
        GetResult testGet1 = primary.getService().getForUpdate("1", UNASSIGNED_SEQ_NO, UNASSIGNED_PRIMARY_TERM);
        assertEquals(new String(testGet1.source(), StandardCharsets.UTF_8), "{\"foo\" : \"baz\"}");
        assertTrue(testGet1.getFields().containsKey(RoutingFieldMapper.NAME));
        assertEquals("foobar", testGet1.getFields().get(RoutingFieldMapper.NAME).getValue());
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 1); // we read from the translog
        }
        primary.getEngine().refresh("test");
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 2);
        }

        // now again from the reader
        Engine.IndexResult test2 = indexDoc(primary, "1", "{\"foo\" : \"baz\"}",  XContentType.JSON, "foobar");
        assertTrue(primary.getEngine().refreshNeeded());
        testGet1 = primary.getService().getForUpdate("1", UNASSIGNED_SEQ_NO, UNASSIGNED_PRIMARY_TERM);
        assertEquals(new String(testGet1.source(), StandardCharsets.UTF_8), "{\"foo\" : \"baz\"}");
        assertTrue(testGet1.getFields().containsKey(RoutingFieldMapper.NAME));
        assertEquals("foobar", testGet1.getFields().get(RoutingFieldMapper.NAME).getValue());

        final long primaryTerm = primary.getOperationPrimaryTerm();
        testGet1 = primary.getService().getForUpdate("1", test2.getSeqNo(), primaryTerm);
        assertEquals(new String(testGet1.source(), StandardCharsets.UTF_8), "{\"foo\" : \"baz\"}");

        expectThrows(VersionConflictEngineException.class, () ->
            primary.getService().getForUpdate("1", test2.getSeqNo() + 1, primaryTerm));
        expectThrows(VersionConflictEngineException.class, () ->
            primary.getService().getForUpdate("1", test2.getSeqNo(), primaryTerm + 1));
        closeShards(primary);
    }

    public void testGetFromTranslogWithStringSourceMappingOptionsAndStoredFields() throws IOException {
        String docToIndex = "{\"foo\" : \"foo\", \"bar\" : \"bar\"}";
        boolean noSource = randomBoolean();
        String sourceOptions = noSource ? "\"enabled\": false" : randomBoolean() ? "\"excludes\": [\"fo*\"]" : "\"includes\": [\"ba*\"]";
        runGetFromTranslogWithOptions(docToIndex, sourceOptions, noSource ? "" : "{\"bar\":\"bar\"}", "\"text\"", "foo");
    }

    public void testGetFromTranslogWithLongSourceMappingOptionsAndStoredFields() throws IOException {
        String docToIndex = "{\"foo\" : 7, \"bar\" : 42}";
        boolean noSource = randomBoolean();
        String sourceOptions = noSource ? "\"enabled\": false" : randomBoolean() ? "\"excludes\": [\"fo*\"]" : "\"includes\": [\"ba*\"]";
        runGetFromTranslogWithOptions(docToIndex, sourceOptions, noSource ? "" : "{\"bar\":42}", "\"long\"", 7L);
    }

    private void runGetFromTranslogWithOptions(String docToIndex, String sourceOptions, String expectedResult, String fieldType,
                                               Object expectedFooVal) throws IOException {
        Settings settings = Settings.builder().put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
            .build();

        IndexMetadata metadata = IndexMetadata.builder("test")
            .putMapping("{ \"properties\": { \"foo\":  { \"type\": " + fieldType + ", \"store\": true }, " +
                "\"bar\":  { \"type\": " + fieldType + "}}, \"_source\": { " + sourceOptions + "}}}")
            .settings(settings)
            .primaryTerm(0, 1).build();
        IndexShard primary = newShard(new ShardId(metadata.getIndex(), 0), true, "n1", metadata, EngineTestCase.randomReaderWrapper());
        recoverShardFromStore(primary);
        Engine.IndexResult test = indexDoc(primary, "test", "0", docToIndex);
        assertTrue(primary.getEngine().refreshNeeded());
        GetResult testGet = primary.getService().getForUpdate("0", UNASSIGNED_SEQ_NO, UNASSIGNED_PRIMARY_TERM);
        assertFalse(testGet.getFields().containsKey(RoutingFieldMapper.NAME));
        assertEquals(new String(testGet.source() == null ? new byte[0] : testGet.source(), StandardCharsets.UTF_8), expectedResult);
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 1); // we refreshed
        }

        Engine.IndexResult test1 = indexDoc(primary, "1", docToIndex,  XContentType.JSON, "foobar");
        assertTrue(primary.getEngine().refreshNeeded());
        GetResult testGet1 = primary.getService().getForUpdate("1", UNASSIGNED_SEQ_NO, UNASSIGNED_PRIMARY_TERM);
        assertEquals(new String(testGet1.source() == null ? new byte[0] : testGet1.source(), StandardCharsets.UTF_8), expectedResult);
        assertTrue(testGet1.getFields().containsKey(RoutingFieldMapper.NAME));
        assertEquals("foobar", testGet1.getFields().get(RoutingFieldMapper.NAME).getValue());
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 1); // we read from the translog
        }
        primary.getEngine().refresh("test");
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 2);
        }

        Engine.IndexResult test2 = indexDoc(primary, "2", docToIndex,  XContentType.JSON, "foobar");
        assertTrue(primary.getEngine().refreshNeeded());
        GetResult testGet2 = primary.getService().get("2", new String[]{"foo"}, true, 1, VersionType.INTERNAL,
            FetchSourceContext.FETCH_SOURCE);
        assertEquals(new String(testGet2.source() == null ? new byte[0] : testGet2.source(), StandardCharsets.UTF_8), expectedResult);
        assertTrue(testGet2.getFields().containsKey(RoutingFieldMapper.NAME));
        assertTrue(testGet2.getFields().containsKey("foo"));
        assertEquals(expectedFooVal, testGet2.getFields().get("foo").getValue());
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 2); // we read from the translog
        }
        primary.getEngine().refresh("test");
        try (Engine.Searcher searcher = primary.getEngine().acquireSearcher("test", Engine.SearcherScope.INTERNAL)) {
            assertEquals(searcher.getIndexReader().maxDoc(), 3);
        }

        testGet2 = primary.getService().get("2", new String[]{"foo"}, true, 1, VersionType.INTERNAL,
            FetchSourceContext.FETCH_SOURCE);
        assertEquals(new String(testGet2.source() == null ? new byte[0] : testGet2.source(), StandardCharsets.UTF_8), expectedResult);
        assertTrue(testGet2.getFields().containsKey(RoutingFieldMapper.NAME));
        assertTrue(testGet2.getFields().containsKey("foo"));
        assertEquals(expectedFooVal, testGet2.getFields().get("foo").getValue());

        closeShards(primary);
    }

    public void testTypelessGetForUpdate() throws IOException {
        Settings settings = Settings.builder().put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
                .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1)
                .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
                .build();
        IndexMetadata metadata = IndexMetadata.builder("index")
                .putMapping("{ \"properties\": { \"foo\":  { \"type\": \"text\"}}}")
                .settings(settings)
                .primaryTerm(0, 1).build();
        IndexShard shard = newShard(new ShardId(metadata.getIndex(), 0), true, "n1", metadata, null);
        recoverShardFromStore(shard);
        Engine.IndexResult indexResult = indexDoc(shard, "some_type", "0", "{\"foo\" : \"bar\"}");
        assertTrue(indexResult.isCreated());

        GetResult getResult = shard.getService().getForUpdate( "0", UNASSIGNED_SEQ_NO, UNASSIGNED_PRIMARY_TERM);
        assertTrue(getResult.isExists());

        closeShards(shard);
    }
}
