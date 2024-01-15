/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.util.SetOnce;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterStateListener;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.component.Lifecycle;
import org.elasticsearch.common.logging.ESLogMessage;
import org.elasticsearch.common.scheduler.SchedulerEngine;
import org.elasticsearch.common.scheduler.TimeValueSchedule;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.RunOnce;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.gateway.GatewayService;
import org.elasticsearch.health.node.selection.HealthNode;
import org.elasticsearch.telemetry.TelemetryProvider;
import org.elasticsearch.telemetry.metric.LongGaugeMetric;
import org.elasticsearch.telemetry.metric.MeterRegistry;

import java.io.Closeable;
import java.time.Clock;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

import static org.elasticsearch.health.HealthStatus.GREEN;
import static org.elasticsearch.health.HealthStatus.RED;

/**
 * This class periodically logs the results of the Health API to the standard Elasticsearch server log file.
 */
public class HealthPeriodicLogger implements ClusterStateListener, Closeable, SchedulerEngine.Listener {
    public static final String HEALTH_FIELD_PREFIX = "elasticsearch.health";
    public static final String MESSAGE_FIELD = "message";

    /**
     * Valid modes of output for this logger
     */
    public enum OutputMode {
        LOGS("logs"),
        METRICS("metrics");

        private final String mode;

        OutputMode(String mode) {
            this.mode = mode;
        }

        public static OutputMode fromString(String mode) {
            return valueOf(mode.toUpperCase(Locale.ROOT));
        }

        @Override
        public String toString() {
            return this.mode.toLowerCase(Locale.ROOT);
        }

        static OutputMode parseOutputMode(String value) {
            try {
                return OutputMode.fromString(value);
            } catch (Exception e) {
                throw new IllegalArgumentException("Illegal OutputMode:" + value);
            }
        }
    }

    public static final Setting<TimeValue> POLL_INTERVAL_SETTING = Setting.timeSetting(
        "health.periodic_logger.poll_interval",
        TimeValue.timeValueSeconds(60),
        TimeValue.timeValueSeconds(15),
        Setting.Property.Dynamic,
        Setting.Property.NodeScope
    );

    public static final Setting<Boolean> ENABLED_SETTING = Setting.boolSetting(
        "health.periodic_logger.enabled",
        false,
        Setting.Property.Dynamic,
        Setting.Property.NodeScope
    );

    public static final Setting<List<OutputMode>> OUTPUT_MODE_SETTING = Setting.listSetting(
        "health.periodic_logger.output_mode",
        List.of(OutputMode.LOGS.toString(), OutputMode.METRICS.toString()),
        OutputMode::parseOutputMode,
        Setting.Property.Dynamic,
        Setting.Property.NodeScope
    );

    /**
     * Name constant for the job HealthService schedules
     */
    protected static final String HEALTH_PERIODIC_LOGGER_JOB_NAME = "health_periodic_logger";

    private final Settings settings;

    private final ClusterService clusterService;
    private final Client client;

    private final HealthService healthService;
    private final Clock clock;

    // default visibility for testing purposes
    volatile boolean isHealthNode = false;

    private final AtomicBoolean currentlyRunning = new AtomicBoolean(false);

    private final SetOnce<SchedulerEngine> scheduler = new SetOnce<>();
    private volatile TimeValue pollInterval;
    private volatile boolean enabled;
    private volatile Set<OutputMode> outputModes;

    private static final Logger logger = LogManager.getLogger(HealthPeriodicLogger.class);

    private final MeterRegistry meterRegistry;
    private final Map<String, LongGaugeMetric> redMetrics = new HashMap<>();

    // Writers for logs or messages
    // default visibility for testing purposes
    private final BiConsumer<LongGaugeMetric, Long> metricWriter;
    private final Consumer<ESLogMessage> logWriter;

    /**
     * Creates a new HealthPeriodicLogger.
     * This creates a scheduled job using the SchedulerEngine framework and runs it on the current health node.
     *
     * @param settings the cluster settings, used to get the interval setting.
     * @param clusterService the cluster service, used to know when the health node changes.
     * @param client the client used to call the Health Service.
     * @param healthService the Health Service, where the actual Health API logic lives.
     * @param telemetryProvider used to get the meter registry for metrics
     */
    public static HealthPeriodicLogger create(
        Settings settings,
        ClusterService clusterService,
        Client client,
        HealthService healthService,
        TelemetryProvider telemetryProvider
    ) {
        return HealthPeriodicLogger.create(settings, clusterService, client, healthService, telemetryProvider, null, null);
    }

    static HealthPeriodicLogger create(
        Settings settings,
        ClusterService clusterService,
        Client client,
        HealthService healthService,
        TelemetryProvider telemetryProvider,
        BiConsumer<LongGaugeMetric, Long> metricWriter,
        Consumer<ESLogMessage> logWriter
    ) {
        HealthPeriodicLogger logger = new HealthPeriodicLogger(
            settings,
            clusterService,
            client,
            healthService,
            telemetryProvider.getMeterRegistry(),
            metricWriter,
            logWriter
        );
        logger.registerListeners();
        return logger;
    }

