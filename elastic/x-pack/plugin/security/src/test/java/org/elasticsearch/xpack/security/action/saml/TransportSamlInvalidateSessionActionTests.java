/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.action.saml;

import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.ClearScrollAction;
import org.elasticsearch.action.search.ClearScrollRequest;
import org.elasticsearch.action.search.ClearScrollResponse;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchResponseSections;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.io.PathUtils;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.TermQueryBuilder;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.test.client.NoOpClient;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.Transport;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.action.saml.SamlInvalidateSessionRequest;
import org.elasticsearch.xpack.core.security.action.saml.SamlInvalidateSessionResponse;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.Authentication.RealmRef;
import org.elasticsearch.xpack.core.security.authc.RealmConfig;
import org.elasticsearch.xpack.core.security.authc.esnative.NativeRealmSettings;
import org.elasticsearch.xpack.core.security.authc.saml.SamlRealmSettings;
import org.elasticsearch.protocol.xpack.security.User;
import org.elasticsearch.xpack.security.authc.Realms;
import org.elasticsearch.xpack.security.authc.TokenService;
import org.elasticsearch.xpack.security.authc.UserToken;
import org.elasticsearch.xpack.security.authc.saml.SamlLogoutRequestHandler;
import org.elasticsearch.xpack.security.authc.saml.SamlNameId;
import org.elasticsearch.xpack.security.authc.saml.SamlRealm;
import org.elasticsearch.xpack.security.authc.saml.SamlRealmTestHelper;
import org.elasticsearch.xpack.security.authc.saml.SamlRealmTests;
import org.elasticsearch.xpack.security.authc.saml.SamlTestCase;
import org.elasticsearch.xpack.security.support.SecurityIndexManager;
import org.junit.After;
import org.junit.Before;
import org.opensaml.saml.saml2.core.NameID;

