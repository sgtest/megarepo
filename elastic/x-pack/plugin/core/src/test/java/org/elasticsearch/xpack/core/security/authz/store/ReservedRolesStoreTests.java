/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.authz.store;

import org.elasticsearch.Version;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthAction;
import org.elasticsearch.action.admin.cluster.repositories.get.GetRepositoriesAction;
import org.elasticsearch.action.admin.cluster.repositories.put.PutRepositoryAction;
import org.elasticsearch.action.admin.cluster.reroute.ClusterRerouteAction;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsAction;
import org.elasticsearch.action.admin.cluster.snapshots.create.CreateSnapshotAction;
import org.elasticsearch.action.admin.cluster.snapshots.get.GetSnapshotsAction;
import org.elasticsearch.action.admin.cluster.snapshots.status.SnapshotsStatusAction;
import org.elasticsearch.action.admin.cluster.state.ClusterStateAction;
import org.elasticsearch.action.admin.cluster.stats.ClusterStatsAction;
import org.elasticsearch.action.admin.indices.create.CreateIndexAction;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexAction;
import org.elasticsearch.action.admin.indices.get.GetIndexAction;
import org.elasticsearch.action.admin.indices.recovery.RecoveryAction;
import org.elasticsearch.action.admin.indices.segments.IndicesSegmentsAction;
import org.elasticsearch.action.admin.indices.settings.get.GetSettingsAction;
import org.elasticsearch.action.admin.indices.settings.put.UpdateSettingsAction;
import org.elasticsearch.action.admin.indices.shards.IndicesShardStoresAction;
import org.elasticsearch.action.admin.indices.stats.IndicesStatsAction;
import org.elasticsearch.action.admin.indices.template.delete.DeleteIndexTemplateAction;
import org.elasticsearch.action.admin.indices.template.get.GetIndexTemplatesAction;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateAction;
import org.elasticsearch.action.admin.indices.upgrade.get.UpgradeStatusAction;
import org.elasticsearch.action.bulk.BulkAction;
import org.elasticsearch.action.delete.DeleteAction;
import org.elasticsearch.action.get.GetAction;
import org.elasticsearch.action.index.IndexAction;
import org.elasticsearch.action.ingest.DeletePipelineAction;
import org.elasticsearch.action.ingest.GetPipelineAction;
import org.elasticsearch.action.ingest.PutPipelineAction;
import org.elasticsearch.action.main.MainAction;
import org.elasticsearch.action.search.MultiSearchAction;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.cluster.metadata.AliasMetaData;
import org.elasticsearch.cluster.metadata.AliasOrIndex;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.xpack.core.action.XPackInfoAction;
import org.elasticsearch.xpack.core.ml.MlMetaIndex;
import org.elasticsearch.xpack.core.ml.action.CloseJobAction;
import org.elasticsearch.xpack.core.ml.action.DeleteCalendarAction;
import org.elasticsearch.xpack.core.ml.action.DeleteCalendarEventAction;
import org.elasticsearch.xpack.core.ml.action.DeleteDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.DeleteExpiredDataAction;
import org.elasticsearch.xpack.core.ml.action.DeleteFilterAction;
import org.elasticsearch.xpack.core.ml.action.DeleteForecastAction;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.elasticsearch.xpack.core.ml.action.DeleteModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.action.FinalizeJobExecutionAction;
import org.elasticsearch.xpack.core.ml.action.FindFileStructureAction;
import org.elasticsearch.xpack.core.ml.action.FlushJobAction;
import org.elasticsearch.xpack.core.ml.action.ForecastJobAction;
import org.elasticsearch.xpack.core.ml.action.GetBucketsAction;
import org.elasticsearch.xpack.core.ml.action.GetCalendarEventsAction;
import org.elasticsearch.xpack.core.ml.action.GetCalendarsAction;
import org.elasticsearch.xpack.core.ml.action.GetCategoriesAction;
import org.elasticsearch.xpack.core.ml.action.GetDatafeedsAction;
import org.elasticsearch.xpack.core.ml.action.GetDatafeedsStatsAction;
import org.elasticsearch.xpack.core.ml.action.GetFiltersAction;
import org.elasticsearch.xpack.core.ml.action.GetInfluencersAction;
import org.elasticsearch.xpack.core.ml.action.GetJobsAction;
import org.elasticsearch.xpack.core.ml.action.GetJobsStatsAction;
import org.elasticsearch.xpack.core.ml.action.GetModelSnapshotsAction;
import org.elasticsearch.xpack.core.ml.action.GetOverallBucketsAction;
import org.elasticsearch.xpack.core.ml.action.GetRecordsAction;
import org.elasticsearch.xpack.core.ml.action.IsolateDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.KillProcessAction;
import org.elasticsearch.xpack.core.ml.action.MlInfoAction;
import org.elasticsearch.xpack.core.ml.action.OpenJobAction;
import org.elasticsearch.xpack.core.ml.action.PersistJobAction;
import org.elasticsearch.xpack.core.ml.action.PostCalendarEventsAction;
import org.elasticsearch.xpack.core.ml.action.PostDataAction;
import org.elasticsearch.xpack.core.ml.action.PreviewDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.PutCalendarAction;
import org.elasticsearch.xpack.core.ml.action.PutDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.PutFilterAction;
import org.elasticsearch.xpack.core.ml.action.PutJobAction;
import org.elasticsearch.xpack.core.ml.action.RevertModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.action.SetUpgradeModeAction;
import org.elasticsearch.xpack.core.ml.action.StartDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.StopDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.UpdateCalendarJobAction;
import org.elasticsearch.xpack.core.ml.action.UpdateDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.UpdateFilterAction;
import org.elasticsearch.xpack.core.ml.action.UpdateJobAction;
import org.elasticsearch.xpack.core.ml.action.UpdateModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.action.UpdateProcessAction;
import org.elasticsearch.xpack.core.ml.action.ValidateDetectorAction;
import org.elasticsearch.xpack.core.ml.action.ValidateJobConfigAction;
import org.elasticsearch.xpack.core.ml.annotations.AnnotationIndex;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndexFields;
import org.elasticsearch.xpack.core.ml.notifications.AuditorField;
import org.elasticsearch.xpack.core.monitoring.action.MonitoringBulkAction;
import org.elasticsearch.xpack.core.security.action.privilege.DeletePrivilegesAction;
import org.elasticsearch.xpack.core.security.action.privilege.DeletePrivilegesRequest;
import org.elasticsearch.xpack.core.security.action.privilege.GetPrivilegesAction;
import org.elasticsearch.xpack.core.security.action.privilege.GetPrivilegesRequest;
import org.elasticsearch.xpack.core.security.action.privilege.PutPrivilegesAction;
import org.elasticsearch.xpack.core.security.action.privilege.PutPrivilegesRequest;
import org.elasticsearch.xpack.core.security.action.role.PutRoleAction;
import org.elasticsearch.xpack.core.security.action.saml.SamlAuthenticateAction;
import org.elasticsearch.xpack.core.security.action.saml.SamlPrepareAuthenticationAction;
import org.elasticsearch.xpack.core.security.action.token.CreateTokenAction;
import org.elasticsearch.xpack.core.security.action.token.InvalidateTokenAction;
import org.elasticsearch.xpack.core.security.action.user.PutUserAction;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor;
import org.elasticsearch.xpack.core.security.authz.accesscontrol.IndicesAccessControl.IndexAccessControl;
import org.elasticsearch.xpack.core.security.authz.permission.FieldPermissionsCache;
import org.elasticsearch.xpack.core.security.authz.permission.Role;
import org.elasticsearch.xpack.core.security.authz.privilege.ApplicationPrivilege;
import org.elasticsearch.xpack.core.security.authz.privilege.ApplicationPrivilegeDescriptor;
import org.elasticsearch.xpack.core.security.index.RestrictedIndicesNames;
import org.elasticsearch.xpack.core.security.user.APMSystemUser;
import org.elasticsearch.xpack.core.security.user.BeatsSystemUser;
import org.elasticsearch.xpack.core.security.user.LogstashSystemUser;
import org.elasticsearch.xpack.core.security.user.RemoteMonitoringUser;
import org.elasticsearch.xpack.core.security.user.SystemUser;
import org.elasticsearch.xpack.core.security.user.XPackUser;
import org.elasticsearch.xpack.core.watcher.execution.TriggeredWatchStoreField;
import org.elasticsearch.xpack.core.watcher.history.HistoryStoreField;
import org.elasticsearch.xpack.core.watcher.transport.actions.ack.AckWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.activate.ActivateWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.delete.DeleteWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.execute.ExecuteWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.get.GetWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.service.WatcherServiceAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.stats.WatcherStatsAction;
import org.elasticsearch.xpack.core.watcher.watch.Watch;