    private HealthPeriodicLogger(
        Settings settings,
        ClusterService clusterService,
        Client client,
        HealthService healthService,
        MeterRegistry meterRegistry,
        BiConsumer<LongGaugeMetric, Long> metricWriter,
        Consumer<ESLogMessage> logWriter
    ) {
        this.settings = settings;
        this.clusterService = clusterService;
        this.client = client;
        this.healthService = healthService;
        this.clock = Clock.systemUTC();
        this.pollInterval = POLL_INTERVAL_SETTING.get(settings);
        this.enabled = ENABLED_SETTING.get(settings);
        this.outputModes = new HashSet<>(OUTPUT_MODE_SETTING.get(settings));
        this.meterRegistry = meterRegistry;
        this.metricWriter = metricWriter == null ? LongGaugeMetric::set : metricWriter;
        this.logWriter = logWriter == null ? logger::info : logWriter;

        // create metric for overall level metrics
        this.redMetrics.put(
            "overall",
            LongGaugeMetric.create(this.meterRegistry, "es.health.overall.red.status", "Overall: Red", "{cluster}")
        );
    }

    private void registerListeners() {
        if (enabled) {
            clusterService.addListener(this);
        }
        clusterService.getClusterSettings().addSettingsUpdateConsumer(ENABLED_SETTING, this::enable);
        clusterService.getClusterSettings().addSettingsUpdateConsumer(POLL_INTERVAL_SETTING, this::updatePollInterval);
        clusterService.getClusterSettings().addSettingsUpdateConsumer(OUTPUT_MODE_SETTING, this::updateOutputModes);
    }

    @Override
    public void clusterChanged(ClusterChangedEvent event) {
        // wait for the cluster state to be recovered
        if (event.state().blocks().hasGlobalBlock(GatewayService.STATE_NOT_RECOVERED_BLOCK)) {
            return;
        }

        DiscoveryNode healthNode = HealthNode.findHealthNode(event.state());
        if (healthNode == null) {
            this.isHealthNode = false;
            this.maybeCancelJob();
            return;
        }
        final boolean isCurrentlyHealthNode = healthNode.getId().equals(this.clusterService.localNode().getId());
        if (this.isHealthNode != isCurrentlyHealthNode) {
            this.isHealthNode = isCurrentlyHealthNode;
            if (this.isHealthNode) {
                // we weren't the health node, and now we are
                maybeScheduleJob();
            } else {
                // we were the health node, and now we aren't
                maybeCancelJob();
            }
        }
    }

    @Override
    public void close() {
        SchedulerEngine engine = scheduler.get();
        if (engine != null) {
            engine.stop();
        }
    }

    @Override
    public void triggered(SchedulerEngine.Event event) {
        if (event.getJobName().equals(HEALTH_PERIODIC_LOGGER_JOB_NAME) && this.enabled) {
            this.tryToLogHealth();
        }
    }

    // default visibility for testing purposes
    void tryToLogHealth() {
        if (this.currentlyRunning.compareAndExchange(false, true) == false) {
            RunOnce release = new RunOnce(() -> currentlyRunning.set(false));
            try {
                ActionListener<List<HealthIndicatorResult>> listenerWithRelease = ActionListener.runAfter(resultsListener, release);
                this.healthService.getHealth(this.client, null, false, 0, listenerWithRelease);
            } catch (Exception e) {
                logger.warn(() -> "The health periodic logger encountered an error.", e);
                // In case of an exception before the listener was wired, we can release the flag here, and we feel safe
                // that it will not release it again because this can only be run once.
                release.run();
            }
        }
    }

    // default visibility for testing purposes
    SchedulerEngine getScheduler() {
        return this.scheduler.get();
    }

    /**
     * Create a Map of the results, which is then turned into JSON for logging.
     * The structure looks like:
     * {"elasticsearch.health.overall.status": "green", "elasticsearch.health.[other indicators].status": "green"}
     * Only the indicator status values are included, along with the computed top-level status.
     *
     * @param indicatorResults the results of the Health API call that will be used as the output.
     */
    // default visibility for testing purposes
    static Map<String, Object> convertToLoggedFields(List<HealthIndicatorResult> indicatorResults) {
        if (indicatorResults == null || indicatorResults.isEmpty()) {
            return Map.of();
        }

        final Map<String, Object> result = new HashMap<>();

        // overall status
        final HealthStatus status = calculateOverallStatus(indicatorResults);
        result.put(String.format(Locale.ROOT, "%s.overall.status", HEALTH_FIELD_PREFIX), status.xContentValue());

        // top-level status for each indicator
        indicatorResults.forEach((indicatorResult) -> {
            result.put(
                String.format(Locale.ROOT, "%s.%s.status", HEALTH_FIELD_PREFIX, indicatorResult.name()),
                indicatorResult.status().xContentValue()
            );
        });

        // message field. Show the non-green indicators if they exist.
        List<String> nonGreen = indicatorResults.stream()
            .filter(p -> p.status() != GREEN)
            .map(HealthIndicatorResult::name)
            .sorted()
            .toList();
        if (nonGreen.isEmpty()) {
            result.put(MESSAGE_FIELD, String.format(Locale.ROOT, "health=%s", status.xContentValue()));
        } else {
            result.put(MESSAGE_FIELD, String.format(Locale.ROOT, "health=%s [%s]", status.xContentValue(), String.join(",", nonGreen)));
        }

        return result;
    }

