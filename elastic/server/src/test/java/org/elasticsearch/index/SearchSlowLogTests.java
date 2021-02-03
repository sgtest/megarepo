/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index;

import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.core.LoggerContext;
import org.elasticsearch.Version;
import org.elasticsearch.action.search.SearchShardTask;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.logging.ESLogMessage;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.logging.MockAppender;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.search.internal.ShardSearchRequest;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESSingleNodeTestCase;
import org.elasticsearch.test.TestSearchContext;
import org.hamcrest.Matchers;
import org.junit.AfterClass;
import org.junit.BeforeClass;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasToString;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.not;

public class SearchSlowLogTests extends ESSingleNodeTestCase {
    static MockAppender appender;
    static Logger queryLog = LogManager.getLogger(SearchSlowLog.INDEX_SEARCH_SLOWLOG_PREFIX + ".query");
    static Logger fetchLog = LogManager.getLogger(SearchSlowLog.INDEX_SEARCH_SLOWLOG_PREFIX + ".fetch");

    @BeforeClass
    public static void init() throws IllegalAccessException {
        appender = new MockAppender("trace_appender");
        appender.start();
        Loggers.addAppender(queryLog, appender);
        Loggers.addAppender(fetchLog, appender);
    }

    @AfterClass
    public static void cleanup() {
        appender.stop();
        Loggers.removeAppender(queryLog, appender);
        Loggers.removeAppender(fetchLog, appender);
    }

    @Override
    protected SearchContext createSearchContext(IndexService indexService) {
        return createSearchContext(indexService, new String[]{});
    }

    protected SearchContext createSearchContext(IndexService indexService, String... groupStats) {
        final ShardSearchRequest request =
            new ShardSearchRequest(new ShardId(indexService.index(), 0), 0L, null);
        return new TestSearchContext(indexService) {
            @Override
            public List<String> groupStats() {
                return Arrays.asList(groupStats);
            }

            @Override
            public ShardSearchRequest request() {
                return request;
            }

            @Override
            public SearchShardTask getTask() {
                return super.getTask();
            }
        };
    }

    public void testLevelPrecedence() {
        SearchContext ctx = searchContextWithSourceAndTask(createIndex("index"));
        String uuid = UUIDs.randomBase64UUID();
        IndexSettings settings =
            new IndexSettings(createIndexMetadata("index", settings(uuid)), Settings.EMPTY);
        SearchSlowLog log = new SearchSlowLog(settings);

        // For this test, when level is not breached, the level below should be used.
        {
            log.onQueryPhase(ctx, 40L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.INFO));
            log.onQueryPhase(ctx, 41L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.WARN));

