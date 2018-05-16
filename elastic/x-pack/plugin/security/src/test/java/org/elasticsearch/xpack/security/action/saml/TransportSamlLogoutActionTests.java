/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.action.saml;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.action.get.GetAction;
import org.elasticsearch.action.get.GetRequestBuilder;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.get.MultiGetAction;
import org.elasticsearch.action.get.MultiGetItemResponse;
import org.elasticsearch.action.get.MultiGetRequest;
import org.elasticsearch.action.get.MultiGetRequestBuilder;
import org.elasticsearch.action.get.MultiGetResponse;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateRequestBuilder;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.MapBuilder;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.io.PathUtils;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.action.saml.SamlLogoutRequest;
import org.elasticsearch.xpack.core.security.action.saml.SamlLogoutResponse;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.RealmConfig;
import org.elasticsearch.xpack.core.security.authc.saml.SamlRealmSettings;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.security.authc.Realms;
import org.elasticsearch.xpack.security.authc.TokenService;
import org.elasticsearch.xpack.security.authc.UserToken;
import org.elasticsearch.xpack.security.authc.saml.SamlNameId;
import org.elasticsearch.xpack.security.authc.saml.SamlRealm;
import org.elasticsearch.xpack.security.authc.saml.SamlRealmTests;
import org.elasticsearch.xpack.security.authc.saml.SamlTestCase;
import org.elasticsearch.xpack.security.authc.support.UserRoleMapper;
import org.elasticsearch.xpack.security.support.SecurityIndexManager;
import org.junit.After;
import org.junit.Before;
import org.opensaml.saml.saml2.core.NameID;

import java.nio.file.Path;
import java.time.Clock;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

