/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.profile;

import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.bulk.BulkAction;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.get.GetAction;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.get.MultiGetAction;
import org.elasticsearch.action.get.MultiGetItemResponse;
import org.elasticsearch.action.get.MultiGetRequest;
import org.elasticsearch.action.get.MultiGetResponse;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.hash.MessageDigests;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.index.get.GetResult;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.MultiMatchQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.ScoreSortBuilder;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.MockLogAppender;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.threadpool.FixedExecutorBuilder;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.security.action.profile.Profile;
import org.elasticsearch.xpack.core.security.action.profile.SuggestProfilesRequest;
import org.elasticsearch.xpack.core.security.action.profile.SuggestProfilesRequestTests;
import org.elasticsearch.xpack.core.security.action.profile.SuggestProfilesResponse;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.AuthenticationTestHelper;
import org.elasticsearch.xpack.core.security.authc.AuthenticationTests;
import org.elasticsearch.xpack.core.security.authc.DomainConfig;
import org.elasticsearch.xpack.core.security.authc.RealmConfig;
import org.elasticsearch.xpack.core.security.authc.RealmDomain;
import org.elasticsearch.xpack.core.security.authc.Subject;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.security.support.SecurityIndexManager;
import org.elasticsearch.xpack.security.test.SecurityMocks;
import org.hamcrest.Matchers;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.time.Clock;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import java.util.concurrent.ExecutionException;
import java.util.function.Consumer;
import java.util.stream.Collectors;

