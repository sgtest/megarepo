/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ilm;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.rollup.RollupV2;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.stream.Collectors;

import static java.util.stream.Collectors.toList;

/**
 * Represents the lifecycle of an index from creation to deletion. A
 * {@link TimeseriesLifecycleType} is made up of a set of {@link Phase}s which it will
 * move through. Soon we will constrain the phases using some kinda of lifecycle
 * type which will allow only particular {@link Phase}s to be defined, will
 * dictate the order in which the {@link Phase}s are executed and will define
 * which {@link LifecycleAction}s are allowed in each phase.
 */
public class TimeseriesLifecycleType implements LifecycleType {
    public static final TimeseriesLifecycleType INSTANCE = new TimeseriesLifecycleType();

    public static final String TYPE = "timeseries";
    static final String HOT_PHASE = "hot";
    static final String WARM_PHASE = "warm";
    static final String COLD_PHASE = "cold";
    static final String DELETE_PHASE = "delete";
    static final List<String> VALID_PHASES = Arrays.asList(HOT_PHASE, WARM_PHASE, COLD_PHASE, DELETE_PHASE);
    static final List<String> ORDERED_VALID_HOT_ACTIONS;
    static final List<String> ORDERED_VALID_WARM_ACTIONS = Arrays.asList(SetPriorityAction.NAME, UnfollowAction.NAME, ReadOnlyAction.NAME,
        AllocateAction.NAME, MigrateAction.NAME, ShrinkAction.NAME, ForceMergeAction.NAME);
    static final List<String> ORDERED_VALID_COLD_ACTIONS;
    static final List<String> ORDERED_VALID_DELETE_ACTIONS = Arrays.asList(WaitForSnapshotAction.NAME, DeleteAction.NAME);
    static final Set<String> VALID_HOT_ACTIONS;
    static final Set<String> VALID_WARM_ACTIONS = Sets.newHashSet(ORDERED_VALID_WARM_ACTIONS);
    static final Set<String> VALID_COLD_ACTIONS;
    static final Set<String> VALID_DELETE_ACTIONS = Sets.newHashSet(ORDERED_VALID_DELETE_ACTIONS);
    private static final Map<String, Set<String>> ALLOWED_ACTIONS;

    static final Set<String> HOT_ACTIONS_THAT_REQUIRE_ROLLOVER = Sets.newHashSet(ReadOnlyAction.NAME, ShrinkAction.NAME,
        ForceMergeAction.NAME, RollupILMAction.NAME, SearchableSnapshotAction.NAME);
    // a set of actions that cannot be defined (executed) after the managed index has been mounted as searchable snapshot
    static final Set<String> ACTIONS_CANNOT_FOLLOW_SEARCHABLE_SNAPSHOT = Sets.newHashSet(ShrinkAction.NAME, ForceMergeAction.NAME,
        FreezeAction.NAME, SearchableSnapshotAction.NAME, RollupILMAction.NAME);

    static {
        if (RollupV2.isEnabled()) {
            ORDERED_VALID_HOT_ACTIONS = Arrays.asList(SetPriorityAction.NAME, UnfollowAction.NAME, RolloverAction.NAME,
                ReadOnlyAction.NAME, RollupILMAction.NAME, ShrinkAction.NAME, ForceMergeAction.NAME, SearchableSnapshotAction.NAME);
            ORDERED_VALID_COLD_ACTIONS = Arrays.asList(SetPriorityAction.NAME, UnfollowAction.NAME, AllocateAction.NAME,
                MigrateAction.NAME, FreezeAction.NAME, RollupILMAction.NAME, SearchableSnapshotAction.NAME);
        } else {
            ORDERED_VALID_HOT_ACTIONS = Arrays.asList(SetPriorityAction.NAME, UnfollowAction.NAME, RolloverAction.NAME,
                ReadOnlyAction.NAME, ShrinkAction.NAME, ForceMergeAction.NAME, SearchableSnapshotAction.NAME);
            ORDERED_VALID_COLD_ACTIONS = Arrays.asList(SetPriorityAction.NAME, UnfollowAction.NAME, AllocateAction.NAME,
                MigrateAction.NAME, FreezeAction.NAME, SearchableSnapshotAction.NAME);
        }
        VALID_HOT_ACTIONS = Sets.newHashSet(ORDERED_VALID_HOT_ACTIONS);
        VALID_COLD_ACTIONS = Sets.newHashSet(ORDERED_VALID_COLD_ACTIONS);
        ALLOWED_ACTIONS = new HashMap<>();
        ALLOWED_ACTIONS.put(HOT_PHASE, VALID_HOT_ACTIONS);
        ALLOWED_ACTIONS.put(WARM_PHASE, VALID_WARM_ACTIONS);
        ALLOWED_ACTIONS.put(COLD_PHASE, VALID_COLD_ACTIONS);
        ALLOWED_ACTIONS.put(DELETE_PHASE, VALID_DELETE_ACTIONS);
    }