import java.time.ZoneOffset;
import java.time.ZonedDateTime;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.SortedMap;

import static org.hamcrest.Matchers.hasEntry;
import static org.hamcrest.Matchers.is;
import static org.mockito.Mockito.mock;

/**
 * Unit tests for the {@link ReservedRolesStore}
 */
public class ReservedRolesStoreTests extends ESTestCase {

    private static final String READ_CROSS_CLUSTER_NAME = "internal:transport/proxy/indices:data/read/query";

    public void testIsReserved() {
        assertThat(ReservedRolesStore.isReserved("kibana_system"), is(true));
        assertThat(ReservedRolesStore.isReserved("superuser"), is(true));
        assertThat(ReservedRolesStore.isReserved("foobar"), is(false));
        assertThat(ReservedRolesStore.isReserved(SystemUser.ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved("transport_client"), is(true));
        assertThat(ReservedRolesStore.isReserved("kibana_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("ingest_admin"), is(true));
        assertThat(ReservedRolesStore.isReserved("monitoring_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("reporting_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("machine_learning_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("machine_learning_admin"), is(true));
        assertThat(ReservedRolesStore.isReserved("watcher_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("watcher_admin"), is(true));
        assertThat(ReservedRolesStore.isReserved("kibana_dashboard_only_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("beats_admin"), is(true));
        assertThat(ReservedRolesStore.isReserved(XPackUser.ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved(LogstashSystemUser.ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved(BeatsSystemUser.ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved(APMSystemUser.ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved(RemoteMonitoringUser.COLLECTION_ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved(RemoteMonitoringUser.INDEXING_ROLE_NAME), is(true));
        assertThat(ReservedRolesStore.isReserved("snapshot_user"), is(true));
        assertThat(ReservedRolesStore.isReserved("code_admin"), is(true));
        assertThat(ReservedRolesStore.isReserved("code_user"), is(true));
    }

    public void testSnapshotUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("snapshot_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role snapshotUserRole = Role.builder(roleDescriptor, null).build();
        assertThat(snapshotUserRole.cluster().check(GetRepositoriesAction.NAME, request), is(true));
        assertThat(snapshotUserRole.cluster().check(CreateSnapshotAction.NAME, request), is(true));
        assertThat(snapshotUserRole.cluster().check(SnapshotsStatusAction.NAME, request), is(true));
        assertThat(snapshotUserRole.cluster().check(GetSnapshotsAction.NAME, request), is(true));

        assertThat(snapshotUserRole.cluster().check(PutRepositoryAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(GetIndexTemplatesAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(DeleteIndexTemplateAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(PutPipelineAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(GetPipelineAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(DeletePipelineAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(GetWatchAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(PutWatchAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(DeleteWatchAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(ExecuteWatchAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(AckWatchAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(ActivateWatchAction.NAME, request), is(false));
        assertThat(snapshotUserRole.cluster().check(WatcherServiceAction.NAME, request), is(false));

        assertThat(snapshotUserRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(randomAlphaOfLengthBetween(8, 24)), is(false));
        assertThat(snapshotUserRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)), is(false));
        assertThat(snapshotUserRole.indices().allowedIndicesMatcher(GetAction.NAME).test(randomAlphaOfLengthBetween(8, 24)), is(false));
        assertThat(snapshotUserRole.indices().allowedIndicesMatcher(GetAction.NAME).test(randomAlphaOfLengthBetween(8, 24)), is(false));

        assertThat(snapshotUserRole.indices().allowedIndicesMatcher(GetIndexAction.NAME)
                .test(randomAlphaOfLengthBetween(8, 24)), is(true));
        assertThat(snapshotUserRole.indices().allowedIndicesMatcher(GetIndexAction.NAME)
                .test(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX), is(true));
        assertThat(snapshotUserRole.indices().allowedIndicesMatcher(GetIndexAction.NAME)
                .test(RestrictedIndicesNames.SECURITY_INDEX_NAME), is(true));

        assertNoAccessAllowed(snapshotUserRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testIngestAdminRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("ingest_admin");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role ingestAdminRole = Role.builder(roleDescriptor, null).build();
        assertThat(ingestAdminRole.cluster().check(PutIndexTemplateAction.NAME, request), is(true));
        assertThat(ingestAdminRole.cluster().check(GetIndexTemplatesAction.NAME, request), is(true));
        assertThat(ingestAdminRole.cluster().check(DeleteIndexTemplateAction.NAME, request), is(true));
        assertThat(ingestAdminRole.cluster().check(PutPipelineAction.NAME, request), is(true));
        assertThat(ingestAdminRole.cluster().check(GetPipelineAction.NAME, request), is(true));
        assertThat(ingestAdminRole.cluster().check(DeletePipelineAction.NAME, request), is(true));

        assertThat(ingestAdminRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(ingestAdminRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(ingestAdminRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(ingestAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(ingestAdminRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                is(false));
        assertThat(ingestAdminRole.indices().allowedIndicesMatcher(GetAction.NAME).test(randomAlphaOfLengthBetween(8, 24)),
                is(false));

        assertNoAccessAllowed(ingestAdminRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testKibanaSystemRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("kibana_system");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role kibanaRole = Role.builder(roleDescriptor, null).build();
        assertThat(kibanaRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(ClusterStateAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(ClusterStatsAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(PutIndexTemplateAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(GetIndexTemplatesAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(kibanaRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(kibanaRole.cluster().check(MonitoringBulkAction.NAME, request), is(true));

        // SAML and token
        assertThat(kibanaRole.cluster().check(SamlPrepareAuthenticationAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(SamlAuthenticateAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(InvalidateTokenAction.NAME, request), is(true));
        assertThat(kibanaRole.cluster().check(CreateTokenAction.NAME, request), is(true));

        // Application Privileges
        DeletePrivilegesRequest deleteKibanaPrivileges = new DeletePrivilegesRequest("kibana-.kibana", new String[]{ "all", "read" });
        DeletePrivilegesRequest deleteLogstashPrivileges = new DeletePrivilegesRequest("logstash", new String[]{ "all", "read" });
        assertThat(kibanaRole.cluster().check(DeletePrivilegesAction.NAME, deleteKibanaPrivileges), is(true));
        assertThat(kibanaRole.cluster().check(DeletePrivilegesAction.NAME, deleteLogstashPrivileges), is(false));

        GetPrivilegesRequest getKibanaPrivileges = new GetPrivilegesRequest();
        getKibanaPrivileges.application("kibana-.kibana-sales");
        GetPrivilegesRequest getApmPrivileges = new GetPrivilegesRequest();
        getApmPrivileges.application("apm");
        assertThat(kibanaRole.cluster().check(GetPrivilegesAction.NAME, getKibanaPrivileges), is(true));
        assertThat(kibanaRole.cluster().check(GetPrivilegesAction.NAME, getApmPrivileges), is(false));

        PutPrivilegesRequest putKibanaPrivileges = new PutPrivilegesRequest();
        putKibanaPrivileges.setPrivileges(Collections.singletonList(new ApplicationPrivilegeDescriptor(
            "kibana-.kibana-" + randomAlphaOfLengthBetween(2,6), "all", Collections.emptySet(), Collections.emptyMap())));
        PutPrivilegesRequest putSwiftypePrivileges = new PutPrivilegesRequest();
        putSwiftypePrivileges.setPrivileges(Collections.singletonList(new ApplicationPrivilegeDescriptor(
            "swiftype-kibana" , "all", Collections.emptySet(), Collections.emptyMap())));
        assertThat(kibanaRole.cluster().check(PutPrivilegesAction.NAME, putKibanaPrivileges), is(true));
        assertThat(kibanaRole.cluster().check(PutPrivilegesAction.NAME, putSwiftypePrivileges), is(false));

        // Everything else
        assertThat(kibanaRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertThat(kibanaRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)), is(false));

        Arrays.asList(".kibana", ".kibana-devnull", ".reporting-" + randomAlphaOfLength(randomIntBetween(0, 13))).forEach((index) -> {
            logger.info("index name [{}]", index);
            assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
            // inherits from 'all'
            assertThat(kibanaRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test(index), is(true));
        });

        // read-only index access
        Arrays.asList(".monitoring-" + randomAlphaOfLength(randomIntBetween(0, 13))).forEach((index) -> {
            logger.info("index name [{}]", index);
            assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
            assertThat(kibanaRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test(index), is(true));
        });

        // Beats management index
        final String index = ".management-beats";
        assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(false));
        assertThat(kibanaRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(false));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(true));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(true));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(true));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(kibanaRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test(index), is(false));

        assertNoAccessAllowed(kibanaRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testKibanaUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("kibana_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role kibanaUserRole = Role.builder(roleDescriptor, null).build();
        assertThat(kibanaUserRole.cluster().check(ClusterHealthAction.NAME, request), is(false));
        assertThat(kibanaUserRole.cluster().check(ClusterStateAction.NAME, request), is(false));
        assertThat(kibanaUserRole.cluster().check(ClusterStatsAction.NAME, request), is(false));
        assertThat(kibanaUserRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(kibanaUserRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(kibanaUserRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(kibanaUserRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(kibanaUserRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertThat(kibanaUserRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(kibanaUserRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(kibanaUserRole.indices().allowedIndicesMatcher("indices:foo")
                .test(randomAlphaOfLengthBetween(8, 24)), is(false));

        final String randomApplication = "kibana-" + randomAlphaOfLengthBetween(8, 24);
        assertThat(kibanaUserRole.application().grants(new ApplicationPrivilege(randomApplication, "app-random", "all"), "*"), is(false));

        final String application = "kibana-.kibana";
        assertThat(kibanaUserRole.application().grants(new ApplicationPrivilege(application, "app-foo", "foo"), "*"), is(false));
        assertThat(kibanaUserRole.application().grants(new ApplicationPrivilege(application, "app-all", "all"), "*"), is(true));

        final String applicationWithRandomIndex = "kibana-.kibana_" + randomAlphaOfLengthBetween(8, 24);
        assertThat(kibanaUserRole.application().grants(new ApplicationPrivilege(applicationWithRandomIndex, "app-random-index", "all"),
            "*"), is(false));

        assertNoAccessAllowed(kibanaUserRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testMonitoringUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("monitoring_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role monitoringUserRole = Role.builder(roleDescriptor, null).build();
        assertThat(monitoringUserRole.cluster().check(MainAction.NAME, request), is(true));
        assertThat(monitoringUserRole.cluster().check(XPackInfoAction.NAME, request), is(true));
        assertThat(monitoringUserRole.cluster().check(ClusterHealthAction.NAME, request), is(false));
        assertThat(monitoringUserRole.cluster().check(ClusterStateAction.NAME, request), is(false));
        assertThat(monitoringUserRole.cluster().check(ClusterStatsAction.NAME, request), is(false));
        assertThat(monitoringUserRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(monitoringUserRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(monitoringUserRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(monitoringUserRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(monitoringUserRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test("foo"), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".reporting"), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".kibana"), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                   is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test("foo"), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test(".reporting"), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test(".kibana"), is(false));

        final String index = ".monitoring-" + randomAlphaOfLength(randomIntBetween(0, 13));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(monitoringUserRole.indices().allowedIndicesMatcher(READ_CROSS_CLUSTER_NAME).test(index), is(true));

        assertNoAccessAllowed(monitoringUserRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testRemoteMonitoringAgentRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("remote_monitoring_agent");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role remoteMonitoringAgentRole = Role.builder(roleDescriptor, null).build();
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterStateAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterStatsAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(PutIndexTemplateAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(GetWatchAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(PutWatchAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(DeleteWatchAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(ExecuteWatchAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(AckWatchAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(ActivateWatchAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(WatcherServiceAction.NAME, request), is(false));
        // we get this from the cluster:monitor privilege
        assertThat(remoteMonitoringAgentRole.cluster().check(WatcherStatsAction.NAME, request), is(true));

        assertThat(remoteMonitoringAgentRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test("foo"), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".reporting"), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".kibana"), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:foo")
                .test(randomAlphaOfLengthBetween(8, 24)), is(false));

        final String monitoringIndex = ".monitoring-" + randomAlphaOfLength(randomIntBetween(0, 13));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:foo").test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:bar").test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetAction.NAME).test(monitoringIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetIndexAction.NAME).test(monitoringIndex), is(true));

        final String metricbeatIndex = "metricbeat-" + randomAlphaOfLength(randomIntBetween(0, 13));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:foo").test(metricbeatIndex), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:bar").test(metricbeatIndex), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(metricbeatIndex), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(metricbeatIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(metricbeatIndex), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(metricbeatIndex), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(metricbeatIndex), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(metricbeatIndex), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetAction.NAME).test(metricbeatIndex), is(false));

        assertNoAccessAllowed(remoteMonitoringAgentRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testRemoteMonitoringCollectorRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("remote_monitoring_collector");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role remoteMonitoringAgentRole = Role.builder(roleDescriptor, null).build();
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterStateAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterStatsAction.NAME, request), is(true));
        assertThat(remoteMonitoringAgentRole.cluster().check(GetIndexTemplatesAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(DeleteIndexTemplateAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(remoteMonitoringAgentRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(remoteMonitoringAgentRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(RecoveryAction.NAME).test("foo"), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test("foo"), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".reporting"), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".kibana"), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetAction.NAME).test(".kibana"), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:foo")
            .test(randomAlphaOfLengthBetween(8, 24)), is(false));

        Arrays.asList(
            ".monitoring-" + randomAlphaOfLength(randomIntBetween(0, 13)),
            "metricbeat-" + randomAlphaOfLength(randomIntBetween(0, 13))
        ).forEach((index) -> {
            logger.info("index name [{}]", index);
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(false));
            assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetIndexAction.NAME).test(index), is(false));
        });

        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetSettingsAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndicesShardStoresAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(UpgradeStatusAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(RecoveryAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndicesStatsAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndicesSegmentsAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(true));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(SearchAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(GetAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(DeleteAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(false));
        assertThat(remoteMonitoringAgentRole.indices().allowedIndicesMatcher(IndexAction.NAME)
                .test(randomFrom(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME)), is(false));

        assertMonitoringOnRestrictedIndices(remoteMonitoringAgentRole);

        assertNoAccessAllowed(remoteMonitoringAgentRole, RestrictedIndicesNames.NAMES_SET);
    }

    private void assertMonitoringOnRestrictedIndices(Role role) {
        final Settings indexSettings = Settings.builder().put("index.version.created", Version.CURRENT).build();
        final MetaData metaData = new MetaData.Builder()
                .put(new IndexMetaData.Builder(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX)
                        .settings(indexSettings)
                        .numberOfShards(1)
                        .numberOfReplicas(0)
                        .putAlias(new AliasMetaData.Builder(RestrictedIndicesNames.SECURITY_INDEX_NAME).build())
                        .build(), true)
                .build();
        final FieldPermissionsCache fieldPermissionsCache = new FieldPermissionsCache(Settings.EMPTY);
        final List<String> indexMonitoringActionNamesList = Arrays.asList(IndicesStatsAction.NAME, IndicesSegmentsAction.NAME,
                GetSettingsAction.NAME, IndicesShardStoresAction.NAME, UpgradeStatusAction.NAME, RecoveryAction.NAME);
        for (final String indexMonitoringActionName : indexMonitoringActionNamesList) {
            final Map<String, IndexAccessControl> authzMap = role.indices().authorize(indexMonitoringActionName,
                Sets.newHashSet(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX, RestrictedIndicesNames.SECURITY_INDEX_NAME),
                metaData.getAliasAndIndexLookup(), fieldPermissionsCache);
            assertThat(authzMap.get(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX).isGranted(), is(true));
            assertThat(authzMap.get(RestrictedIndicesNames.SECURITY_INDEX_NAME).isGranted(), is(true));
        }
    }

    public void testReportingUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("reporting_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role reportingUserRole = Role.builder(roleDescriptor, null).build();
        assertThat(reportingUserRole.cluster().check(ClusterHealthAction.NAME, request), is(false));
        assertThat(reportingUserRole.cluster().check(ClusterStateAction.NAME, request), is(false));
        assertThat(reportingUserRole.cluster().check(ClusterStatsAction.NAME, request), is(false));
        assertThat(reportingUserRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(reportingUserRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(reportingUserRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(reportingUserRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(reportingUserRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertThat(reportingUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test("foo"), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".reporting"), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".kibana"), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                is(false));

        final String index = ".reporting-" + randomAlphaOfLength(randomIntBetween(0, 13));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(UpdateAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
        assertThat(reportingUserRole.indices().allowedIndicesMatcher(BulkAction.NAME).test(index), is(false));

        assertNoAccessAllowed(reportingUserRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testKibanaDashboardOnlyUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("kibana_dashboard_only_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role dashboardsOnlyUserRole = Role.builder(roleDescriptor, null).build();
        assertThat(dashboardsOnlyUserRole.cluster().check(ClusterHealthAction.NAME, request), is(false));
        assertThat(dashboardsOnlyUserRole.cluster().check(ClusterStateAction.NAME, request), is(false));
        assertThat(dashboardsOnlyUserRole.cluster().check(ClusterStatsAction.NAME, request), is(false));
        assertThat(dashboardsOnlyUserRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(dashboardsOnlyUserRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(dashboardsOnlyUserRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(dashboardsOnlyUserRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(dashboardsOnlyUserRole.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        final String randomApplication = "kibana-" + randomAlphaOfLengthBetween(8, 24);
        assertThat(dashboardsOnlyUserRole.application().grants(new ApplicationPrivilege(randomApplication, "app-random", "all"), "*"),
            is(false));

        final String application = "kibana-.kibana";
        assertThat(dashboardsOnlyUserRole.application().grants(new ApplicationPrivilege(application, "app-foo", "foo"), "*"), is(false));
        assertThat(dashboardsOnlyUserRole.application().grants(new ApplicationPrivilege(application, "app-all", "all"), "*"), is(false));
        assertThat(dashboardsOnlyUserRole.application().grants(new ApplicationPrivilege(application, "app-read", "read"), "*"), is(true));

        final String applicationWithRandomIndex = "kibana-.kibana_" + randomAlphaOfLengthBetween(8, 24);
        assertThat(dashboardsOnlyUserRole.application().grants(
            new ApplicationPrivilege(applicationWithRandomIndex, "app-random-index", "all"), "*"), is(false));

        assertNoAccessAllowed(dashboardsOnlyUserRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testSuperuserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("superuser");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role superuserRole = Role.builder(roleDescriptor, null).build();
        assertThat(superuserRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(superuserRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(true));
        assertThat(superuserRole.cluster().check(PutUserAction.NAME, request), is(true));
        assertThat(superuserRole.cluster().check(PutRoleAction.NAME, request), is(true));
        assertThat(superuserRole.cluster().check(PutIndexTemplateAction.NAME, request), is(true));
        assertThat(superuserRole.cluster().check("internal:admin/foo", request), is(false));

        final Settings indexSettings = Settings.builder().put("index.version.created", Version.CURRENT).build();
        final MetaData metaData = new MetaData.Builder()
                .put(new IndexMetaData.Builder("a1").settings(indexSettings).numberOfShards(1).numberOfReplicas(0).build(), true)
                .put(new IndexMetaData.Builder("a2").settings(indexSettings).numberOfShards(1).numberOfReplicas(0).build(), true)
                .put(new IndexMetaData.Builder("aaaaaa").settings(indexSettings).numberOfShards(1).numberOfReplicas(0).build(), true)
                .put(new IndexMetaData.Builder("bbbbb").settings(indexSettings).numberOfShards(1).numberOfReplicas(0).build(), true)
                .put(new IndexMetaData.Builder("b")
                        .settings(indexSettings)
                        .numberOfShards(1)
                        .numberOfReplicas(0)
                        .putAlias(new AliasMetaData.Builder("ab").build())
                        .putAlias(new AliasMetaData.Builder("ba").build())
                        .build(), true)
                .put(new IndexMetaData.Builder(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX)
                        .settings(indexSettings)
                        .numberOfShards(1)
                        .numberOfReplicas(0)
                        .putAlias(new AliasMetaData.Builder(RestrictedIndicesNames.SECURITY_INDEX_NAME).build())
                        .build(), true)
                .build();

        FieldPermissionsCache fieldPermissionsCache = new FieldPermissionsCache(Settings.EMPTY);
        SortedMap<String, AliasOrIndex> lookup = metaData.getAliasAndIndexLookup();
        Map<String, IndexAccessControl> authzMap =
                superuserRole.indices().authorize(SearchAction.NAME, Sets.newHashSet("a1", "ba"), lookup, fieldPermissionsCache);
        assertThat(authzMap.get("a1").isGranted(), is(true));
        assertThat(authzMap.get("b").isGranted(), is(true));
        authzMap =
            superuserRole.indices().authorize(DeleteIndexAction.NAME, Sets.newHashSet("a1", "ba"), lookup, fieldPermissionsCache);
        assertThat(authzMap.get("a1").isGranted(), is(true));
        assertThat(authzMap.get("b").isGranted(), is(true));
        authzMap = superuserRole.indices().authorize(IndexAction.NAME, Sets.newHashSet("a2", "ba"), lookup, fieldPermissionsCache);
        assertThat(authzMap.get("a2").isGranted(), is(true));
        assertThat(authzMap.get("b").isGranted(), is(true));
        authzMap = superuserRole.indices()
                .authorize(UpdateSettingsAction.NAME, Sets.newHashSet("aaaaaa", "ba"), lookup, fieldPermissionsCache);
        assertThat(authzMap.get("aaaaaa").isGranted(), is(true));
        assertThat(authzMap.get("b").isGranted(), is(true));
        authzMap = superuserRole.indices().authorize(randomFrom(IndexAction.NAME, DeleteIndexAction.NAME, SearchAction.NAME),
                Sets.newHashSet(RestrictedIndicesNames.SECURITY_INDEX_NAME), lookup, fieldPermissionsCache);
        assertThat(authzMap.get(RestrictedIndicesNames.SECURITY_INDEX_NAME).isGranted(), is(true));
        assertThat(authzMap.get(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX).isGranted(), is(true));
        assertTrue(superuserRole.indices().check(SearchAction.NAME));
        assertFalse(superuserRole.indices().check("unknown"));

        assertThat(superuserRole.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(true));

        assertThat(superuserRole.indices().allowedIndicesMatcher(randomFrom(IndexAction.NAME, DeleteIndexAction.NAME, SearchAction.NAME))
                .test(RestrictedIndicesNames.SECURITY_INDEX_NAME), is(true));
        assertThat(superuserRole.indices().allowedIndicesMatcher(randomFrom(IndexAction.NAME, DeleteIndexAction.NAME, SearchAction.NAME))
                .test(RestrictedIndicesNames.INTERNAL_SECURITY_INDEX), is(true));
    }

    public void testLogstashSystemRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("logstash_system");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role logstashSystemRole = Role.builder(roleDescriptor, null).build();
        assertThat(logstashSystemRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(logstashSystemRole.cluster().check(ClusterStateAction.NAME, request), is(true));
        assertThat(logstashSystemRole.cluster().check(ClusterStatsAction.NAME, request), is(true));
        assertThat(logstashSystemRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(logstashSystemRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(logstashSystemRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(logstashSystemRole.cluster().check(MonitoringBulkAction.NAME, request), is(true));

        assertThat(logstashSystemRole.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(logstashSystemRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(logstashSystemRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(logstashSystemRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                is(false));

        assertNoAccessAllowed(logstashSystemRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testBeatsAdminRole() {
        final TransportRequest request = mock(TransportRequest.class);

        final RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("beats_admin");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));


        final Role beatsAdminRole = Role.builder(roleDescriptor, null).build();
        assertThat(beatsAdminRole.cluster().check(ClusterHealthAction.NAME, request), is(false));
        assertThat(beatsAdminRole.cluster().check(ClusterStateAction.NAME, request), is(false));
        assertThat(beatsAdminRole.cluster().check(ClusterStatsAction.NAME, request), is(false));
        assertThat(beatsAdminRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(beatsAdminRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(beatsAdminRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(beatsAdminRole.cluster().check(MonitoringBulkAction.NAME, request), is(false));

        assertThat(beatsAdminRole.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(beatsAdminRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
            is(false));

        final String index = ".management-beats";
        logger.info("index name [{}]", index);
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher("indices:foo").test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher("indices:bar").test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
        assertThat(beatsAdminRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));

        assertNoAccessAllowed(beatsAdminRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testBeatsSystemRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor(BeatsSystemUser.ROLE_NAME);
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role logstashSystemRole = Role.builder(roleDescriptor, null).build();
        assertThat(logstashSystemRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(logstashSystemRole.cluster().check(ClusterStateAction.NAME, request), is(true));
        assertThat(logstashSystemRole.cluster().check(ClusterStatsAction.NAME, request), is(true));
        assertThat(logstashSystemRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(logstashSystemRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(logstashSystemRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(logstashSystemRole.cluster().check(MonitoringBulkAction.NAME, request), is(true));

        assertThat(logstashSystemRole.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(logstashSystemRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(logstashSystemRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(logstashSystemRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                is(false));

        assertNoAccessAllowed(logstashSystemRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testAPMSystemRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor(APMSystemUser.ROLE_NAME);
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role APMSystemRole = Role.builder(roleDescriptor, null).build();
        assertThat(APMSystemRole.cluster().check(ClusterHealthAction.NAME, request), is(true));
        assertThat(APMSystemRole.cluster().check(ClusterStateAction.NAME, request), is(true));
        assertThat(APMSystemRole.cluster().check(ClusterStatsAction.NAME, request), is(true));
        assertThat(APMSystemRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(APMSystemRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(APMSystemRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));
        assertThat(APMSystemRole.cluster().check(MonitoringBulkAction.NAME, request), is(true));

        assertThat(APMSystemRole.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(APMSystemRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(APMSystemRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(APMSystemRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                is(false));

        assertNoAccessAllowed(APMSystemRole, RestrictedIndicesNames.NAMES_SET);
    }

    public void testAPMUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        final RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("apm_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role role = Role.builder(roleDescriptor, null).build();

        assertThat(role.runAs().check(randomAlphaOfLengthBetween(1, 12)), is(false));

        assertNoAccessAllowed(role, "foo");

        assertOnlyReadAllowed(role, "apm-" + randomIntBetween(0, 5));
        assertOnlyReadAllowed(role, AnomalyDetectorsIndexFields.RESULTS_INDEX_PREFIX + AnomalyDetectorsIndexFields.RESULTS_INDEX_DEFAULT);
    }

    public void testMachineLearningAdminRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("machine_learning_admin");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role role = Role.builder(roleDescriptor, null).build();
        assertThat(role.cluster().check(CloseJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteCalendarAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteCalendarEventAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteDatafeedAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteExpiredDataAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteFilterAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteForecastAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteModelSnapshotAction.NAME, request), is(true));
        assertThat(role.cluster().check(FinalizeJobExecutionAction.NAME, request), is(false)); // internal use only
        assertThat(role.cluster().check(FindFileStructureAction.NAME, request), is(true));
        assertThat(role.cluster().check(FlushJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(ForecastJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetBucketsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetCalendarEventsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetCalendarsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetCategoriesAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetDatafeedsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetDatafeedsStatsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetFiltersAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetInfluencersAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetJobsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetJobsStatsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetModelSnapshotsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetOverallBucketsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetRecordsAction.NAME, request), is(true));
        assertThat(role.cluster().check(IsolateDatafeedAction.NAME, request), is(false)); // internal use only
        assertThat(role.cluster().check(KillProcessAction.NAME, request), is(false)); // internal use only
        assertThat(role.cluster().check(MlInfoAction.NAME, request), is(true));
        assertThat(role.cluster().check(OpenJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(PersistJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(PostCalendarEventsAction.NAME, request), is(true));
        assertThat(role.cluster().check(PostDataAction.NAME, request), is(true));
        assertThat(role.cluster().check(PreviewDatafeedAction.NAME, request), is(true));
        assertThat(role.cluster().check(PutCalendarAction.NAME, request), is(true));
        assertThat(role.cluster().check(PutDatafeedAction.NAME, request), is(true));
        assertThat(role.cluster().check(PutFilterAction.NAME, request), is(true));
        assertThat(role.cluster().check(PutJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(RevertModelSnapshotAction.NAME, request), is(true));
        assertThat(role.cluster().check(SetUpgradeModeAction.NAME, request), is(true));
        assertThat(role.cluster().check(StartDatafeedAction.NAME, request), is(true));
        assertThat(role.cluster().check(StopDatafeedAction.NAME, request), is(true));
        assertThat(role.cluster().check(UpdateCalendarJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(UpdateDatafeedAction.NAME, request), is(true));
        assertThat(role.cluster().check(UpdateFilterAction.NAME, request), is(true));
        assertThat(role.cluster().check(UpdateJobAction.NAME, request), is(true));
        assertThat(role.cluster().check(UpdateModelSnapshotAction.NAME, request), is(true));
        assertThat(role.cluster().check(UpdateProcessAction.NAME, request), is(false)); // internal use only
        assertThat(role.cluster().check(ValidateDetectorAction.NAME, request), is(true));
        assertThat(role.cluster().check(ValidateJobConfigAction.NAME, request), is(true));
        assertThat(role.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertNoAccessAllowed(role, "foo");
        assertNoAccessAllowed(role, AnomalyDetectorsIndexFields.CONFIG_INDEX); // internal use only
        assertOnlyReadAllowed(role, MlMetaIndex.INDEX_NAME);
        assertOnlyReadAllowed(role, AnomalyDetectorsIndexFields.STATE_INDEX_PREFIX);
        assertOnlyReadAllowed(role, AnomalyDetectorsIndexFields.RESULTS_INDEX_PREFIX + AnomalyDetectorsIndexFields.RESULTS_INDEX_DEFAULT);
        assertOnlyReadAllowed(role, AuditorField.NOTIFICATIONS_INDEX);
        assertReadWriteDocsButNotDeleteIndexAllowed(role, AnnotationIndex.INDEX_NAME);

        assertNoAccessAllowed(role, RestrictedIndicesNames.NAMES_SET);
    }

    public void testMachineLearningUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("machine_learning_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role role = Role.builder(roleDescriptor, null).build();
        assertThat(role.cluster().check(CloseJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteCalendarAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteCalendarEventAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteExpiredDataAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteFilterAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteForecastAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(DeleteModelSnapshotAction.NAME, request), is(false));
        assertThat(role.cluster().check(FinalizeJobExecutionAction.NAME, request), is(false));
        assertThat(role.cluster().check(FindFileStructureAction.NAME, request), is(true));
        assertThat(role.cluster().check(FlushJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(ForecastJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(GetBucketsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetCalendarEventsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetCalendarsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetCategoriesAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetDatafeedsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetDatafeedsStatsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetFiltersAction.NAME, request), is(false));
        assertThat(role.cluster().check(GetInfluencersAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetJobsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetJobsStatsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetModelSnapshotsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetOverallBucketsAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetRecordsAction.NAME, request), is(true));
        assertThat(role.cluster().check(IsolateDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(KillProcessAction.NAME, request), is(false));
        assertThat(role.cluster().check(MlInfoAction.NAME, request), is(true));
        assertThat(role.cluster().check(OpenJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(PersistJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(PostCalendarEventsAction.NAME, request), is(false));
        assertThat(role.cluster().check(PostDataAction.NAME, request), is(false));
        assertThat(role.cluster().check(PreviewDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(PutCalendarAction.NAME, request), is(false));
        assertThat(role.cluster().check(PutDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(PutFilterAction.NAME, request), is(false));
        assertThat(role.cluster().check(PutJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(RevertModelSnapshotAction.NAME, request), is(false));
        assertThat(role.cluster().check(SetUpgradeModeAction.NAME, request), is(false));
        assertThat(role.cluster().check(StartDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(StopDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(UpdateCalendarJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(UpdateDatafeedAction.NAME, request), is(false));
        assertThat(role.cluster().check(UpdateFilterAction.NAME, request), is(false));
        assertThat(role.cluster().check(UpdateJobAction.NAME, request), is(false));
        assertThat(role.cluster().check(UpdateModelSnapshotAction.NAME, request), is(false));
        assertThat(role.cluster().check(UpdateProcessAction.NAME, request), is(false));
        assertThat(role.cluster().check(ValidateDetectorAction.NAME, request), is(false));
        assertThat(role.cluster().check(ValidateJobConfigAction.NAME, request), is(false));
        assertThat(role.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertNoAccessAllowed(role, "foo");
        assertNoAccessAllowed(role, AnomalyDetectorsIndexFields.CONFIG_INDEX);
        assertNoAccessAllowed(role, MlMetaIndex.INDEX_NAME);
        assertNoAccessAllowed(role, AnomalyDetectorsIndexFields.STATE_INDEX_PREFIX);
        assertOnlyReadAllowed(role, AnomalyDetectorsIndexFields.RESULTS_INDEX_PREFIX + AnomalyDetectorsIndexFields.RESULTS_INDEX_DEFAULT);
        assertOnlyReadAllowed(role, AuditorField.NOTIFICATIONS_INDEX);
        assertReadWriteDocsButNotDeleteIndexAllowed(role, AnnotationIndex.INDEX_NAME);

        assertNoAccessAllowed(role, RestrictedIndicesNames.NAMES_SET);
    }

    public void testWatcherAdminRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("watcher_admin");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role role = Role.builder(roleDescriptor, null).build();
        assertThat(role.cluster().check(PutWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(GetWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(ExecuteWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(AckWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(ActivateWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(WatcherServiceAction.NAME, request), is(true));
        assertThat(role.cluster().check(WatcherStatsAction.NAME, request), is(true));
        assertThat(role.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(role.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));

        ZonedDateTime now = ZonedDateTime.now(ZoneOffset.UTC);
        String historyIndex = HistoryStoreField.getHistoryIndexNameForTime(now);
        for (String index : new String[]{ Watch.INDEX, historyIndex, TriggeredWatchStoreField.INDEX_NAME }) {
            assertOnlyReadAllowed(role, index);
        }

        assertNoAccessAllowed(role, RestrictedIndicesNames.NAMES_SET);
    }

    public void testWatcherUserRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("watcher_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role role = Role.builder(roleDescriptor, null).build();
        assertThat(role.cluster().check(PutWatchAction.NAME, request), is(false));
        assertThat(role.cluster().check(GetWatchAction.NAME, request), is(true));
        assertThat(role.cluster().check(DeleteWatchAction.NAME, request), is(false));
        assertThat(role.cluster().check(ExecuteWatchAction.NAME, request), is(false));
        assertThat(role.cluster().check(AckWatchAction.NAME, request), is(false));
        assertThat(role.cluster().check(ActivateWatchAction.NAME, request), is(false));
        assertThat(role.cluster().check(WatcherServiceAction.NAME, request), is(false));
        assertThat(role.cluster().check(WatcherStatsAction.NAME, request), is(true));
        assertThat(role.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(role.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(role.indices().allowedIndicesMatcher(IndexAction.NAME).test(TriggeredWatchStoreField.INDEX_NAME), is(false));

        ZonedDateTime now = ZonedDateTime.now(ZoneOffset.UTC);
        String historyIndex = HistoryStoreField.getHistoryIndexNameForTime(now);
        for (String index : new String[]{ Watch.INDEX, historyIndex }) {
            assertOnlyReadAllowed(role, index);
        }

        assertNoAccessAllowed(role, RestrictedIndicesNames.NAMES_SET);
    }

    private void assertReadWriteDocsButNotDeleteIndexAllowed(Role role, String index) {
        assertThat(role.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(UpdateAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(BulkAction.NAME).test(index), is(true));
    }

    private void assertOnlyReadAllowed(Role role, String index) {
        assertThat(role.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(role.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(UpdateAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(BulkAction.NAME).test(index), is(false));

        assertNoAccessAllowed(role, RestrictedIndicesNames.NAMES_SET);
    }

    private void assertNoAccessAllowed(Role role, Collection<String> indices) {
        for (String index : indices) {
            assertNoAccessAllowed(role, index);
        }
    }

    private void assertNoAccessAllowed(Role role, String index) {
        assertThat(role.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(UpdateAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
        assertThat(role.indices().allowedIndicesMatcher(BulkAction.NAME).test(index), is(false));
    }

    public void testLogstashAdminRole() {
        final TransportRequest request = mock(TransportRequest.class);

        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("logstash_admin");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role logstashAdminRole = Role.builder(roleDescriptor, null).build();
        assertThat(logstashAdminRole.cluster().check(ClusterHealthAction.NAME, request), is(false));
        assertThat(logstashAdminRole.cluster().check(PutIndexTemplateAction.NAME, request), is(false));
        assertThat(logstashAdminRole.cluster().check(ClusterRerouteAction.NAME, request), is(false));
        assertThat(logstashAdminRole.cluster().check(ClusterUpdateSettingsAction.NAME, request), is(false));

        assertThat(logstashAdminRole.runAs().check(randomAlphaOfLengthBetween(1, 30)), is(false));

        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".logstash"), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
                   is(false));

        final String index = ".logstash-" + randomIntBetween(0, 5);

        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
        assertThat(logstashAdminRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(true));
    }

    public void testCodeAdminRole() {
        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("code_admin");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role codeAdminRole = Role.builder(roleDescriptor, null).build();


        assertThat(codeAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test("foo"), is(false));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".reporting"), is(false));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(".code-"), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
            is(false));

        final String index = ".code-" + randomIntBetween(0, 5);

        assertThat(codeAdminRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
        assertThat(codeAdminRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(true));
    }

    public void testCodeUserRole() {
        RoleDescriptor roleDescriptor = new ReservedRolesStore().roleDescriptor("code_user");
        assertNotNull(roleDescriptor);
        assertThat(roleDescriptor.getMetadata(), hasEntry("_reserved", true));

        Role codeUserRole = Role.builder(roleDescriptor, null).build();


        assertThat(codeUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test("foo"), is(false));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".reporting"), is(false));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(".code-"), is(true));
        assertThat(codeUserRole.indices().allowedIndicesMatcher("indices:foo").test(randomAlphaOfLengthBetween(8, 24)),
            is(false));

        final String index = ".code-" + randomIntBetween(0, 5);

        assertThat(codeUserRole.indices().allowedIndicesMatcher(DeleteAction.NAME).test(index), is(false));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(DeleteIndexAction.NAME).test(index), is(false));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(CreateIndexAction.NAME).test(index), is(false));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(IndexAction.NAME).test(index), is(false));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(GetAction.NAME).test(index), is(true));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(SearchAction.NAME).test(index), is(true));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(MultiSearchAction.NAME).test(index), is(true));
        assertThat(codeUserRole.indices().allowedIndicesMatcher(UpdateSettingsAction.NAME).test(index), is(false));
    }
}