    static HealthStatus calculateOverallStatus(List<HealthIndicatorResult> indicatorResults) {
        return HealthStatus.merge(indicatorResults.stream().map(HealthIndicatorResult::status));
    }

    /**
     * Handle the result of the Health Service getHealth call
     */
    // default visibility for testing purposes
    final ActionListener<List<HealthIndicatorResult>> resultsListener = new ActionListener<List<HealthIndicatorResult>>() {
        @Override
        public void onResponse(List<HealthIndicatorResult> healthIndicatorResults) {
            try {
                if (logsEnabled()) {
                    Map<String, Object> resultsMap = convertToLoggedFields(healthIndicatorResults);

                    // if we have a valid response, log in JSON format
                    if (resultsMap.isEmpty() == false) {
                        ESLogMessage msg = new ESLogMessage().withFields(resultsMap);
                        logWriter.accept(msg);
                    }
                }

                // handle metrics
                if (metricsEnabled()) {
                    writeMetrics(healthIndicatorResults);
                }

            } catch (Exception e) {
                logger.warn("Health Periodic Logger error:{}", e.toString());
            }
        }

        @Override
        public void onFailure(Exception e) {
            logger.warn("Health Periodic Logger error:{}", e.toString());
        }
    };

    /**
     * Write (and possibly create) the APM metrics
     */
    // default visibility for testing purposes
    void writeMetrics(List<HealthIndicatorResult> healthIndicatorResults) {
        if (healthIndicatorResults != null) {
            for (HealthIndicatorResult result : healthIndicatorResults) {
                String metricName = result.name();
                LongGaugeMetric metric = this.redMetrics.get(metricName);
                if (metric == null) {
                    metric = LongGaugeMetric.create(
                        this.meterRegistry,
                        String.format(Locale.ROOT, "es.health.%s.red.status", metricName),
                        String.format(Locale.ROOT, "%s: Red", metricName),
                        "{cluster}"
                    );
                    this.redMetrics.put(metricName, metric);
                }
                metricWriter.accept(metric, result.status() == RED ? 1L : 0L);
            }

            metricWriter.accept(this.redMetrics.get("overall"), calculateOverallStatus(healthIndicatorResults) == RED ? 1L : 0L);
        }
    }

    private void updateOutputModes(List<OutputMode> newMode) {
        this.outputModes = new HashSet<>(newMode);
    }

    /**
     * Returns true if any of the outputModes are set to logs
     */
    private boolean logsEnabled() {
        return this.outputModes.contains(OutputMode.LOGS);
    }

    /**
     * Returns true if any of the outputModes are set to metrics
     */
    private boolean metricsEnabled() {
        return this.outputModes.contains(OutputMode.METRICS);
    }

    /**
     * Create the SchedulerEngine.Job if this node is the health node
     */
    private void maybeScheduleJob() {
        if (this.isHealthNode == false) {
            return;
        }

        if (this.enabled == false) {
            return;
        }

        // don't schedule the job if the node is shutting down
        if (isClusterServiceStoppedOrClosed()) {
            logger.trace(
                "Skipping scheduling a health periodic logger job due to the cluster lifecycle state being: [{}] ",
                clusterService.lifecycleState()
            );
            return;
        }

        if (scheduler.get() == null) {
            scheduler.set(new SchedulerEngine(settings, clock));
            scheduler.get().register(this);
        }

        assert scheduler.get() != null : "scheduler should be available";
        final SchedulerEngine.Job scheduledJob = new SchedulerEngine.Job(
            HEALTH_PERIODIC_LOGGER_JOB_NAME,
            new TimeValueSchedule(pollInterval)
        );
        scheduler.get().add(scheduledJob);
    }

    private void maybeCancelJob() {
        if (scheduler.get() != null) {
            scheduler.get().remove(HEALTH_PERIODIC_LOGGER_JOB_NAME);
        }
    }

    private void enable(boolean enabled) {
        this.enabled = enabled;
        if (enabled) {
            clusterService.addListener(this);
            maybeScheduleJob();
        } else {
            clusterService.removeListener(this);
            maybeCancelJob();
        }
    }

    private void updatePollInterval(TimeValue newInterval) {
        this.pollInterval = newInterval;
        maybeScheduleJob();
    }

    private boolean isClusterServiceStoppedOrClosed() {
        final Lifecycle.State state = clusterService.lifecycleState();
        return state == Lifecycle.State.STOPPED || state == Lifecycle.State.CLOSED;
    }
}