    private TimeseriesLifecycleType() {
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
    }

    @Override
    public String getWriteableName() {
        return TYPE;
    }

    public List<Phase> getOrderedPhases(Map<String, Phase> phases) {
        List<Phase> orderedPhases = new ArrayList<>(VALID_PHASES.size());
        for (String phaseName : VALID_PHASES) {
            Phase phase = phases.get(phaseName);
            if (phase != null) {
                Map<String, LifecycleAction> actions = phase.getActions();
                if (actions.containsKey(UnfollowAction.NAME) == false &&
                    (actions.containsKey(RolloverAction.NAME) || actions.containsKey(ShrinkAction.NAME) ||
                        actions.containsKey(SearchableSnapshotAction.NAME))) {
                    Map<String, LifecycleAction> actionMap = new HashMap<>(phase.getActions());
                    actionMap.put(UnfollowAction.NAME, new UnfollowAction());
                    phase = new Phase(phase.getName(), phase.getMinimumAge(), actionMap);
                }

                if (shouldInjectMigrateStepForPhase(phase)) {
                    Map<String, LifecycleAction> actionMap = new HashMap<>(phase.getActions());
                    actionMap.put(MigrateAction.NAME, new MigrateAction(true));
                    phase = new Phase(phase.getName(), phase.getMinimumAge(), actionMap);
                }

                orderedPhases.add(phase);
            }
        }
        return orderedPhases;
    }

    static boolean shouldInjectMigrateStepForPhase(Phase phase) {
        AllocateAction allocateAction = (AllocateAction) phase.getActions().get(AllocateAction.NAME);
        if (allocateAction != null) {
            if (definesAllocationRules(allocateAction)) {
                // we won't automatically migrate the data if an allocate action that defines any allocation rule is present
                return false;
            }
        }

        MigrateAction migrateAction = (MigrateAction) phase.getActions().get(MigrateAction.NAME);
        // if the user configured the {@link MigrateAction} already we won't automatically configure it
        return migrateAction == null;
    }

    @Override
    public String getNextPhaseName(String currentPhaseName, Map<String, Phase> phases) {
        int index = VALID_PHASES.indexOf(currentPhaseName);
        if (index < 0 && "new".equals(currentPhaseName) == false) {
            throw new IllegalArgumentException("[" + currentPhaseName + "] is not a valid phase for lifecycle type [" + TYPE + "]");
        } else {
            // Find the next phase after `index` that exists in `phases` and return it
            while (++index < VALID_PHASES.size()) {
                String phaseName = VALID_PHASES.get(index);
                if (phases.containsKey(phaseName)) {
                    return phaseName;
                }
            }
            // if we have exhausted VALID_PHASES and haven't found a matching
            // phase in `phases` return null indicating there is no next phase
            // available
            return null;
        }
    }

    public String getPreviousPhaseName(String currentPhaseName, Map<String, Phase> phases) {
        if ("new".equals(currentPhaseName)) {
            return null;
        }
        int index = VALID_PHASES.indexOf(currentPhaseName);
        if (index < 0) {
            throw new IllegalArgumentException("[" + currentPhaseName + "] is not a valid phase for lifecycle type [" + TYPE + "]");
        } else {
            // Find the previous phase before `index` that exists in `phases` and return it
            while (--index >= 0) {
                String phaseName = VALID_PHASES.get(index);
                if (phases.containsKey(phaseName)) {
                    return phaseName;
                }
            }
            // if we have exhausted VALID_PHASES and haven't found a matching
            // phase in `phases` return null indicating there is no previous phase
            // available
            return null;
        }
    }

    public List<LifecycleAction> getOrderedActions(Phase phase) {
        Map<String, LifecycleAction> actions = phase.getActions();
        switch (phase.getName()) {
            case HOT_PHASE:
                return ORDERED_VALID_HOT_ACTIONS.stream().map(actions::get)
                    .filter(Objects::nonNull).collect(toList());
            case WARM_PHASE:
                return ORDERED_VALID_WARM_ACTIONS.stream().map(actions::get)
                    .filter(Objects::nonNull).collect(toList());
            case COLD_PHASE:
                return ORDERED_VALID_COLD_ACTIONS.stream().map(actions::get)
                    .filter(Objects::nonNull).collect(toList());
            case DELETE_PHASE:
                return ORDERED_VALID_DELETE_ACTIONS.stream().map(actions::get)
                    .filter(Objects::nonNull).collect(toList());
            default:
                throw new IllegalArgumentException("lifecycle type[" + TYPE + "] does not support phase[" + phase.getName() + "]");
        }
    }

