/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.XPackFeatureSet;
import org.elasticsearch.xpack.core.XPackFeatureSet.Usage;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.ml.MachineLearningFeatureSetUsage;
import org.elasticsearch.xpack.core.ml.MachineLearningField;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.action.GetDatafeedsStatsAction;
import org.elasticsearch.xpack.core.ml.action.GetJobsStatsAction;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedState;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Detector;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSizeStats;
import org.elasticsearch.xpack.core.ml.stats.ForecastStats;
import org.elasticsearch.xpack.core.ml.stats.ForecastStatsTests;
import org.elasticsearch.xpack.core.watcher.support.xcontent.XContentSource;
import org.junit.Before;

import java.util.Arrays;
import java.util.Collections;
import java.util.Date;
import java.util.List;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.core.Is.is;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.same;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class MachineLearningFeatureSetTests extends ESTestCase {

    private Settings commonSettings;
    private ClusterService clusterService;
    private Client client;
    private XPackLicenseState licenseState;

    @Before
    public void init() {
        commonSettings = Settings.builder()
                .put(Environment.PATH_HOME_SETTING.getKey(), createTempDir().toAbsolutePath())
                .put(MachineLearningField.AUTODETECT_PROCESS.getKey(), false)
                .build();
        clusterService = mock(ClusterService.class);
        client = mock(Client.class);
        licenseState = mock(XPackLicenseState.class);
        givenJobs(Collections.emptyList(), Collections.emptyList());
        givenDatafeeds(Collections.emptyList());
    }

    public void testIsRunningOnMlPlatform() {
        assertTrue(MachineLearningFeatureSet.isRunningOnMlPlatform("Linux", "amd64", true));
        assertTrue(MachineLearningFeatureSet.isRunningOnMlPlatform("Windows 10", "amd64", true));
        assertTrue(MachineLearningFeatureSet.isRunningOnMlPlatform("Mac OS X", "x86_64", true));
        assertFalse(MachineLearningFeatureSet.isRunningOnMlPlatform("Linux", "i386", false));
        assertFalse(MachineLearningFeatureSet.isRunningOnMlPlatform("Windows 10", "i386", false));
        assertFalse(MachineLearningFeatureSet.isRunningOnMlPlatform("SunOS", "amd64", false));
        expectThrows(ElasticsearchException.class,
                () -> MachineLearningFeatureSet.isRunningOnMlPlatform("Linux", "i386", true));
        expectThrows(ElasticsearchException.class,
                () -> MachineLearningFeatureSet.isRunningOnMlPlatform("Windows 10", "i386", true));
        expectThrows(ElasticsearchException.class,
                () -> MachineLearningFeatureSet.isRunningOnMlPlatform("SunOS", "amd64", true));
    }

    public void testAvailable() throws Exception {
        MachineLearningFeatureSet featureSet = new MachineLearningFeatureSet(TestEnvironment.newEnvironment(commonSettings), clusterService,
                client, licenseState);
        boolean available = randomBoolean();
        when(licenseState.isMachineLearningAllowed()).thenReturn(available);
        assertThat(featureSet.available(), is(available));
        PlainActionFuture<Usage> future = new PlainActionFuture<>();
        featureSet.usage(future);
        XPackFeatureSet.Usage usage = future.get();
        assertThat(usage.available(), is(available));

        BytesStreamOutput out = new BytesStreamOutput();
        usage.writeTo(out);
        XPackFeatureSet.Usage serializedUsage = new MachineLearningFeatureSetUsage(out.bytes().streamInput());
        assertThat(serializedUsage.available(), is(available));
    }

    public void testEnabled() throws Exception {
        boolean useDefault = randomBoolean();
        boolean enabled = true;
        Settings.Builder settings = Settings.builder().put(commonSettings);
        if (useDefault == false) {
            enabled = randomBoolean();
            settings.put("xpack.ml.enabled", enabled);
        }
        boolean expected = enabled || useDefault;
        MachineLearningFeatureSet featureSet = new MachineLearningFeatureSet(TestEnvironment.newEnvironment(settings.build()),
                clusterService, client, licenseState);
        assertThat(featureSet.enabled(), is(expected));
        PlainActionFuture<Usage> future = new PlainActionFuture<>();
        featureSet.usage(future);
        XPackFeatureSet.Usage usage = future.get();
        assertThat(usage.enabled(), is(expected));

        BytesStreamOutput out = new BytesStreamOutput();
        usage.writeTo(out);
        XPackFeatureSet.Usage serializedUsage = new MachineLearningFeatureSetUsage(out.bytes().streamInput());
        assertThat(serializedUsage.enabled(), is(expected));
    }

    public void testUsage() throws Exception {
        when(licenseState.isMachineLearningAllowed()).thenReturn(true);
        Settings.Builder settings = Settings.builder().put(commonSettings);
        settings.put("xpack.ml.enabled", true);

        Job opened1 = buildJob("opened1", Arrays.asList(buildMinDetector("foo")));
        GetJobsStatsAction.Response.JobStats opened1JobStats = buildJobStats("opened1", JobState.OPENED, 100L, 3L);
        Job opened2 = buildJob("opened2", Arrays.asList(buildMinDetector("foo"), buildMinDetector("bar")));
        GetJobsStatsAction.Response.JobStats opened2JobStats = buildJobStats("opened2", JobState.OPENED, 200L, 8L);
        Job closed1 = buildJob("closed1", Arrays.asList(buildMinDetector("foo"), buildMinDetector("bar"), buildMinDetector("foobar")));
        GetJobsStatsAction.Response.JobStats closed1JobStats = buildJobStats("closed1", JobState.CLOSED, 300L, 0);
        givenJobs(Arrays.asList(opened1, opened2, closed1),
                Arrays.asList(opened1JobStats, opened2JobStats, closed1JobStats));

        givenDatafeeds(Arrays.asList(
                buildDatafeedStats(DatafeedState.STARTED),
                buildDatafeedStats(DatafeedState.STARTED),
                buildDatafeedStats(DatafeedState.STOPPED)
        ));

        MachineLearningFeatureSet featureSet = new MachineLearningFeatureSet(TestEnvironment.newEnvironment(settings.build()),
                clusterService, client, licenseState);
        PlainActionFuture<Usage> future = new PlainActionFuture<>();
        featureSet.usage(future);
        XPackFeatureSet.Usage mlUsage = future.get();

        BytesStreamOutput out = new BytesStreamOutput();
        mlUsage.writeTo(out);
        XPackFeatureSet.Usage serializedUsage = new MachineLearningFeatureSetUsage(out.bytes().streamInput());

        for (XPackFeatureSet.Usage usage : Arrays.asList(mlUsage, serializedUsage)) {
            assertThat(usage, is(notNullValue()));
            assertThat(usage.name(), is(XPackField.MACHINE_LEARNING));
            assertThat(usage.enabled(), is(true));
            assertThat(usage.available(), is(true));
            XContentSource source;
            try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
                usage.toXContent(builder, ToXContent.EMPTY_PARAMS);
                source = new XContentSource(builder);
            }

            assertThat(source.getValue("jobs._all.count"), equalTo(3));
            assertThat(source.getValue("jobs._all.detectors.min"), equalTo(1.0));
            assertThat(source.getValue("jobs._all.detectors.max"), equalTo(3.0));
            assertThat(source.getValue("jobs._all.detectors.total"), equalTo(6.0));
            assertThat(source.getValue("jobs._all.detectors.avg"), equalTo(2.0));
            assertThat(source.getValue("jobs._all.model_size.min"), equalTo(100.0));
            assertThat(source.getValue("jobs._all.model_size.max"), equalTo(300.0));
            assertThat(source.getValue("jobs._all.model_size.total"), equalTo(600.0));
            assertThat(source.getValue("jobs._all.model_size.avg"), equalTo(200.0));

            assertThat(source.getValue("jobs.opened.count"), equalTo(2));
            assertThat(source.getValue("jobs.opened.detectors.min"), equalTo(1.0));
            assertThat(source.getValue("jobs.opened.detectors.max"), equalTo(2.0));
            assertThat(source.getValue("jobs.opened.detectors.total"), equalTo(3.0));
            assertThat(source.getValue("jobs.opened.detectors.avg"), equalTo(1.5));
            assertThat(source.getValue("jobs.opened.model_size.min"), equalTo(100.0));
            assertThat(source.getValue("jobs.opened.model_size.max"), equalTo(200.0));
            assertThat(source.getValue("jobs.opened.model_size.total"), equalTo(300.0));
            assertThat(source.getValue("jobs.opened.model_size.avg"), equalTo(150.0));

            assertThat(source.getValue("jobs.closed.count"), equalTo(1));
            assertThat(source.getValue("jobs.closed.detectors.min"), equalTo(3.0));
            assertThat(source.getValue("jobs.closed.detectors.max"), equalTo(3.0));
            assertThat(source.getValue("jobs.closed.detectors.total"), equalTo(3.0));
            assertThat(source.getValue("jobs.closed.detectors.avg"), equalTo(3.0));
            assertThat(source.getValue("jobs.closed.model_size.min"), equalTo(300.0));
            assertThat(source.getValue("jobs.closed.model_size.max"), equalTo(300.0));
            assertThat(source.getValue("jobs.closed.model_size.total"), equalTo(300.0));
            assertThat(source.getValue("jobs.closed.model_size.avg"), equalTo(300.0));

            assertThat(source.getValue("jobs.opening"), is(nullValue()));
            assertThat(source.getValue("jobs.closing"), is(nullValue()));
            assertThat(source.getValue("jobs.failed"), is(nullValue()));

            assertThat(source.getValue("datafeeds._all.count"), equalTo(3));
            assertThat(source.getValue("datafeeds.started.count"), equalTo(2));
            assertThat(source.getValue("datafeeds.stopped.count"), equalTo(1));

            assertThat(source.getValue("jobs._all.forecasts.total"), equalTo(11));
            assertThat(source.getValue("jobs._all.forecasts.forecasted_jobs"), equalTo(2));

            assertThat(source.getValue("jobs.closed.forecasts.total"), equalTo(0));
            assertThat(source.getValue("jobs.closed.forecasts.forecasted_jobs"), equalTo(0));

            assertThat(source.getValue("jobs.opened.forecasts.total"), equalTo(11));
            assertThat(source.getValue("jobs.opened.forecasts.forecasted_jobs"), equalTo(2));
        }
    }

    public void testUsageGivenMlMetadataNotInstalled() throws Exception {
        when(licenseState.isMachineLearningAllowed()).thenReturn(true);
        Settings.Builder settings = Settings.builder().put(commonSettings);
        settings.put("xpack.ml.enabled", true);
        when(clusterService.state()).thenReturn(ClusterState.EMPTY_STATE);

        MachineLearningFeatureSet featureSet = new MachineLearningFeatureSet(TestEnvironment.newEnvironment(settings.build()),
                clusterService, client, licenseState);
        PlainActionFuture<Usage> future = new PlainActionFuture<>();
        featureSet.usage(future);
        XPackFeatureSet.Usage usage = future.get();

        assertThat(usage.available(), is(true));
        assertThat(usage.enabled(), is(true));

        XContentSource source;
        try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
            usage.toXContent(builder, ToXContent.EMPTY_PARAMS);
            source = new XContentSource(builder);
        }

        assertThat(source.getValue("jobs._all.count"), equalTo(0));
        assertThat(source.getValue("jobs._all.detectors.min"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.detectors.max"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.detectors.total"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.detectors.avg"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.model_size.min"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.model_size.max"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.model_size.total"), equalTo(0.0));
        assertThat(source.getValue("jobs._all.model_size.avg"), equalTo(0.0));

        assertThat(source.getValue("jobs.opening"), is(nullValue()));
        assertThat(source.getValue("jobs.opened"), is(nullValue()));
        assertThat(source.getValue("jobs.closing"), is(nullValue()));
        assertThat(source.getValue("jobs.closed"), is(nullValue()));
        assertThat(source.getValue("jobs.failed"), is(nullValue()));

        assertThat(source.getValue("datafeeds._all.count"), equalTo(0));

        assertThat(source.getValue("datafeeds.started"), is(nullValue()));
        assertThat(source.getValue("datafeeds.stopped"), is(nullValue()));
    }

    private void givenJobs(List<Job> jobs, List<GetJobsStatsAction.Response.JobStats> jobsStats) {
        MlMetadata.Builder mlMetadataBuilder = new MlMetadata.Builder();
        for (Job job : jobs) {
            mlMetadataBuilder.putJob(job, false);
        }
        ClusterState clusterState = new ClusterState.Builder(ClusterState.EMPTY_STATE)
                .metaData(new MetaData.Builder()
                        .putCustom(MlMetadata.TYPE, mlMetadataBuilder.build()))
                .build();
        when(clusterService.state()).thenReturn(clusterState);

        doAnswer(invocationOnMock -> {
            ActionListener<GetJobsStatsAction.Response> listener =
                    (ActionListener<GetJobsStatsAction.Response>) invocationOnMock.getArguments()[2];
            listener.onResponse(new GetJobsStatsAction.Response(
                    new QueryPage<>(jobsStats, jobsStats.size(), Job.RESULTS_FIELD)));
            return Void.TYPE;
        }).when(client).execute(same(GetJobsStatsAction.INSTANCE), any(), any());
    }

    private void givenDatafeeds(List<GetDatafeedsStatsAction.Response.DatafeedStats> datafeedStats) {
        doAnswer(invocationOnMock -> {
            ActionListener<GetDatafeedsStatsAction.Response> listener =
                    (ActionListener<GetDatafeedsStatsAction.Response>) invocationOnMock.getArguments()[2];
            listener.onResponse(new GetDatafeedsStatsAction.Response(
                    new QueryPage<>(datafeedStats, datafeedStats.size(), DatafeedConfig.RESULTS_FIELD)));
            return Void.TYPE;
        }).when(client).execute(same(GetDatafeedsStatsAction.INSTANCE), any(), any());
    }

    private static Detector buildMinDetector(String fieldName) {
        Detector.Builder detectorBuilder = new Detector.Builder();
        detectorBuilder.setFunction("min");
        detectorBuilder.setFieldName(fieldName);
        return detectorBuilder.build();
    }

    private static Job buildJob(String jobId, List<Detector> detectors) {
        AnalysisConfig.Builder analysisConfig = new AnalysisConfig.Builder(detectors);
        return new Job.Builder(jobId)
                .setAnalysisConfig(analysisConfig)
                .setDataDescription(new DataDescription.Builder())
                .build(new Date(randomNonNegativeLong()));
    }

    private static GetJobsStatsAction.Response.JobStats buildJobStats(String jobId, JobState state, long modelBytes,
            long numberOfForecasts) {
        ModelSizeStats.Builder modelSizeStats = new ModelSizeStats.Builder(jobId);
        modelSizeStats.setModelBytes(modelBytes);
        GetJobsStatsAction.Response.JobStats jobStats = mock(GetJobsStatsAction.Response.JobStats.class);
        ForecastStats forecastStats = buildForecastStats(numberOfForecasts);

        when(jobStats.getJobId()).thenReturn(jobId);
        when(jobStats.getModelSizeStats()).thenReturn(modelSizeStats.build());
        when(jobStats.getForecastStats()).thenReturn(forecastStats);
        when(jobStats.getState()).thenReturn(state);
        return jobStats;
    }

    private static GetDatafeedsStatsAction.Response.DatafeedStats buildDatafeedStats(DatafeedState state) {
        GetDatafeedsStatsAction.Response.DatafeedStats stats = mock(GetDatafeedsStatsAction.Response.DatafeedStats.class);
        when(stats.getDatafeedState()).thenReturn(state);
        return stats;
    }

    private static ForecastStats buildForecastStats(long numberOfForecasts) {
        return new ForecastStatsTests().createForecastStats(numberOfForecasts, numberOfForecasts);
    }
}
