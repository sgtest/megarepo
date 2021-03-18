/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.ilm;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.Index;
import org.elasticsearch.xpack.core.ilm.step.info.SingleMessageFieldInfo;

import java.time.Clock;
import java.util.Locale;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;

import static org.elasticsearch.xpack.core.ilm.LifecycleExecutionState.fromIndexMetadata;

/**
 * This step wraps an {@link ClusterStateWaitStep} in order to be able to manipulate what the next step will be, depending on the result of
 * the wrapped {@link ClusterStateWaitStep}.
 * <p>
 * If the action response is complete, the {@link ClusterStateWaitUntilThresholdStep}'s nextStepKey will be the nextStepKey of the
 * wrapped action. When the threshold level is surpassed, if the underlying step's condition was not met, the nextStepKey will be changed to
 * the provided {@link #nextKeyOnThresholdBreach} and this step will stop waiting.
 *
 * Failures encountered whilst executing the wrapped action will be propagated directly.
 */
public class ClusterStateWaitUntilThresholdStep extends ClusterStateWaitStep {

    private static final Logger logger = LogManager.getLogger(ClusterStateWaitUntilThresholdStep.class);

    private final ClusterStateWaitStep stepToExecute;
    private final StepKey nextKeyOnThresholdBreach;
    private final AtomicBoolean thresholdPassed = new AtomicBoolean(false);

    public ClusterStateWaitUntilThresholdStep(ClusterStateWaitStep stepToExecute, StepKey nextKeyOnThresholdBreach) {
        super(stepToExecute.getKey(), stepToExecute.getNextStepKey());
        this.stepToExecute = stepToExecute;
        this.nextKeyOnThresholdBreach = nextKeyOnThresholdBreach;
    }

    @Override
    public boolean isRetryable() {
        return true;
    }

    @Override
    public Result isConditionMet(Index index, ClusterState clusterState) {
        IndexMetadata idxMeta = clusterState.metadata().index(index);
        if (idxMeta == null) {
            // Index must have been since deleted, ignore it
            logger.debug("[{}] lifecycle action for index [{}] executed but index no longer exists",
                getKey().getAction(), index.getName());
            return new Result(false, null);
        }

        Result stepResult = stepToExecute.isConditionMet(index, clusterState);

        if (stepResult.isComplete() == false) {
            // checking the threshold after we execute the step to make sure we execute the wrapped step at least once (because time is a
            // wonderful thing)
            TimeValue retryThreshold = LifecycleSettings.LIFECYCLE_STEP_WAIT_TIME_THRESHOLD_SETTING.get(idxMeta.getSettings());
            LifecycleExecutionState lifecycleState = fromIndexMetadata(idxMeta);
            if (waitedMoreThanThresholdLevel(retryThreshold, lifecycleState, Clock.systemUTC())) {
                // we retried this step enough, next step will be the configured to {@code nextKeyOnThresholdBreach}
                thresholdPassed.set(true);

                String message = String.format(Locale.ROOT, "[%s] lifecycle step, as part of [%s] action, for index [%s] executed for" +
                        " more than [%s]. Abandoning execution and moving to the next fallback step [%s]",
                    getKey().getName(), getKey().getAction(), idxMeta.getIndex().getName(), retryThreshold,
                    nextKeyOnThresholdBreach);
                logger.debug(message);

                return new Result(true, new SingleMessageFieldInfo(message));
            }
        }

        return stepResult;
    }

    static boolean waitedMoreThanThresholdLevel(@Nullable TimeValue retryThreshold, LifecycleExecutionState lifecycleState, Clock clock) {
        assert lifecycleState.getStepTime() != null : "lifecycle state [" + lifecycleState + "] does not have the step time set";
        if (retryThreshold != null) {
            // return true if the threshold was surpassed and false otherwise
            return (lifecycleState.getStepTime() + retryThreshold.millis()) < clock.millis();
        }
        return false;
    }

    @Override
    public StepKey getNextStepKey() {
        if (thresholdPassed.get()) {
            return nextKeyOnThresholdBreach;
        } else {
            return super.getNextStepKey();
        }
    }

    /**
     * Represents the {@link ClusterStateWaitStep} that's wrapped by this branching step.
     */
    ClusterStateWaitStep getStepToExecute() {
        return stepToExecute;
    }

    /**
     * The step key to be reported as the {@link ClusterStateWaitUntilThresholdStep#getNextStepKey()} if the index configured a max wait
     * time using {@link LifecycleSettings#LIFECYCLE_STEP_WAIT_TIME_THRESHOLD_SETTING} and the threshold was passed.
     */
    StepKey getNextKeyOnThreshold() {
        return nextKeyOnThresholdBreach;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        if (super.equals(o) == false) {
            return false;
        }
        ClusterStateWaitUntilThresholdStep that = (ClusterStateWaitUntilThresholdStep) o;
        return super.equals(o)
            && Objects.equals(stepToExecute, that.stepToExecute)
            && Objects.equals(nextKeyOnThresholdBreach, that.nextKeyOnThresholdBreach);
    }

    @Override
    public int hashCode() {
        return Objects.hash(super.hashCode(), stepToExecute, nextKeyOnThresholdBreach);
    }
}