import java.io.IOException;
import java.nio.file.Path;
import java.time.Clock;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.iterableWithSize;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.startsWith;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyString;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TransportSamlInvalidateSessionActionTests extends SamlTestCase {

    private SamlRealm samlRealm;
    private TokenService tokenService;
    private List<IndexRequest> indexRequests;
    private List<UpdateRequest> updateRequests;
    private List<SearchRequest> searchRequests;
    private TransportSamlInvalidateSessionAction action;
    private SamlLogoutRequestHandler.Result logoutRequest;
    private Function<SearchRequest, SearchHit[]> searchFunction = ignore -> new SearchHit[0];

    @Before
    public void setup() throws Exception {
        final Settings settings = Settings.builder()
                .put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), true)
                .put("path.home", createTempDir())
                .build();

        final ThreadContext threadContext = new ThreadContext(settings);
        final ThreadPool threadPool = mock(ThreadPool.class);
        when(threadPool.getThreadContext()).thenReturn(threadContext);
        new Authentication(new User("kibana"), new RealmRef("realm", "type", "node"), null).writeToContext(threadContext);

        indexRequests = new ArrayList<>();
        updateRequests = new ArrayList<>();
        searchRequests = new ArrayList<>();
        final Client client = new NoOpClient(threadPool) {
            @Override
            protected <Request extends ActionRequest, Response extends ActionResponse>
            void doExecute(Action<Response> action, Request request, ActionListener<Response> listener) {
                if (IndexAction.NAME.equals(action.name())) {
                    assertThat(request, instanceOf(IndexRequest.class));
                    IndexRequest indexRequest = (IndexRequest) request;
                    indexRequests.add(indexRequest);
                    final IndexResponse response = new IndexResponse(
                            indexRequest.shardId(), indexRequest.type(), indexRequest.id(), 1, 1, 1, true);
                    listener.onResponse((Response) response);
                } else if (UpdateAction.NAME.equals(action.name())) {
                    assertThat(request, instanceOf(UpdateRequest.class));
                    updateRequests.add((UpdateRequest) request);
                    listener.onResponse((Response) new UpdateResponse());
                } else if (SearchAction.NAME.equals(action.name())) {
                    assertThat(request, instanceOf(SearchRequest.class));
                    SearchRequest searchRequest = (SearchRequest) request;
                    searchRequests.add(searchRequest);
                    final SearchHit[] hits = searchFunction.apply(searchRequest);
                    final SearchResponse response = new SearchResponse(
                            new SearchResponseSections(new SearchHits(hits, hits.length, 0f),
                                    null, null, false, false, null, 1), "_scrollId1", 1, 1, 0, 1, null, null);
                    listener.onResponse((Response) response);
                } else if (ClearScrollAction.NAME.equals(action.name())) {
                    assertThat(request, instanceOf(ClearScrollRequest.class));
                    ClearScrollRequest scrollRequest = (ClearScrollRequest) request;
                    assertEquals("_scrollId1", scrollRequest.getScrollIds().get(0));
                    ClearScrollResponse response = new ClearScrollResponse(true, 1);
                    listener.onResponse((Response) response);
                } else {
                    super.doExecute(action, request, listener);
                }
            }
        };

        final SecurityIndexManager securityIndex = mock(SecurityIndexManager.class);
        doAnswer(inv -> {
            ((Runnable) inv.getArguments()[1]).run();
            return null;
        }).when(securityIndex).prepareIndexIfNeededThenExecute(any(Consumer.class), any(Runnable.class));

        final ClusterService clusterService = ClusterServiceUtils.createClusterService(threadPool);
        tokenService = new TokenService(settings, Clock.systemUTC(), client, securityIndex, clusterService);

        final TransportService transportService = new TransportService(Settings.EMPTY, mock(Transport.class), null,
                TransportService.NOOP_TRANSPORT_INTERCEPTOR, x -> null, null, Collections.emptySet());
        final Realms realms = mock(Realms.class);
        action = new TransportSamlInvalidateSessionAction(settings, transportService, mock(ActionFilters.class),tokenService, realms);

        final Path metadata = PathUtils.get(SamlRealm.class.getResource("idp1.xml").toURI());
        final Environment env = TestEnvironment.newEnvironment(settings);
        final Settings realmSettings = Settings.builder()
                .put(SamlRealmSettings.IDP_METADATA_PATH.getKey(), metadata.toString())
                .put(SamlRealmSettings.IDP_ENTITY_ID.getKey(), SamlRealmTests.TEST_IDP_ENTITY_ID)
                .put(SamlRealmSettings.SP_ENTITY_ID.getKey(), SamlRealmTestHelper.SP_ENTITY_ID)
                .put(SamlRealmSettings.SP_ACS.getKey(), SamlRealmTestHelper.SP_ACS_URL)
                .put(SamlRealmSettings.SP_LOGOUT.getKey(), SamlRealmTestHelper.SP_LOGOUT_URL)
                .put("attributes.principal", "uid")
                .build();

        final RealmConfig realmConfig = new RealmConfig("saml1", realmSettings, settings, env, threadContext);
        samlRealm = SamlRealmTestHelper.buildRealm(realmConfig, null);
        when(realms.realm(realmConfig.name())).thenReturn(samlRealm);
        when(realms.stream()).thenAnswer(i -> Stream.of(samlRealm));

        logoutRequest = new SamlLogoutRequestHandler.Result(
                randomAlphaOfLengthBetween(8, 24),
                new SamlNameId(NameID.TRANSIENT, randomAlphaOfLengthBetween(8, 24), null, null, null),
                randomAlphaOfLengthBetween(12, 16),
                null
        );
        when(samlRealm.getLogoutHandler().parseFromQueryString(anyString())).thenReturn(logoutRequest);
    }

    private SearchHit tokenHit(int idx, BytesReference source) {
        try {
            final Map<String, Object> sourceMap = XContentType.JSON.xContent()
                    .createParser(NamedXContentRegistry.EMPTY, DeprecationHandler.THROW_UNSUPPORTED_OPERATION, source.streamInput()).map();
            final Map<String, Object> accessToken = (Map<String, Object>) sourceMap.get("access_token");
            final Map<String, Object> userToken = (Map<String, Object>) accessToken.get("user_token");
            final SearchHit hit = new SearchHit(idx, "token_" + userToken.get("id"), null, null);
            hit.sourceRef(source);
            return hit;
        } catch (IOException e) {
            throw ExceptionsHelper.convertToRuntime(e);
        }
    }

    @After
    public void cleanup() {
        samlRealm.close();
    }

    public void testInvalidateCorrectTokensFromLogoutRequest() throws Exception {
        storeToken(logoutRequest.getNameId(), randomAlphaOfLength(10));
        final Tuple<UserToken, String> tokenToInvalidate1 = storeToken(logoutRequest.getNameId(), logoutRequest.getSession());
        final Tuple<UserToken, String> tokenToInvalidate2 = storeToken(logoutRequest.getNameId(), logoutRequest.getSession());
        storeToken(new SamlNameId(NameID.PERSISTENT, randomAlphaOfLength(16), null, null, null), logoutRequest.getSession());

        assertThat(indexRequests.size(), equalTo(4));

        final AtomicInteger counter = new AtomicInteger();
        final SearchHit[] searchHits = indexRequests.stream()
                .filter(r -> r.id().startsWith("token"))
                .map(r -> tokenHit(counter.incrementAndGet(), r.source()))
                .collect(Collectors.toList())
                .toArray(new SearchHit[0]);
        assertThat(searchHits.length, equalTo(4));
        searchFunction = req1 -> {
            searchFunction = findTokenByRefreshToken(searchHits);
            return searchHits;
        };

        indexRequests.clear();

        final SamlInvalidateSessionRequest request = new SamlInvalidateSessionRequest();
        request.setRealmName(samlRealm.name());
        request.setQueryString("SAMLRequest=foo");
        final PlainActionFuture<SamlInvalidateSessionResponse> future = new PlainActionFuture<>();
        action.doExecute(mock(Task.class), request, future);
        final SamlInvalidateSessionResponse response = future.get();
        assertThat(response, notNullValue());
        assertThat(response.getCount(), equalTo(2));
        assertThat(response.getRealmName(), equalTo(samlRealm.name()));
        assertThat(response.getRedirectUrl(), notNullValue());
        assertThat(response.getRedirectUrl(), startsWith(SamlRealmTestHelper.IDP_LOGOUT_URL));
        assertThat(response.getRedirectUrl(), containsString("SAMLResponse="));

        // 1 to find the tokens for the realm
        // 2 more to find the UserTokens from the 2 matching refresh tokens
        assertThat(searchRequests.size(), equalTo(3));

        assertThat(searchRequests.get(0).source().query(), instanceOf(BoolQueryBuilder.class));
        final List<QueryBuilder> filter0 = ((BoolQueryBuilder) searchRequests.get(0).source().query()).filter();
        assertThat(filter0, iterableWithSize(3));

        assertThat(filter0.get(0), instanceOf(TermQueryBuilder.class));
        assertThat(((TermQueryBuilder) filter0.get(0)).fieldName(), equalTo("doc_type"));
        assertThat(((TermQueryBuilder) filter0.get(0)).value(), equalTo("token"));

        assertThat(filter0.get(1), instanceOf(TermQueryBuilder.class));
        assertThat(((TermQueryBuilder) filter0.get(1)).fieldName(), equalTo("access_token.realm"));
        assertThat(((TermQueryBuilder) filter0.get(1)).value(), equalTo(samlRealm.name()));

        assertThat(filter0.get(2), instanceOf(BoolQueryBuilder.class));
        assertThat(((BoolQueryBuilder) filter0.get(2)).should(), iterableWithSize(2));

        assertThat(searchRequests.get(1).source().query(), instanceOf(BoolQueryBuilder.class));
        final List<QueryBuilder> filter1 = ((BoolQueryBuilder) searchRequests.get(1).source().query()).filter();
        assertThat(filter1, iterableWithSize(2));

        assertThat(filter1.get(0), instanceOf(TermQueryBuilder.class));
        assertThat(((TermQueryBuilder) filter1.get(0)).fieldName(), equalTo("doc_type"));
        assertThat(((TermQueryBuilder) filter1.get(0)).value(), equalTo("token"));

        assertThat(filter1.get(1), instanceOf(TermQueryBuilder.class));
        assertThat(((TermQueryBuilder) filter1.get(1)).fieldName(), equalTo("refresh_token.token"));
        assertThat(((TermQueryBuilder) filter1.get(1)).value(), equalTo(tokenToInvalidate1.v2()));

        assertThat(updateRequests.size(), equalTo(4)); // (refresh-token + access-token) * 2
        assertThat(updateRequests.get(0).id(), equalTo("token_" + tokenToInvalidate1.v1().getId()));
        assertThat(updateRequests.get(1).id(), equalTo(updateRequests.get(0).id()));
        assertThat(updateRequests.get(2).id(), equalTo("token_" + tokenToInvalidate2.v1().getId()));
        assertThat(updateRequests.get(3).id(), equalTo(updateRequests.get(2).id()));

        assertThat(indexRequests.size(), equalTo(2)); // bwc-invalidate * 2
        assertThat(indexRequests.get(0).id(), startsWith("invalidated-token_"));
        assertThat(indexRequests.get(1).id(), startsWith("invalidated-token_"));
    }

    private Function<SearchRequest, SearchHit[]> findTokenByRefreshToken(SearchHit[] searchHits) {
        return request -> {
            assertThat(request.source().query(), instanceOf(BoolQueryBuilder.class));
            final List<QueryBuilder> filters = ((BoolQueryBuilder) request.source().query()).filter();
            assertThat(filters, iterableWithSize(2));
            assertThat(filters.get(1), instanceOf(TermQueryBuilder.class));
            final TermQueryBuilder termQuery = (TermQueryBuilder) filters.get(1);
            assertThat(termQuery.fieldName(), equalTo("refresh_token.token"));
            for (SearchHit hit : searchHits) {
                final Map<String, Object> refreshToken = (Map<String, Object>) hit.getSourceAsMap().get("refresh_token");
                if (termQuery.value().equals(refreshToken.get("token"))) {
                    return new SearchHit[]{hit};
                }
            }
            return new SearchHit[0];
        };
    }

    private Tuple<UserToken, String> storeToken(SamlNameId nameId, String session) throws IOException {
        Authentication authentication = new Authentication(new User("bob"),
                new RealmRef("native", NativeRealmSettings.TYPE, "node01"), null);
        final Map<String, Object> metadata = samlRealm.createTokenMetadata(nameId, session);
        final PlainActionFuture<Tuple<UserToken, String>> future = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, future, metadata);
        return future.actionGet();
    }

}