    @Override
    public String getNextActionName(String currentActionName, Phase phase) {
        List<String> orderedActionNames;
        switch (phase.getName()) {
            case HOT_PHASE:
                orderedActionNames = ORDERED_VALID_HOT_ACTIONS;
                break;
            case WARM_PHASE:
                orderedActionNames = ORDERED_VALID_WARM_ACTIONS;
                break;
            case COLD_PHASE:
                orderedActionNames = ORDERED_VALID_COLD_ACTIONS;
                break;
            case DELETE_PHASE:
                orderedActionNames = ORDERED_VALID_DELETE_ACTIONS;
                break;
            default:
                throw new IllegalArgumentException("lifecycle type [" + TYPE + "] does not support phase [" + phase.getName() + "]");
        }

        int index = orderedActionNames.indexOf(currentActionName);
        if (index < 0) {
            throw new IllegalArgumentException("[" + currentActionName + "] is not a valid action for phase [" + phase.getName()
                + "] in lifecycle type [" + TYPE + "]");
        } else {
            // Find the next action after `index` that exists in the phase and return it
            while (++index < orderedActionNames.size()) {
                String actionName = orderedActionNames.get(index);
                if (phase.getActions().containsKey(actionName)) {
                    return actionName;
                }
            }
            // if we have exhausted `validActions` and haven't found a matching
            // action in the Phase return null indicating there is no next
            // action available
            return null;
        }
    }

    @Override
    public void validate(Collection<Phase> phases) {
        phases.forEach(phase -> {
            if (ALLOWED_ACTIONS.containsKey(phase.getName()) == false) {
                throw new IllegalArgumentException("Timeseries lifecycle does not support phase [" + phase.getName() + "]");
            }
            phase.getActions().forEach((actionName, action) -> {
                if (ALLOWED_ACTIONS.get(phase.getName()).contains(actionName) == false) {
                    throw new IllegalArgumentException("invalid action [" + actionName + "] " +
                        "defined in phase [" + phase.getName() + "]");
                }
            });
        });

        // Check for actions in the hot phase that require a rollover
        String invalidHotPhaseActions = phases.stream()
            // Is there a hot phase
            .filter(phase -> HOT_PHASE.equals(phase.getName()))
            // that does *not* contain the 'rollover' action
            .filter(phase -> phase.getActions().containsKey(RolloverAction.NAME) == false)
            // but that does have actions that require a rollover action?
            .flatMap(phase -> Sets.intersection(phase.getActions().keySet(), HOT_ACTIONS_THAT_REQUIRE_ROLLOVER).stream())
            .collect(Collectors.joining(", "));
        if (Strings.hasText(invalidHotPhaseActions)) {
            throw new IllegalArgumentException("the [" + invalidHotPhaseActions +
                "] action(s) may not be used in the [" + HOT_PHASE +
                "] phase without an accompanying [" + RolloverAction.NAME + "] action");
        }

        // look for phases that have the migrate action enabled and also specify allocation rules via the AllocateAction
        String phasesWithConflictingMigrationActions = phases.stream()
            .filter(phase -> phase.getActions().containsKey(MigrateAction.NAME) &&
                ((MigrateAction) phase.getActions().get(MigrateAction.NAME)).isEnabled() &&
                phase.getActions().containsKey(AllocateAction.NAME) &&
                definesAllocationRules((AllocateAction) phase.getActions().get(AllocateAction.NAME))
            )
            .map(Phase::getName)
            .collect(Collectors.joining(","));
        if (Strings.hasText(phasesWithConflictingMigrationActions)) {
            throw new IllegalArgumentException("phases [" + phasesWithConflictingMigrationActions + "] specify an enabled " +
                MigrateAction.NAME + " action and an " + AllocateAction.NAME + " action with allocation rules. specify only a single " +
                "data migration in each phase");
        }

        validateActionsFollowingSearchableSnapshot(phases);
    }

    static void validateActionsFollowingSearchableSnapshot(Collection<Phase> phases) {
        boolean hotPhaseContainsSearchableSnapshot = phases.stream()
            .filter(phase -> HOT_PHASE.equals(phase.getName()))
            .anyMatch(phase -> phase.getActions().containsKey(SearchableSnapshotAction.NAME));
        if (hotPhaseContainsSearchableSnapshot) {
            String phasesDefiningIllegalActions = phases.stream()
                // we're looking for prohibited actions in phases other than hot
                .filter(phase -> HOT_PHASE.equals(phase.getName()) == false)
                // filter the phases that define illegal actions
                .filter(phase ->
                    Collections.disjoint(ACTIONS_CANNOT_FOLLOW_SEARCHABLE_SNAPSHOT, phase.getActions().keySet()) == false)
                .map(Phase::getName)
                .collect(Collectors.joining(","));
            if (Strings.hasText(phasesDefiningIllegalActions)) {
                throw new IllegalArgumentException("phases [" + phasesDefiningIllegalActions + "] define one or more of " +
                    ACTIONS_CANNOT_FOLLOW_SEARCHABLE_SNAPSHOT + " actions which are not allowed after a " +
                    "managed index is mounted as a searchable snapshot");
            }
        }
    }

    private static boolean definesAllocationRules(AllocateAction action) {
        return action.getRequire().isEmpty() == false || action.getInclude().isEmpty() == false || action.getExclude().isEmpty() == false;
    }
}
