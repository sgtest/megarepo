/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.action;

import org.elasticsearch.ElasticsearchTimeoutException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BlockFactory;
import org.elasticsearch.compute.data.BlockUtils;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.xpack.core.LocalStateCompositeXPackPlugin;
import org.elasticsearch.xpack.core.async.DeleteAsyncResultAction;
import org.elasticsearch.xpack.core.async.DeleteAsyncResultRequest;
import org.elasticsearch.xpack.core.async.GetAsyncResultRequest;
import org.elasticsearch.xpack.esql.TestBlockFactory;
import org.elasticsearch.xpack.esql.plugin.QueryPragmas;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.core.TimeValue.timeValueSeconds;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.core.IsEqual.equalTo;
import static org.hamcrest.core.IsNot.not;

/**
 * Runs test scenarios from EsqlActionIT, with an extra level of indirection
 * through the async query and async get APIs.
 */
public class EsqlAsyncActionIT extends EsqlActionIT {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        ArrayList<Class<? extends Plugin>> actions = new ArrayList<>(super.nodePlugins());
        actions.add(LocalStateEsqlAsync.class);
        return Collections.unmodifiableList(actions);
    }

    @Override
    protected EsqlQueryResponse run(String esqlCommands, QueryPragmas pragmas, QueryBuilder filter) {
        EsqlQueryRequest request = new EsqlQueryRequest();
        request.query(esqlCommands);
        request.pragmas(pragmas);
        request.async(true);
        // deliberately small timeout, to frequently trigger incomplete response
        request.waitForCompletionTimeout(TimeValue.timeValueNanos(1));
        request.keepOnCompletion(randomBoolean());
        if (filter != null) {
            request.filter(filter);
        }

        var response = run(request);
        if (response.asyncExecutionId().isPresent()) {
            List<ColumnInfo> initialColumns = null;
            List<Page> initialPages = null;
            String id = response.asyncExecutionId().get();
            if (response.isRunning() == false) {
                assertThat(request.keepOnCompletion(), is(true));
                assertThat(response.columns(), is(not(empty())));
                assertThat(response.pages(), is(not(empty())));
                initialColumns = List.copyOf(response.columns());
                initialPages = deepCopyOf(response.pages(), TestBlockFactory.getNonBreakingInstance());
            } else {
                assertThat(response.columns(), is(empty())); // no partial results
                assertThat(response.pages(), is(empty()));
            }
            response.close();
            var getResponse = getAsyncResponse(id);

            // assert initial contents, if any, are the same as async get contents
            if (initialColumns != null) {
                assertEquals(initialColumns, getResponse.columns());
                assertEquals(initialPages, getResponse.pages());
            }

            assertDeletable(id);
            return getResponse;
        } else {
            return response;
        }
    }

    void assertDeletable(String id) {
        var resp = deleteAsyncId(id);
        assertTrue(resp.isAcknowledged());
        // the stored response should no longer be retrievable
        var e = expectThrows(ResourceNotFoundException.class, () -> getAsyncResponse(id));
        assertThat(e.getMessage(), equalTo(id));
    }

    EsqlQueryResponse getAsyncResponse(String id) {
        try {
            var getResultsRequest = new GetAsyncResultRequest(id).setWaitForCompletionTimeout(timeValueSeconds(60));
            return client().execute(EsqlAsyncGetResultAction.INSTANCE, getResultsRequest).actionGet(30, TimeUnit.SECONDS);
        } catch (ElasticsearchTimeoutException e) {
            throw new AssertionError("timeout", e);
        }
    }

    AcknowledgedResponse deleteAsyncId(String id) {
        try {
            DeleteAsyncResultRequest request = new DeleteAsyncResultRequest(id);
            return client().execute(DeleteAsyncResultAction.INSTANCE, request).actionGet(30, TimeUnit.SECONDS);
        } catch (ElasticsearchTimeoutException e) {
            throw new AssertionError("timeout", e);
        }
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/102455")
    // junit.framework.AssertionFailedError: Unexpected exception type, expected VerificationException but got
    // org.elasticsearch.common.io.stream.NotSerializableExceptionWrapper: verification_exception: Found 1 problem
    @Override
    public void testOverlappingIndexPatterns() throws Exception {
        super.testOverlappingIndexPatterns();
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/102455")
    @Override
    public void testIndexPatterns() throws Exception {
        super.testOverlappingIndexPatterns();
    }

    public static class LocalStateEsqlAsync extends LocalStateCompositeXPackPlugin {
        public LocalStateEsqlAsync(final Settings settings, final Path configPath) {
            super(settings, configPath);
        }
    }

    // -- TODO: eventually remove and use common compute test infra

    public static List<Page> deepCopyOf(List<Page> pages, BlockFactory blockFactory) {
        return pages.stream().map(page -> deepCopyOf(page, blockFactory)).toList();
    }

    public static Page deepCopyOf(Page page, BlockFactory blockFactory) {
        Block[] blockCopies = new Block[page.getBlockCount()];
        for (int i = 0; i < blockCopies.length; i++) {
            blockCopies[i] = BlockUtils.deepCopyOf(page.getBlock(i), blockFactory);
        }
        return new Page(blockCopies);
    }
}
