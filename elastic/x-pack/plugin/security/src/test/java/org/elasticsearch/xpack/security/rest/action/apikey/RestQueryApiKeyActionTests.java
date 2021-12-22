/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.rest.action.apikey;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.MatchAllQueryBuilder;
import org.elasticsearch.index.query.PrefixQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.TermsQueryBuilder;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.rest.AbstractRestChannel;
import org.elasticsearch.rest.RestChannel;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.search.searchafter.SearchAfterBuilder;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.core.security.action.apikey.QueryApiKeyRequest;
import org.elasticsearch.xpack.core.security.action.apikey.QueryApiKeyResponse;

import java.util.List;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.mockito.Mockito.mock;

public class RestQueryApiKeyActionTests extends ESTestCase {

    private final XPackLicenseState mockLicenseState = mock(XPackLicenseState.class);
    private Settings settings;
    private ThreadPool threadPool;

    @Override
    public void setUp() throws Exception {
        super.setUp();
        settings = Settings.builder()
            .put("path.home", createTempDir().toString())
            .put("node.name", "test-" + getTestName())
            .put(Environment.PATH_HOME_SETTING.getKey(), createTempDir().toString())
            .build();
        threadPool = new ThreadPool(settings);
    }

    @Override
    public void tearDown() throws Exception {
        super.tearDown();
        terminate(threadPool);
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        final SearchModule searchModule = new SearchModule(Settings.EMPTY, List.of());
        return new NamedXContentRegistry(searchModule.getNamedXContents());
    }

    public void testQueryParsing() throws Exception {
        final String query1 = """
            {
              "query": {
                "bool": {
                  "must": [
                    {
                      "terms": {
                        "name": [ "k1", "k2" ]
                      }
                    }
                  ],
                  "should": [ { "prefix": { "metadata.environ": "prod" } } ]
                }
              }
            }""";
        final FakeRestRequest restRequest = new FakeRestRequest.Builder(xContentRegistry()).withContent(
            new BytesArray(query1),
            XContentType.JSON
        ).build();

        final SetOnce<RestResponse> responseSetOnce = new SetOnce<>();
        final RestChannel restChannel = new AbstractRestChannel(restRequest, randomBoolean()) {
            @Override
            public void sendResponse(RestResponse restResponse) {
                responseSetOnce.set(restResponse);
            }
        };

        try (NodeClient client = new NodeClient(Settings.EMPTY, threadPool) {
            @SuppressWarnings("unchecked")
            @Override
            public <Request extends ActionRequest, Response extends ActionResponse> void doExecute(
                ActionType<Response> action,
                Request request,
                ActionListener<Response> listener
            ) {
                QueryApiKeyRequest queryApiKeyRequest = (QueryApiKeyRequest) request;
                final QueryBuilder queryBuilder = queryApiKeyRequest.getQueryBuilder();
                assertNotNull(queryBuilder);
                assertThat(queryBuilder.getClass(), is(BoolQueryBuilder.class));
                final BoolQueryBuilder boolQueryBuilder = (BoolQueryBuilder) queryBuilder;
                assertTrue(boolQueryBuilder.filter().isEmpty());
                assertTrue(boolQueryBuilder.mustNot().isEmpty());
                assertThat(boolQueryBuilder.must(), hasSize(1));
                final QueryBuilder mustQueryBuilder = boolQueryBuilder.must().get(0);
                assertThat(mustQueryBuilder.getClass(), is(TermsQueryBuilder.class));
                assertThat(((TermsQueryBuilder) mustQueryBuilder).fieldName(), equalTo("name"));
                assertThat(boolQueryBuilder.should(), hasSize(1));
                final QueryBuilder shouldQueryBuilder = boolQueryBuilder.should().get(0);
                assertThat(shouldQueryBuilder.getClass(), is(PrefixQueryBuilder.class));
                assertThat(((PrefixQueryBuilder) shouldQueryBuilder).fieldName(), equalTo("metadata.environ"));
                listener.onResponse((Response) new QueryApiKeyResponse(0, List.of()));
            }
        }) {
            final RestQueryApiKeyAction restQueryApiKeyAction = new RestQueryApiKeyAction(Settings.EMPTY, mockLicenseState);
            restQueryApiKeyAction.handleRequest(restRequest, restChannel, client);
        }

        assertNotNull(responseSetOnce.get());
    }

    public void testParsingSearchParameters() throws Exception {
        final String requestBody = """
            {
              "query": {
                "match_all": {}
              },
              "from": 42,
              "size": 20,
              "sort": [ "name", { "creation_time": { "order": "desc", "format": "strict_date_time" } }, "username" ],
              "search_after": [ "key-2048", "2021-07-01T00:00:59.000Z" ]
            }""";

        final FakeRestRequest restRequest = new FakeRestRequest.Builder(xContentRegistry()).withContent(
            new BytesArray(requestBody),
            XContentType.JSON
        ).build();

        final SetOnce<RestResponse> responseSetOnce = new SetOnce<>();
        final RestChannel restChannel = new AbstractRestChannel(restRequest, randomBoolean()) {
            @Override
            public void sendResponse(RestResponse restResponse) {
                responseSetOnce.set(restResponse);
            }
        };

        try (NodeClient client = new NodeClient(Settings.EMPTY, threadPool) {
            @SuppressWarnings("unchecked")
            @Override
            public <Request extends ActionRequest, Response extends ActionResponse> void doExecute(
                ActionType<Response> action,
                Request request,
                ActionListener<Response> listener
            ) {
                QueryApiKeyRequest queryApiKeyRequest = (QueryApiKeyRequest) request;
                final QueryBuilder queryBuilder = queryApiKeyRequest.getQueryBuilder();
                assertNotNull(queryBuilder);
                assertThat(queryBuilder.getClass(), is(MatchAllQueryBuilder.class));
                assertThat(queryApiKeyRequest.getFrom(), equalTo(42));
                assertThat(queryApiKeyRequest.getSize(), equalTo(20));
                final List<FieldSortBuilder> fieldSortBuilders = queryApiKeyRequest.getFieldSortBuilders();
                assertThat(fieldSortBuilders, hasSize(3));

                assertThat(fieldSortBuilders.get(0), equalTo(new FieldSortBuilder("name")));
                assertThat(
                    fieldSortBuilders.get(1),
                    equalTo(new FieldSortBuilder("creation_time").setFormat("strict_date_time").order(SortOrder.DESC))
                );
                assertThat(fieldSortBuilders.get(2), equalTo(new FieldSortBuilder("username")));

                final SearchAfterBuilder searchAfterBuilder = queryApiKeyRequest.getSearchAfterBuilder();
                assertThat(
                    searchAfterBuilder,
                    equalTo(new SearchAfterBuilder().setSortValues(new String[] { "key-2048", "2021-07-01T00:00:59.000Z" }))
                );

                listener.onResponse((Response) new QueryApiKeyResponse(0, List.of()));
            }
        }) {
            final RestQueryApiKeyAction restQueryApiKeyAction = new RestQueryApiKeyAction(Settings.EMPTY, mockLicenseState);
            restQueryApiKeyAction.handleRequest(restRequest, restChannel, client);
        }

        assertNotNull(responseSetOnce.get());
    }
}
