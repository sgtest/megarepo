/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.indexlifecycle;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.common.logging.ESLoggerFactory;
import org.elasticsearch.index.Index;
import org.elasticsearch.xpack.core.indexlifecycle.ClusterStateWaitStep;
import org.elasticsearch.xpack.core.indexlifecycle.InitializePolicyContextStep;
import org.elasticsearch.xpack.core.indexlifecycle.Step;

public class ExecuteStepsUpdateTask extends ClusterStateUpdateTask {
    private static final Logger logger = ESLoggerFactory.getLogger(ExecuteStepsUpdateTask.class);
    private final String policy;
    private final Index index;
    private final Step startStep;
    private final PolicyStepsRegistry policyStepsRegistry;

    public ExecuteStepsUpdateTask(String policy, Index index, Step startStep, PolicyStepsRegistry policyStepsRegistry) {
        this.policy = policy;
        this.index = index;
        this.startStep = startStep;
        this.policyStepsRegistry = policyStepsRegistry;
    }

    String getPolicy() {
        return policy;
    }

    Index getIndex() {
        return index;
    }

    Step getStartStep() {
        return startStep;
    }


    @Override
    public ClusterState execute(ClusterState currentState) {
        Step currentStep = startStep;
        Step registeredCurrentStep = IndexLifecycleRunner.getCurrentStep(policyStepsRegistry, policy,
            currentState.metaData().index(index).getSettings());
        if (currentStep.equals(registeredCurrentStep)) {
            // We can do cluster state steps all together until we
            // either get to a step that isn't a cluster state step or a
            // cluster state wait step returns not completed
            while (currentStep instanceof InitializePolicyContextStep || currentStep instanceof ClusterStateWaitStep) {
                if (currentStep instanceof InitializePolicyContextStep) {
                    // cluster state action step so do the action and
                    // move
                    // the cluster state to the next step
                    currentState = ((InitializePolicyContextStep) currentStep).performAction(index, currentState);
                    if (currentStep.getNextStepKey() == null) {
                        return currentState;
                    }
                    currentState = IndexLifecycleRunner.moveClusterStateToNextStep(index, currentState, currentStep.getNextStepKey());
                } else {
                    // cluster state wait step so evaluate the
                    // condition, if the condition is met move to the
                    // next step, if its not met return the current
                    // cluster state so it can be applied and we will
                    // wait for the next trigger to evaluate the
                    // condition again
                    boolean complete = ((ClusterStateWaitStep) currentStep).isConditionMet(index, currentState);
                    if (complete) {
                        if (currentStep.getNextStepKey() == null) {
                            return currentState;
                        }
                        currentState = IndexLifecycleRunner.moveClusterStateToNextStep(index, currentState, currentStep.getNextStepKey());
                    } else {
                        logger.warn("condition not met, returning existing state");
                        return currentState;
                    }
                }
                currentStep = policyStepsRegistry.getStep(policy, currentStep.getNextStepKey());
            }
            return currentState;
        } else {
            // either we are no longer the master or the step is now
            // not the same as when we submitted the update task. In
            // either case we don't want to do anything now
            return currentState;
        }
    }

    @Override
    public void onFailure(String source, Exception e) {
        throw new RuntimeException(e); // NORELEASE implement error handling
    }
}