import static org.elasticsearch.xpack.security.authc.TokenServiceTests.mockGetTokenFromId;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.startsWith;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyString;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TransportSamlLogoutActionTests extends SamlTestCase {

    private static final String SP_URL = "https://sp.example.net/saml";

    private SamlRealm samlRealm;
    private TokenService tokenService;
    private List<IndexRequest> indexRequests;
    private List<UpdateRequest> updateRequests;
    private TransportSamlLogoutAction action;
    private Client client;

    @Before
    public void setup() throws Exception {
        final Settings settings = Settings.builder()
                .put(XPackSettings.TOKEN_SERVICE_ENABLED_SETTING.getKey(), true)
                .put("path.home", createTempDir())
                .build();

        final ThreadContext threadContext = new ThreadContext(settings);
        final ThreadPool threadPool = mock(ThreadPool.class);
        when(threadPool.getThreadContext()).thenReturn(threadContext);
        new Authentication(new User("kibana"), new Authentication.RealmRef("realm", "type", "node"), null).writeToContext(threadContext);

        indexRequests = new ArrayList<>();
        updateRequests = new ArrayList<>();
        client = mock(Client.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(client.settings()).thenReturn(settings);
        doAnswer(invocationOnMock -> {
            GetRequestBuilder builder = new GetRequestBuilder(client, GetAction.INSTANCE);
            builder.setIndex((String) invocationOnMock.getArguments()[0])
                    .setType((String) invocationOnMock.getArguments()[1])
                    .setId((String) invocationOnMock.getArguments()[2]);
            return builder;
        }).when(client).prepareGet(anyString(), anyString(), anyString());
        doAnswer(invocationOnMock -> {
            IndexRequestBuilder builder = new IndexRequestBuilder(client, IndexAction.INSTANCE);
            builder.setIndex((String) invocationOnMock.getArguments()[0])
                    .setType((String) invocationOnMock.getArguments()[1])
                    .setId((String) invocationOnMock.getArguments()[2]);
            return builder;
        }).when(client).prepareIndex(anyString(), anyString(), anyString());
        doAnswer(invocationOnMock -> {
            UpdateRequestBuilder builder = new UpdateRequestBuilder(client, UpdateAction.INSTANCE);
            builder.setIndex((String) invocationOnMock.getArguments()[0])
                    .setType((String) invocationOnMock.getArguments()[1])
                    .setId((String) invocationOnMock.getArguments()[2]);
            return builder;
        }).when(client).prepareUpdate(anyString(), anyString(), anyString());
        when(client.prepareMultiGet()).thenReturn(new MultiGetRequestBuilder(client, MultiGetAction.INSTANCE));
        doAnswer(invocationOnMock -> {
            ActionListener<MultiGetResponse> listener = (ActionListener<MultiGetResponse>) invocationOnMock.getArguments()[1];
            MultiGetResponse response = mock(MultiGetResponse.class);
            MultiGetItemResponse[] responses = new MultiGetItemResponse[2];
            when(response.getResponses()).thenReturn(responses);

            GetResponse oldGetResponse = mock(GetResponse.class);
            when(oldGetResponse.isExists()).thenReturn(false);
            responses[0] = new MultiGetItemResponse(oldGetResponse, null);

            GetResponse getResponse = mock(GetResponse.class);
            responses[1] = new MultiGetItemResponse(getResponse, null);
            when(getResponse.isExists()).thenReturn(false);
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).multiGet(any(MultiGetRequest.class), any(ActionListener.class));
        doAnswer(invocationOnMock -> {
            UpdateRequest updateRequest = (UpdateRequest) invocationOnMock.getArguments()[0];
            ActionListener<UpdateResponse> listener = (ActionListener<UpdateResponse>) invocationOnMock.getArguments()[1];
            updateRequests.add(updateRequest);
            final UpdateResponse response = new UpdateResponse(
                    updateRequest.getShardId(), updateRequest.type(), updateRequest.id(), 1, DocWriteResponse.Result.UPDATED);
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).update(any(UpdateRequest.class), any(ActionListener.class));
        doAnswer(invocationOnMock -> {
            IndexRequest indexRequest = (IndexRequest) invocationOnMock.getArguments()[0];
            ActionListener<IndexResponse> listener = (ActionListener<IndexResponse>) invocationOnMock.getArguments()[1];
            indexRequests.add(indexRequest);
            final IndexResponse response = new IndexResponse(
                    indexRequest.shardId(), indexRequest.type(), indexRequest.id(), 1, 1, 1, true);
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).index(any(IndexRequest.class), any(ActionListener.class));
        doAnswer(invocationOnMock -> {
            IndexRequest indexRequest = (IndexRequest) invocationOnMock.getArguments()[1];
            ActionListener<IndexResponse> listener = (ActionListener<IndexResponse>) invocationOnMock.getArguments()[2];
            indexRequests.add(indexRequest);
            final IndexResponse response = new IndexResponse(
                    indexRequest.shardId(), indexRequest.type(), indexRequest.id(), 1, 1, 1, true);
            listener.onResponse(response);
            return Void.TYPE;
        }).when(client).execute(eq(IndexAction.INSTANCE), any(IndexRequest.class), any(ActionListener.class));

        final SecurityIndexManager securityIndex = mock(SecurityIndexManager.class);
        doAnswer(inv -> {
            ((Runnable) inv.getArguments()[1]).run();
            return null;
        }).when(securityIndex).prepareIndexIfNeededThenExecute(any(Consumer.class), any(Runnable.class));

        final ClusterService clusterService = ClusterServiceUtils.createClusterService(threadPool);
        tokenService = new TokenService(settings, Clock.systemUTC(), client, securityIndex, clusterService);

        final TransportService transportService = new TransportService(Settings.EMPTY, null, null,
                TransportService.NOOP_TRANSPORT_INTERCEPTOR, x -> null, null, Collections.emptySet());
        final Realms realms = mock(Realms.class);
        action = new TransportSamlLogoutAction(settings, threadPool, transportService,
                mock(ActionFilters.class), mock(IndexNameExpressionResolver.class), realms, tokenService);

        final Path metadata = PathUtils.get(SamlRealm.class.getResource("idp1.xml").toURI());
        final Environment env = TestEnvironment.newEnvironment(settings);
        final Settings realmSettings = Settings.builder()
                .put(SamlRealmSettings.IDP_METADATA_PATH.getKey(), metadata.toString())
                .put(SamlRealmSettings.IDP_ENTITY_ID.getKey(), SamlRealmTests.TEST_IDP_ENTITY_ID)
                .put(SamlRealmSettings.SP_ENTITY_ID.getKey(), SP_URL)
                .put(SamlRealmSettings.SP_ACS.getKey(), SP_URL)
                .put("attributes.principal", "uid")
                .build();

        final RealmConfig realmConfig = new RealmConfig("saml1", realmSettings, settings, env, threadContext);
        samlRealm = SamlRealm.create(realmConfig, mock(SSLService.class), mock(ResourceWatcherService.class), mock(UserRoleMapper.class));
        when(realms.realm(realmConfig.name())).thenReturn(samlRealm);
    }

    @After
    public void cleanup() {
        samlRealm.close();
    }

    public void testLogoutInvalidatesToken() throws Exception {
        final String session = randomAlphaOfLengthBetween(12, 18);
        final String nameId = randomAlphaOfLengthBetween(6, 16);
        final Map<String, Object> userMetaData = MapBuilder.<String, Object>newMapBuilder()
                .put(SamlRealm.USER_METADATA_NAMEID_FORMAT, NameID.TRANSIENT)
                .put(SamlRealm.USER_METADATA_NAMEID_VALUE, nameId)
                .map();
        final User user = new User("punisher", new String[] { "superuser" }, null, null, userMetaData, true);
        final Authentication.RealmRef realmRef = new Authentication.RealmRef(samlRealm.name(), SamlRealmSettings.TYPE, "node01");
        final Authentication authentication = new Authentication(user, realmRef, null);

        final Map<String, Object> tokenMetaData = samlRealm.createTokenMetadata(
                new SamlNameId(NameID.TRANSIENT, nameId, null, null, null), session);

        final PlainActionFuture<Tuple<UserToken, String>> future = new PlainActionFuture<>();
        tokenService.createUserToken(authentication, authentication, future, tokenMetaData);
        final UserToken userToken = future.actionGet().v1();
        mockGetTokenFromId(userToken, client);
        final String tokenString = tokenService.getUserTokenString(userToken);

        final SamlLogoutRequest request = new SamlLogoutRequest();
        request.setToken(tokenString);
        final PlainActionFuture<SamlLogoutResponse> listener = new PlainActionFuture<>();
        action.doExecute(request, listener);
        final SamlLogoutResponse response = listener.get();
        assertThat(response, notNullValue());
        assertThat(response.getRedirectUrl(), notNullValue());

        final IndexRequest indexRequest1 = indexRequests.get(0);
        assertThat(indexRequest1, notNullValue());
        assertThat(indexRequest1.id(), startsWith("token"));

        final IndexRequest indexRequest2 = indexRequests.get(1);
        assertThat(indexRequest2, notNullValue());
        assertThat(indexRequest2.id(), startsWith("invalidated-token"));
    }

}