import static java.util.Collections.emptyMap;
import static org.elasticsearch.common.util.concurrent.ThreadContext.ACTION_ORIGIN_TRANSIENT_NAME;
import static org.elasticsearch.test.ActionListenerUtils.anyActionListener;
import static org.elasticsearch.xpack.core.ClientHelper.SECURITY_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.SECURITY_PROFILE_ORIGIN;
import static org.elasticsearch.xpack.security.Security.SECURITY_CRYPTO_THREAD_POOL_NAME;
import static org.elasticsearch.xpack.security.support.SecuritySystemIndices.SECURITY_PROFILE_ALIAS;
import static org.hamcrest.Matchers.arrayWithSize;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.spy;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class ProfileServiceTests extends ESTestCase {

    private static final String SAMPLE_PROFILE_DOCUMENT_TEMPLATE = """
        {
          "user_profile":  {
            "uid": "%s",
            "enabled": true,
            "user": {
              "username": "%s",
              "roles": %s,
              "realm": {
                "name": "realm_name_1",
                "type": "realm_type_1",
                "domain": {
                  "name": "domainA",
                  "realms": [
                    { "name": "realm_name_1", "type": "realm_type_1" },
                    { "name": "realm_name_2", "type": "realm_type_2" }
                  ]
                },
                "node_name": "node1"
              },
              "email": "foo@example.com",
              "full_name": "User Foo"
            },
            "last_synchronized": %s,
            "labels": {
            },
            "application_data": {
              "app1": { "name": "app1" },
              "app2": { "name": "app2" }
            }
          }
        }
        """;
    private ThreadPool threadPool;
    private Client client;
    private SecurityIndexManager profileIndex;
    private ProfileService profileService;
    private Version minNodeVersion;

    @Before
    public void prepare() {
        threadPool = spy(
            new TestThreadPool(
                "api key service tests",
                new FixedExecutorBuilder(
                    Settings.EMPTY,
                    SECURITY_CRYPTO_THREAD_POOL_NAME,
                    1,
                    1000,
                    "xpack.security.crypto.thread_pool",
                    false
                )
            )
        );
        this.client = mock(Client.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(client.prepareSearch(SECURITY_PROFILE_ALIAS)).thenReturn(
            new SearchRequestBuilder(client, SearchAction.INSTANCE).setIndices(SECURITY_PROFILE_ALIAS)
        );
        this.profileIndex = SecurityMocks.mockSecurityIndexManager(SECURITY_PROFILE_ALIAS);
        final ClusterService clusterService = mock(ClusterService.class);
        final ClusterState clusterState = mock(ClusterState.class);
        when(clusterService.state()).thenReturn(clusterState);
        final DiscoveryNodes discoveryNodes = mock(DiscoveryNodes.class);
        when(clusterState.nodes()).thenReturn(discoveryNodes);
        minNodeVersion = VersionUtils.randomVersionBetween(random(), Version.V_7_17_0, Version.CURRENT);
        when(discoveryNodes.getMinNodeVersion()).thenReturn(minNodeVersion);
        this.profileService = new ProfileService(
            Settings.EMPTY,
            Clock.systemUTC(),
            client,
            profileIndex,
            clusterService,
            name -> new DomainConfig(name, Set.of(), false, null),
            threadPool
        );
    }

    @After
    public void stopThreadPool() {
        terminate(threadPool);
    }

    public void testGetProfileByUid() {
        final String uid = randomAlphaOfLength(20);
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final GetRequest getRequest = (GetRequest) invocation.getArguments()[1];
            @SuppressWarnings("unchecked")
            final ActionListener<GetResponse> listener = (ActionListener<GetResponse>) invocation.getArguments()[2];
            client.get(getRequest, listener);
            return null;
        }).when(client).execute(eq(GetAction.INSTANCE), any(GetRequest.class), anyActionListener());

        final long lastSynchronized = Instant.now().toEpochMilli();
        mockGetRequest(uid, lastSynchronized);

        final PlainActionFuture<Profile> future = new PlainActionFuture<>();

        final Set<String> dataKeys = randomFrom(Set.of("app1"), Set.of("app2"), Set.of("app1", "app2"), Set.of());

        profileService.getProfile(uid, dataKeys, future);
        final Profile profile = future.actionGet();

        final Map<String, Object> applicationData = new HashMap<>();

        if (dataKeys != null && dataKeys.contains("app1")) {
            applicationData.put("app1", Map.of("name", "app1"));
        }

        if (dataKeys != null && dataKeys.contains("app2")) {
            applicationData.put("app2", Map.of("name", "app2"));
        }

        assertThat(
            profile,
            equalTo(
                new Profile(
                    uid,
                    true,
                    lastSynchronized,
                    new Profile.ProfileUser("Foo", List.of("role1", "role2"), "realm_name_1", "domainA", "foo@example.com", "User Foo"),
                    Map.of(),
                    applicationData,
                    new Profile.VersionControl(1, 0)
                )
            )
        );
    }

    @SuppressWarnings("unchecked")
    public void testGetProfileSubjectsNoIndex() throws Exception {
        when(profileIndex.indexExists()).thenReturn(false);
        PlainActionFuture<ProfileService.MultiProfileSubjectResponse> future = new PlainActionFuture<>();
        profileService.getProfileSubjects(randomList(1, 5, () -> randomAlphaOfLength(20)), future);
        ProfileService.MultiProfileSubjectResponse multiProfileSubjectResponse = future.get();
        assertThat(multiProfileSubjectResponse.profileUidToSubject().size(), is(0));
        assertThat(multiProfileSubjectResponse.failureProfileUids().size(), is(0));
        when(profileIndex.indexExists()).thenReturn(true);
        ElasticsearchException unavailableException = new ElasticsearchException("mock profile index unavailable");
        when(profileIndex.isAvailable()).thenReturn(false);
        when(profileIndex.getUnavailableReason()).thenReturn(unavailableException);
        PlainActionFuture<ProfileService.MultiProfileSubjectResponse> future2 = new PlainActionFuture<>();
        profileService.getProfileSubjects(randomList(1, 5, () -> randomAlphaOfLength(20)), future2);
        ExecutionException e = expectThrows(ExecutionException.class, () -> future2.get());
        assertThat(e.getCause(), is(unavailableException));
        PlainActionFuture<ProfileService.MultiProfileSubjectResponse> future3 = new PlainActionFuture<>();
        profileService.getProfileSubjects(List.of(), future3);
        multiProfileSubjectResponse = future3.get();
        assertThat(multiProfileSubjectResponse.profileUidToSubject().size(), is(0));
        assertThat(multiProfileSubjectResponse.failureProfileUids().size(), is(0));
        verify(profileIndex, never()).checkIndexVersionThenExecute(any(Consumer.class), any(Runnable.class));
    }

    @SuppressWarnings("unchecked")
    public void testGetProfileSubjectsWithMissingNoFailures() throws Exception {
        final Collection<String> allProfileUids = randomList(1, 5, () -> randomAlphaOfLength(20));
        final Collection<String> missingProfileUids = randomSubsetOf(allProfileUids);
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final MultiGetRequest multiGetRequest = (MultiGetRequest) invocation.getArguments()[1];
            List<MultiGetItemResponse> responses = new ArrayList<>();
            for (MultiGetRequest.Item item : multiGetRequest.getItems()) {
                assertThat(item.index(), is(SECURITY_PROFILE_ALIAS));
                assertThat(item.id(), Matchers.startsWith("profile_"));
                assertThat(allProfileUids, hasItem(item.id().substring("profile_".length())));
                if (missingProfileUids.contains(item.id().substring("profile_".length()))) {
                    GetResponse missingResponse = mock(GetResponse.class);
                    when(missingResponse.isExists()).thenReturn(false);
                    when(missingResponse.getId()).thenReturn(item.id());
                    responses.add(new MultiGetItemResponse(missingResponse, null));
                } else {
                    String source = getSampleProfileDocumentSource(
                        item.id().substring("profile_".length()),
                        "foo_username_" + item.id().substring("profile_".length()),
                        List.of("foo_role_" + item.id().substring("profile_".length())),
                        Instant.now().toEpochMilli()
                    );
                    GetResult existingResult = new GetResult(
                        SECURITY_PROFILE_ALIAS,
                        item.id(),
                        0,
                        1,
                        1,
                        true,
                        new BytesArray(source),
                        emptyMap(),
                        emptyMap()
                    );
                    responses.add(new MultiGetItemResponse(new GetResponse(existingResult), null));
                }
            }
            final ActionListener<MultiGetResponse> listener = (ActionListener<MultiGetResponse>) invocation.getArguments()[2];
            listener.onResponse(new MultiGetResponse(responses.toArray(MultiGetItemResponse[]::new)));
            return null;
        }).when(client).execute(eq(MultiGetAction.INSTANCE), any(MultiGetRequest.class), anyActionListener());

        final PlainActionFuture<ProfileService.MultiProfileSubjectResponse> future = new PlainActionFuture<>();
        profileService.getProfileSubjects(allProfileUids, future);

        ProfileService.MultiProfileSubjectResponse multiProfileSubjectResponse = future.get();
        verify(profileIndex).checkIndexVersionThenExecute(any(Consumer.class), any(Runnable.class));
        assertThat(multiProfileSubjectResponse.failureProfileUids().isEmpty(), is(true));
        assertThat(multiProfileSubjectResponse.profileUidToSubject().size(), is(allProfileUids.size() - missingProfileUids.size()));
        for (Map.Entry<String, Subject> profileIdAndSubject : multiProfileSubjectResponse.profileUidToSubject().entrySet()) {
            assertThat(allProfileUids, hasItem(profileIdAndSubject.getKey()));
            assertThat(missingProfileUids, not(hasItem(profileIdAndSubject.getKey())));
            assertThat(profileIdAndSubject.getValue().getUser().principal(), is("foo_username_" + profileIdAndSubject.getKey()));
            assertThat(profileIdAndSubject.getValue().getUser().roles(), arrayWithSize(1));
            assertThat(profileIdAndSubject.getValue().getUser().roles()[0], is("foo_role_" + profileIdAndSubject.getKey()));
        }
    }

    @SuppressWarnings("unchecked")
    public void testGetProfileSubjectWithFailures() throws Exception {
        final ElasticsearchException mGetException = new ElasticsearchException("mget Exception");
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final ActionListener<MultiGetResponse> listener = (ActionListener<MultiGetResponse>) invocation.getArguments()[2];
            listener.onFailure(mGetException);
            return null;
        }).when(client).execute(eq(MultiGetAction.INSTANCE), any(MultiGetRequest.class), anyActionListener());
        final PlainActionFuture<ProfileService.MultiProfileSubjectResponse> future = new PlainActionFuture<>();
        profileService.getProfileSubjects(randomList(1, 5, () -> randomAlphaOfLength(20)), future);
        ExecutionException e = expectThrows(ExecutionException.class, () -> future.get());
        assertThat(e.getCause(), is(mGetException));
        final Collection<String> missingProfileUids = randomList(1, 5, () -> randomAlphaOfLength(20));
        final Collection<String> errorProfileUids = randomSubsetOf(missingProfileUids);
        final MockLogAppender mockLogAppender = new MockLogAppender();
        if (false == errorProfileUids.isEmpty()) {
            mockLogAppender.addExpectation(
                new MockLogAppender.SeenEventExpectation(
                    "message",
                    "org.elasticsearch.xpack.security.profile.ProfileService",
                    Level.DEBUG,
                    "Failed to retrieve profiles "
                        + missingProfileUids.stream()
                            .filter(v -> errorProfileUids.contains(v))
                            .collect(Collectors.toCollection(TreeSet::new))
                )
            );
        }
        mockLogAppender.start();
        final Logger logger = LogManager.getLogger(ProfileService.class);
        Loggers.setLevel(logger, Level.DEBUG);
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final MultiGetRequest multiGetRequest = (MultiGetRequest) invocation.getArguments()[1];
            List<MultiGetItemResponse> responses = new ArrayList<>();
            for (MultiGetRequest.Item item : multiGetRequest.getItems()) {
                assertThat(item.index(), is(SECURITY_PROFILE_ALIAS));
                assertThat(item.id(), Matchers.startsWith("profile_"));
                if (false == errorProfileUids.contains(item.id().substring("profile_".length()))) {
                    GetResponse missingResponse = mock(GetResponse.class);
                    when(missingResponse.isExists()).thenReturn(false);
                    when(missingResponse.getId()).thenReturn(item.id());
                    responses.add(new MultiGetItemResponse(missingResponse, null));
                } else {
                    MultiGetResponse.Failure failure = mock(MultiGetResponse.Failure.class);
                    when(failure.getId()).thenReturn(item.id());
                    when(failure.getFailure()).thenReturn(new ElasticsearchException("failed mget item"));
                    responses.add(new MultiGetItemResponse(null, failure));
                }
            }
            final ActionListener<MultiGetResponse> listener = (ActionListener<MultiGetResponse>) invocation.getArguments()[2];
            listener.onResponse(new MultiGetResponse(responses.toArray(MultiGetItemResponse[]::new)));
            return null;
        }).when(client).execute(eq(MultiGetAction.INSTANCE), any(MultiGetRequest.class), anyActionListener());

        try {
            Loggers.addAppender(logger, mockLogAppender);
            final PlainActionFuture<ProfileService.MultiProfileSubjectResponse> future2 = new PlainActionFuture<>();
            profileService.getProfileSubjects(missingProfileUids, future2);

            ProfileService.MultiProfileSubjectResponse multiProfileSubjectResponse = future2.get();
            assertThat(multiProfileSubjectResponse.profileUidToSubject().isEmpty(), is(true));
            assertThat(multiProfileSubjectResponse.failureProfileUids(), containsInAnyOrder(errorProfileUids.toArray(String[]::new)));
            mockLogAppender.assertAllExpectationsMatched();
        } finally {
            Loggers.removeAppender(logger, mockLogAppender);
            mockLogAppender.stop();
        }
    }

    public void testActivateProfileShouldFailIfSubjectTypeIsNotUser() {
        final Authentication authentication;
        if (randomBoolean()) {
            final User user = new User(randomAlphaOfLengthBetween(5, 8));
            authentication = AuthenticationTests.randomApiKeyAuthentication(user, randomAlphaOfLength(20));
        } else {
            authentication = AuthenticationTests.randomServiceAccountAuthentication();
        }

        final PlainActionFuture<Profile> future1 = new PlainActionFuture<>();
        profileService.activateProfile(authentication, future1);
        final IllegalArgumentException e1 = expectThrows(IllegalArgumentException.class, future1::actionGet);
        assertThat(e1.getMessage(), containsString("profile is supported for user only"));
    }

    public void testActivateProfileShouldFailForInternalUser() {
        final Authentication authentication = AuthenticationTestHelper.builder().internal().build();

        final PlainActionFuture<Profile> future1 = new PlainActionFuture<>();
        profileService.activateProfile(authentication, future1);
        final IllegalStateException e1 = expectThrows(IllegalStateException.class, future1::actionGet);
        assertThat(e1.getMessage(), containsString("profile should not be created for internal user"));
    }

    public void testFailureForParsingDifferentiator() throws IOException {
        final Subject subject = AuthenticationTestHelper.builder().realm().build(false).getEffectiveSubject();
        final PlainActionFuture<Profile> future1 = new PlainActionFuture<>();
        profileService.maybeIncrementDifferentiatorAndCreateNewProfile(subject, randomProfileDocument(randomAlphaOfLength(20)), future1);
        final ElasticsearchException e1 = expectThrows(ElasticsearchException.class, future1::actionGet);
        assertThat(e1.getMessage(), containsString("does not contain any underscore character"));

        final PlainActionFuture<Profile> future2 = new PlainActionFuture<>();
        profileService.maybeIncrementDifferentiatorAndCreateNewProfile(
            subject,
            randomProfileDocument(randomAlphaOfLength(20) + "_"),
            future2
        );
        final ElasticsearchException e2 = expectThrows(ElasticsearchException.class, future2::actionGet);
        assertThat(e2.getMessage(), containsString("does not contain a differentiator"));

        final PlainActionFuture<Profile> future3 = new PlainActionFuture<>();
        profileService.maybeIncrementDifferentiatorAndCreateNewProfile(
            subject,
            randomProfileDocument(randomAlphaOfLength(20) + "_" + randomAlphaOfLengthBetween(1, 3)),
            future3
        );
        final ElasticsearchException e3 = expectThrows(ElasticsearchException.class, future3::actionGet);
        assertThat(e3.getMessage(), containsString("differentiator is not a number"));
    }

    public void testLiteralUsernameWillThrowOnDuplicate() throws IOException {
        final Subject subject = new Subject(AuthenticationTestHelper.randomUser(), AuthenticationTestHelper.randomRealmRef(true));
        final ProfileService service = new ProfileService(
            Settings.EMPTY,
            Clock.systemUTC(),
            client,
            profileIndex,
            mock(ClusterService.class),
            domainName -> new DomainConfig(domainName, Set.of(), true, "suffix"),
            threadPool
        );
        final PlainActionFuture<Profile> future = new PlainActionFuture<>();
        service.maybeIncrementDifferentiatorAndCreateNewProfile(
            subject,
            ProfileDocument.fromSubjectWithUid(subject, "u_" + subject.getUser().principal() + "_suffix"),
            future
        );

        final ElasticsearchException e = expectThrows(ElasticsearchException.class, future::actionGet);
        assertThat(e.getMessage(), containsString("cannot create new profile for [" + subject.getUser().principal() + "]"));
        assertThat(
            e.getMessage(),
            containsString("suffix setting of domain [" + subject.getRealm().getDomain().name() + "] does not support auto-increment")
        );
    }

    public void testBuildSearchRequest() {
        final String name = randomAlphaOfLengthBetween(0, 8);
        final int size = randomIntBetween(0, Integer.MAX_VALUE);
        final SuggestProfilesRequest.Hint hint = SuggestProfilesRequestTests.randomHint();
        final SuggestProfilesRequest suggestProfilesRequest = new SuggestProfilesRequest(Set.of(), name, size, hint);
        final TaskId parentTaskId = new TaskId(randomAlphaOfLength(20), randomNonNegativeLong());

        final SearchRequest searchRequest = profileService.buildSearchRequest(suggestProfilesRequest, parentTaskId);
        assertThat(searchRequest.getParentTask(), is(parentTaskId));

        final SearchSourceBuilder searchSourceBuilder = searchRequest.source();

        assertThat(
            searchSourceBuilder.sorts(),
            equalTo(List.of(new ScoreSortBuilder(), new FieldSortBuilder("user_profile.last_synchronized").order(SortOrder.DESC)))
        );
        assertThat(searchSourceBuilder.size(), equalTo(size));

        assertThat(searchSourceBuilder.query(), instanceOf(BoolQueryBuilder.class));

        final BoolQueryBuilder query = (BoolQueryBuilder) searchSourceBuilder.query();
        if (Strings.hasText(name)) {
            assertThat(
                query.must(),
                equalTo(
                    List.of(
                        QueryBuilders.multiMatchQuery(
                            name,
                            "user_profile.user.username",
                            "user_profile.user.username._2gram",
                            "user_profile.user.username._3gram",
                            "user_profile.user.full_name",
                            "user_profile.user.full_name._2gram",
                            "user_profile.user.full_name._3gram",
                            "user_profile.user.email"
                        ).type(MultiMatchQueryBuilder.Type.BOOL_PREFIX).fuzziness(Fuzziness.AUTO)
                    )
                )
            );
        } else {
            assertThat(query.must(), empty());
        }

        if (hint != null) {
            final List<QueryBuilder> shouldQueries = new ArrayList<>(query.should());
            if (hint.getUids() != null) {
                assertThat(shouldQueries.remove(0), equalTo(QueryBuilders.termsQuery("user_profile.uid", hint.getUids())));
            }
            final Tuple<String, List<String>> label = hint.getSingleLabel();
            if (label != null) {
                final List<String> labelValues = label.v2();
                assertThat(shouldQueries.remove(0), equalTo(QueryBuilders.termsQuery("user_profile.labels." + label.v1(), labelValues)));
            }
            assertThat(query.minimumShouldMatch(), equalTo("0"));
        } else {
            assertThat(query.should(), empty());
        }
    }

    // Note this method is to test the origin is switched security_profile for all profile related actions.
    // The actual result of the action is not relevant as long as the action is performed with the correct origin.
    // Therefore, exceptions (used in this test) work as good as full successful responses.
    public void testSecurityProfileOrigin() {
        // Activate profile
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            @SuppressWarnings("unchecked")
            final ActionListener<SearchResponse> listener = (ActionListener<SearchResponse>) invocation.getArguments()[2];
            listener.onResponse(SearchResponse.empty(() -> 1L, SearchResponse.Clusters.EMPTY));
            return null;
        }).when(client).execute(eq(SearchAction.INSTANCE), any(SearchRequest.class), anyActionListener());

        when(client.prepareIndex(SECURITY_PROFILE_ALIAS)).thenReturn(
            new IndexRequestBuilder(client, IndexAction.INSTANCE, SECURITY_PROFILE_ALIAS)
        );

        final RuntimeException expectedException = new RuntimeException("expected");
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final ActionListener<?> listener = (ActionListener<?>) invocation.getArguments()[2];
            listener.onFailure(expectedException);
            return null;
        }).when(client).execute(eq(BulkAction.INSTANCE), any(BulkRequest.class), anyActionListener());

        final PlainActionFuture<Profile> future1 = new PlainActionFuture<>();
        profileService.activateProfile(AuthenticationTestHelper.builder().realm().build(), future1);
        final RuntimeException e1 = expectThrows(RuntimeException.class, future1::actionGet);
        assertThat(e1, is(expectedException));

        // Update
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final ActionListener<?> listener = (ActionListener<?>) invocation.getArguments()[2];
            listener.onFailure(expectedException);
            return null;
        }).when(client).execute(eq(UpdateAction.INSTANCE), any(UpdateRequest.class), anyActionListener());
        final PlainActionFuture<UpdateResponse> future2 = new PlainActionFuture<>();
        profileService.doUpdate(mock(UpdateRequest.class), future2);
        final RuntimeException e2 = expectThrows(RuntimeException.class, future2::actionGet);
        assertThat(e2, is(expectedException));

        // Suggest
        doAnswer(invocation -> {
            assertThat(
                threadPool.getThreadContext().getTransient(ACTION_ORIGIN_TRANSIENT_NAME),
                equalTo(minNodeVersion.onOrAfter(Version.V_8_3_0) ? SECURITY_PROFILE_ORIGIN : SECURITY_ORIGIN)
            );
            final ActionListener<?> listener = (ActionListener<?>) invocation.getArguments()[2];
            listener.onFailure(expectedException);
            return null;
        }).when(client).execute(eq(SearchAction.INSTANCE), any(SearchRequest.class), anyActionListener());
        final PlainActionFuture<SuggestProfilesResponse> future3 = new PlainActionFuture<>();
        profileService.suggestProfile(
            new SuggestProfilesRequest(Set.of(), "", 1, null),
            new TaskId(randomAlphaOfLength(20), randomNonNegativeLong()),
            future3
        );
        final RuntimeException e3 = expectThrows(RuntimeException.class, future3::actionGet);
        assertThat(e3, is(expectedException));
    }

    public void testActivateProfileWithDifferentUidFormats() throws IOException {
        final ProfileService service = spy(
            new ProfileService(Settings.EMPTY, Clock.systemUTC(), client, profileIndex, mock(ClusterService.class), domainName -> {
                if (domainName.startsWith("hash")) {
                    return new DomainConfig(domainName, Set.of(), false, null);
                } else {
                    return new DomainConfig(domainName, Set.of(), true, "suffix");
                }
            }, threadPool)
        );

        doAnswer(invocation -> {
            @SuppressWarnings("unchecked")
            final var listener = (ActionListener<ProfileService.VersionedDocument>) invocation.getArguments()[1];
            listener.onResponse(null);
            return null;
        }).when(service).searchVersionedDocumentForSubject(any(), anyActionListener());

        doAnswer(invocation -> {
            final Object[] arguments = invocation.getArguments();
            final Subject subject = (Subject) arguments[0];
            final User user = subject.getUser();
            final Authentication.RealmRef realmRef = subject.getRealm();
            final String uid = (String) arguments[1];
            @SuppressWarnings("unchecked")
            final var listener = (ActionListener<Profile>) arguments[2];
            listener.onResponse(
                new Profile(
                    uid,
                    true,
                    0,
                    new Profile.ProfileUser(
                        user.principal(),
                        Arrays.asList(user.roles()),
                        realmRef.getName(),
                        realmRef.getDomain() == null ? null : realmRef.getDomain().name(),
                        user.email(),
                        user.fullName()
                    ),
                    Map.of(),
                    Map.of(),
                    new Profile.VersionControl(0, 0)
                )
            );
            return null;
        }).when(service).createNewProfile(any(), any(), anyActionListener());

        // Domainless realm or domain with hashed username
        Authentication.RealmRef realmRef1 = AuthenticationTestHelper.randomRealmRef(false);
        if (randomBoolean()) {
            realmRef1 = new Authentication.RealmRef(
                realmRef1.getName(),
                realmRef1.getType(),
                realmRef1.getNodeName(),
                new RealmDomain("hash", Set.of(new RealmConfig.RealmIdentifier(realmRef1.getType(), realmRef1.getName())))
            );
        }

        final Authentication authentication1 = AuthenticationTestHelper.builder().realm().realmRef(realmRef1).build();
        final Subject subject1 = authentication1.getEffectiveSubject();
        final PlainActionFuture<Profile> future1 = new PlainActionFuture<>();
        service.activateProfile(authentication1, future1);
        final Profile profile1 = future1.actionGet();
        assertThat(
            profile1.uid(),
            equalTo(
                "u_"
                    + Base64.getUrlEncoder()
                        .withoutPadding()
                        .encodeToString(MessageDigests.digest(new BytesArray(subject1.getUser().principal()), MessageDigests.sha256()))
                    + "_0"
            )
        );
        assertThat(profile1.user().username(), equalTo(subject1.getUser().principal()));

        // Domain with literal username
        Authentication.RealmRef realmRef2 = AuthenticationTestHelper.randomRealmRef(false);
        realmRef2 = new Authentication.RealmRef(
            realmRef2.getName(),
            realmRef2.getType(),
            realmRef2.getNodeName(),
            new RealmDomain("literal", Set.of(new RealmConfig.RealmIdentifier(realmRef2.getType(), realmRef2.getName())))
        );

        final Authentication authentication2 = AuthenticationTestHelper.builder().realm().realmRef(realmRef2).build();
        final Subject subject2 = authentication2.getEffectiveSubject();
        final PlainActionFuture<Profile> future2 = new PlainActionFuture<>();
        service.activateProfile(authentication2, future2);
        final Profile profile2 = future2.actionGet();
        assertThat(profile2.uid(), equalTo("u_" + subject2.getUser().principal() + "_suffix"));
        assertThat(profile2.user().username(), equalTo(subject2.getUser().principal()));

        // Domain with literal username, but the username is invalid
        final String invalidUsername = randomFrom("", "fóóbár", randomAlphaOfLength(257));
        final Authentication.RealmRef realmRef3 = realmRef2;
        final Authentication authentication3 = AuthenticationTestHelper.builder()
            .realm()
            .user(new User(invalidUsername))
            .realmRef(realmRef3)
            .build();
        final PlainActionFuture<Profile> future3 = new PlainActionFuture<>();
        service.activateProfile(authentication3, future3);

        final ElasticsearchException e3 = expectThrows(ElasticsearchException.class, future3::actionGet);
        assertThat(
            e3.getMessage(),
            containsString("Security domain [" + realmRef3.getDomain().name() + "] is configured to use literal username.")
        );
        assertThat(e3.getMessage(), containsString("The username can contain alphanumeric characters"));
    }

    private void mockGetRequest(String uid, long lastSynchronized) {
        mockGetRequest(uid, "Foo", List.of("role1", "role2"), lastSynchronized);
    }

    public static String getSampleProfileDocumentSource(String uid, String username, List<String> roles, long lastSynchronized) {
        return SAMPLE_PROFILE_DOCUMENT_TEMPLATE.formatted(
            uid,
            username,
            roles.stream().map(v -> "\"" + v + "\"").collect(Collectors.toList()),
            lastSynchronized
        );
    }

    private void mockGetRequest(String uid, String username, List<String> roles, long lastSynchronized) {
        final String source = getSampleProfileDocumentSource(uid, username, roles, lastSynchronized);
        SecurityMocks.mockGetRequest(client, SECURITY_PROFILE_ALIAS, "profile_" + uid, new BytesArray(source));
    }

    private ProfileDocument randomProfileDocument(String uid) {
        return new ProfileDocument(
            uid,
            true,
            randomLong(),
            new ProfileDocument.ProfileDocumentUser(
                randomAlphaOfLengthBetween(3, 8),
                List.of(),
                AuthenticationTests.randomRealmRef(randomBoolean()),
                "foo@example.com",
                null
            ),
            Map.of(),
            null
        );
    }
}