            log.onFetchPhase(ctx, 40L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.INFO));
            log.onFetchPhase(ctx, 41L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.WARN));
        }

        {
            log.onQueryPhase(ctx, 30L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.DEBUG));
            log.onQueryPhase(ctx, 31L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.INFO));

            log.onFetchPhase(ctx, 30L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.DEBUG));
            log.onFetchPhase(ctx, 31L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.INFO));
        }

        {
            log.onQueryPhase(ctx, 20L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.TRACE));
            log.onQueryPhase(ctx, 21L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.DEBUG));

            log.onFetchPhase(ctx, 20L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.TRACE));
            log.onFetchPhase(ctx, 21L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.DEBUG));
        }

        {
            log.onQueryPhase(ctx, 10L);
            assertNull(appender.getLastEventAndReset());
            log.onQueryPhase(ctx, 11L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.TRACE));

            log.onFetchPhase(ctx, 10L);
            assertNull(appender.getLastEventAndReset());
            log.onFetchPhase(ctx, 11L);
            assertThat(appender.getLastEventAndReset().getLevel(), equalTo(Level.TRACE));
        }
    }

    private Settings.Builder settings(String uuid) {
        return Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetadata.SETTING_INDEX_UUID, uuid)
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_TRACE_SETTING.getKey(), "10nanos")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_DEBUG_SETTING.getKey(), "20nanos")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_INFO_SETTING.getKey(), "30nanos")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_WARN_SETTING.getKey(), "40nanos")

            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_TRACE_SETTING.getKey(), "10nanos")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_DEBUG_SETTING.getKey(), "20nanos")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_INFO_SETTING.getKey(), "30nanos")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_WARN_SETTING.getKey(), "40nanos");
    }

    public void testTwoLoggersDifferentLevel() {
        SearchContext ctx1 = searchContextWithSourceAndTask(createIndex("index-1"));
        SearchContext ctx2 = searchContextWithSourceAndTask(createIndex("index-2"));
        IndexSettings settings1 =
            new IndexSettings(createIndexMetadata("index-1", Settings.builder()
                .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
                .put(IndexMetadata.SETTING_INDEX_UUID, UUIDs.randomBase64UUID())
                .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_WARN_SETTING.getKey(), "40nanos")
                .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_WARN_SETTING.getKey(), "40nanos")), Settings.EMPTY);
        SearchSlowLog log1 = new SearchSlowLog(settings1);

        IndexSettings settings2 =
            new IndexSettings(createIndexMetadata("index-2", Settings.builder()
                .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
                .put(IndexMetadata.SETTING_INDEX_UUID, UUIDs.randomBase64UUID())
                .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_TRACE_SETTING.getKey(), "10nanos")
                .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_TRACE_SETTING.getKey(), "10nanos")), Settings.EMPTY);
        SearchSlowLog log2 = new SearchSlowLog(settings2);

        {
            // threshold set on WARN only, should not log
            log1.onQueryPhase(ctx1, 11L);
            assertNull(appender.getLastEventAndReset());
            log1.onFetchPhase(ctx1, 11L);
            assertNull(appender.getLastEventAndReset());

            // threshold set on TRACE, should log
            log2.onQueryPhase(ctx2, 11L);
            assertNotNull(appender.getLastEventAndReset());
            log2.onFetchPhase(ctx2, 11L);
            assertNotNull(appender.getLastEventAndReset());
        }
    }

    public void testMultipleSlowLoggersUseSingleLog4jLogger() {
        LoggerContext context = (LoggerContext) LogManager.getContext(false);

        SearchContext ctx1 = searchContextWithSourceAndTask(createIndex("index-1"));
        IndexSettings settings1 =
            new IndexSettings(createIndexMetadata("index-1", settings(UUIDs.randomBase64UUID())), Settings.EMPTY);
        SearchSlowLog log1 = new SearchSlowLog(settings1);
        int numberOfLoggersBefore = context.getLoggers().size();

        SearchContext ctx2 = searchContextWithSourceAndTask(createIndex("index-2"));
        IndexSettings settings2 =
            new IndexSettings(createIndexMetadata("index-2", settings(UUIDs.randomBase64UUID())), Settings.EMPTY);
        SearchSlowLog log2 = new SearchSlowLog(settings2);

        int numberOfLoggersAfter = context.getLoggers().size();
        assertThat(numberOfLoggersAfter, equalTo(numberOfLoggersBefore));
    }

    private IndexMetadata createIndexMetadata(String index, Settings.Builder put) {
        return newIndexMeta(index, put.build());
    }

    public void testSlowLogHasJsonFields() throws IOException {
        IndexService index = createIndex("foo");
        SearchContext searchContext = searchContextWithSourceAndTask(index);
        ESLogMessage p = SearchSlowLog.SearchSlowLogMessage.of(searchContext, 10);

        assertThat(p.get("message"), equalTo("[foo][0]"));
        assertThat(p.get("took"), equalTo("10nanos"));
        assertThat(p.get("took_millis"), equalTo("0"));
        assertThat(p.get("total_hits"), equalTo("-1"));
        assertThat(p.get("stats"), equalTo("[]"));
        assertThat(p.get("search_type"), Matchers.nullValue());
        assertThat(p.get("total_shards"), equalTo("1"));
        assertThat(p.get("source"), equalTo("{\\\"query\\\":{\\\"match_all\\\":{\\\"boost\\\":1.0}}}"));
    }


    public void testSlowLogsWithStats() throws IOException {
        IndexService index = createIndex("foo");
        SearchContext searchContext = createSearchContext(index, "group1");
        SearchSourceBuilder source = SearchSourceBuilder.searchSource().query(QueryBuilders.matchAllQuery());
        searchContext.request().source(source);
        searchContext.setTask(new SearchShardTask(0, "n/a", "n/a", "test", null,
            Collections.singletonMap(Task.X_OPAQUE_ID, "my_id")));

        ESLogMessage p = SearchSlowLog.SearchSlowLogMessage.of(searchContext, 10);
        assertThat(p.get("stats"), equalTo("[\\\"group1\\\"]"));

        searchContext = createSearchContext(index, "group1", "group2");
        source = SearchSourceBuilder.searchSource().query(QueryBuilders.matchAllQuery());
        searchContext.request().source(source);
        searchContext.setTask(new SearchShardTask(0, "n/a", "n/a", "test", null,
            Collections.singletonMap(Task.X_OPAQUE_ID, "my_id")));
        p = SearchSlowLog.SearchSlowLogMessage.of(searchContext, 10);
        assertThat(p.get("stats"), equalTo("[\\\"group1\\\", \\\"group2\\\"]"));
    }

    public void testSlowLogSearchContextPrinterToLog() throws IOException {
        IndexService index = createIndex("foo");
        SearchContext searchContext = searchContextWithSourceAndTask(index);
        ESLogMessage p = SearchSlowLog.SearchSlowLogMessage.of(searchContext, 10);
        assertThat(p.get("message"), equalTo("[foo][0]"));
        // Makes sure that output doesn't contain any new lines
        assertThat(p.get("source"), not(containsString("\n")));
        assertThat(p.get("id"), equalTo("my_id"));
    }

    public void testSetQueryLevels() {
        IndexMetadata metadata = newIndexMeta("index", Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_TRACE_SETTING.getKey(), "100ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_DEBUG_SETTING.getKey(), "200ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_INFO_SETTING.getKey(), "300ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_WARN_SETTING.getKey(), "400ms")
            .build());
        IndexSettings settings = new IndexSettings(metadata, Settings.EMPTY);
        SearchSlowLog log = new SearchSlowLog(settings);
        assertEquals(TimeValue.timeValueMillis(100).nanos(), log.getQueryTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(200).nanos(), log.getQueryDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(300).nanos(), log.getQueryInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(400).nanos(), log.getQueryWarnThreshold());

        settings.updateIndexMetadata(newIndexMeta("index",
            Settings.builder().put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_TRACE_SETTING.getKey(), "120ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_DEBUG_SETTING.getKey(), "220ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_INFO_SETTING.getKey(), "320ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_WARN_SETTING.getKey(), "420ms").build()));


        assertEquals(TimeValue.timeValueMillis(120).nanos(), log.getQueryTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(220).nanos(), log.getQueryDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(320).nanos(), log.getQueryInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(420).nanos(), log.getQueryWarnThreshold());

        metadata = newIndexMeta("index", Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .build());
        settings.updateIndexMetadata(metadata);
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryWarnThreshold());

        settings = new IndexSettings(metadata, Settings.EMPTY);
        log = new SearchSlowLog(settings);

        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getQueryWarnThreshold());
        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder()
                    .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_TRACE_SETTING.getKey(), "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.query.trace");
        }

        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder()
                    .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_DEBUG_SETTING.getKey(), "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.query.debug");
        }

        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder()
                    .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_INFO_SETTING.getKey(), "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.query.info");
        }

        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder()
                    .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_WARN_SETTING.getKey(), "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.query.warn");
        }
    }

    public void testSetFetchLevels() {
        IndexMetadata metadata = newIndexMeta("index", Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_TRACE_SETTING.getKey(), "100ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_DEBUG_SETTING.getKey(), "200ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_INFO_SETTING.getKey(), "300ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_WARN_SETTING.getKey(), "400ms")
            .build());
        IndexSettings settings = new IndexSettings(metadata, Settings.EMPTY);
        SearchSlowLog log = new SearchSlowLog(settings);
        assertEquals(TimeValue.timeValueMillis(100).nanos(), log.getFetchTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(200).nanos(), log.getFetchDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(300).nanos(), log.getFetchInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(400).nanos(), log.getFetchWarnThreshold());

        settings.updateIndexMetadata(newIndexMeta("index",
            Settings.builder().put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_TRACE_SETTING.getKey(), "120ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_DEBUG_SETTING.getKey(), "220ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_INFO_SETTING.getKey(), "320ms")
            .put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_WARN_SETTING.getKey(), "420ms").build()));


        assertEquals(TimeValue.timeValueMillis(120).nanos(), log.getFetchTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(220).nanos(), log.getFetchDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(320).nanos(), log.getFetchInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(420).nanos(), log.getFetchWarnThreshold());

        metadata = newIndexMeta("index", Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .build());
        settings.updateIndexMetadata(metadata);
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchWarnThreshold());

        settings = new IndexSettings(metadata, Settings.EMPTY);
        log = new SearchSlowLog(settings);

        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchTraceThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchDebugThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchInfoThreshold());
        assertEquals(TimeValue.timeValueMillis(-1).nanos(), log.getFetchWarnThreshold());
        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder().put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_TRACE_SETTING.getKey(),
                    "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.fetch.trace");
        }

        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder().put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_DEBUG_SETTING.getKey(),
                    "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.fetch.debug");
        }

        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder().put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_INFO_SETTING.getKey(),
                    "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.fetch.info");
        }

        try {
            settings.updateIndexMetadata(newIndexMeta("index",
                Settings.builder().put(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_WARN_SETTING.getKey(),
                    "NOT A TIME VALUE").build()));
            fail();
        } catch (IllegalArgumentException ex) {
            assertTimeValueException(ex, "index.search.slowlog.threshold.fetch.warn");
        }
    }

    private void assertTimeValueException(final IllegalArgumentException e, final String key) {
        final String expected = "illegal value can't update [" + key + "] from [-1] to [NOT A TIME VALUE]";
        assertThat(e, hasToString(containsString(expected)));
        assertNotNull(e.getCause());
        assertThat(e.getCause(), instanceOf(IllegalArgumentException.class));
        final IllegalArgumentException cause = (IllegalArgumentException) e.getCause();
        final String causeExpected =
                "failed to parse setting [" + key + "] with value [NOT A TIME VALUE] as a time value: unit is missing or unrecognized";
        assertThat(cause, hasToString(containsString(causeExpected)));
    }

    private IndexMetadata newIndexMeta(String name, Settings indexSettings) {
        Settings build = Settings.builder().put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1)
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
            .put(indexSettings)
            .build();
        IndexMetadata metadata = IndexMetadata.builder(name).settings(build).build();
        return metadata;
    }

    private SearchContext searchContextWithSourceAndTask(IndexService index) {
        SearchContext ctx = createSearchContext(index);
        SearchSourceBuilder source = SearchSourceBuilder.searchSource().query(QueryBuilders.matchAllQuery());
        ctx.request().source(source);
        ctx.setTask(new SearchShardTask(0, "n/a", "n/a", "test", null,
            Collections.singletonMap(Task.X_OPAQUE_ID, "my_id")));
        return ctx;
    }
}
