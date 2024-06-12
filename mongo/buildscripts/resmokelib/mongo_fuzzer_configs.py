"""Generator functions for all parameters that we fuzz when invoked with --fuzzMongodConfigs."""

import random
from buildscripts.resmokelib import utils


def generate_normal_wt_parameters(rng, value):
    """Returns the value assigned the WiredTiger parameters (both eviction or table) based on the fields of the parameters in the config_fuzzer_wt_limits.py."""

    if "choices" in value:
        ret = rng.choice(value["choices"])
        if "multiplier" in value:
            ret *= value["multiplier"]
    elif "min" in value and "max" in value:
        ret = rng.randint(value["min"], value["max"])
    return ret


def generate_special_eviction_configs(rng, ret, fuzzer_stress_mode, params):
    """Returns the value assigned the WiredTiger eviction parameters based on the fields of the parameters in config_fuzzer_wt_limits.py for special parameters (parameters with different assignment behaviors)."""
    from buildscripts.resmokelib.config_fuzzer_wt_limits import target_bytes_max

    # eviction_trigger is relative to eviction_target, so you have to leave them excluded to ensure
    # eviction_trigger is fuzzed first.
    ret["eviction_target"] = rng.randint(
        params["eviction_target"]["min"], params["eviction_target"]["max"]
    )
    ret["eviction_trigger"] = rng.randint(
        ret["eviction_target"] + params["eviction_trigger"]["min"],
        params["eviction_trigger"]["max"],
    )

    # Fuzz eviction_dirty_target and trigger both as relative and absolute values.
    ret["eviction_dirty_target"] = rng.choice(
        [
            rng.randint(
                params["eviction_dirty_target_1"]["min"], params["eviction_dirty_target_1"]["max"]
            ),
            rng.randint(
                params["eviction_dirty_target_2"]["min"], params["eviction_dirty_target_2"]["max"]
            ),
        ]
    )
    ret["trigger_max"] = 75 if ret["eviction_dirty_target"] <= 50 else target_bytes_max
    ret["eviction_dirty_trigger"] = rng.randint(
        ret["eviction_dirty_target"] + 1, ret["trigger_max"]
    )

    assert ret["eviction_dirty_trigger"] > ret["eviction_dirty_target"]
    assert ret["eviction_dirty_trigger"] <= ret["trigger_max"]

    # Fuzz eviction_updates_target and eviction_updates_trigger. These are by default half the
    # values of the corresponding eviction dirty target and trigger. They need to stay less than the
    # dirty equivalents. The default updates target is 2.5% of the cache, so let's start fuzzing
    # from 2%.
    ret["updates_target_min"] = (
        2 if ret["eviction_dirty_target"] <= 100 else 20 * 1024 * 1024
    )  # 2% of 1GB cache
    ret["eviction_updates_target"] = rng.randint(
        ret["updates_target_min"], ret["eviction_dirty_target"] - 1
    )
    ret["eviction_updates_trigger"] = rng.randint(
        ret["eviction_updates_target"] + 1, ret["eviction_dirty_trigger"] - 1
    )

    # dbg_rollback_error rolls back every Nth transaction.
    # The values have been tuned after looking at how many WiredTiger transactions happen per second for the config-fuzzed jstests.
    # The setting is triggering bugs, disabled until they get resolved.
    ret["dbg_rollback_error"] = 0
    # choices = params["dbg_rollback_error"]["choices"]
    # choices.append(rng.randint(params["dbg_rollback_error"]["lower_bound"], params["dbg_rollback_error"]["upper_bound"]))
    # ret["dbg_rollback_error"] = rng.choice(choices)

    ret["dbg_slow_checkpoint"] = (
        "false"
        if fuzzer_stress_mode != "stress"
        else rng.choice(params["dbg_slow_checkpoint"]["choices"])
    )
    return ret


def generate_eviction_configs(rng, fuzzer_stress_mode):
    """Returns a string with random configurations for wiredTigerEngineConfigString parameter."""
    from buildscripts.resmokelib.config_fuzzer_wt_limits import config_fuzzer_params

    params = config_fuzzer_params["wt"]

    ret = {}
    excluded_normal_params = [
        "dbg_rollback_error",
        "dbg_slow_checkpoint",
        "eviction_dirty_target",
        "eviction_dirty_trigger",
        "eviction_target",
        "eviction_trigger",
        "eviction_updates_target",
        "eviction_updates_trigger",
        "trigger_max",
        "updates_target_min",
    ]

    ret = generate_special_eviction_configs(rng, ret, fuzzer_stress_mode, params)
    ret.update(
        {
            key: generate_normal_wt_parameters(rng, value)
            for key, value in params.items()
            if key not in excluded_normal_params
        }
    )

    return (
        "debug_mode=(eviction={0},realloc_exact={1},rollback_error={2}, slow_checkpoint={3}),"
        "eviction_checkpoint_target={4},eviction_dirty_target={5},eviction_dirty_trigger={6},"
        "eviction_target={7},eviction_trigger={8},eviction_updates_target={9},"
        "eviction_updates_trigger={10},file_manager=(close_handle_minimum={11},"
        "close_idle_time={12},close_scan_interval={13})".format(
            ret["dbg_eviction"],
            ret["dbg_realloc_exact"],
            ret["dbg_rollback_error"],
            ret["dbg_slow_checkpoint"],
            ret["eviction_checkpoint_target"],
            ret["eviction_dirty_target"],
            ret["eviction_dirty_trigger"],
            ret["eviction_target"],
            ret["eviction_trigger"],
            ret["eviction_updates_target"],
            ret["eviction_updates_trigger"],
            ret["close_handle_minimum"],
            ret["close_idle_time_secs"],
            ret["close_scan_interval"],
        )
    )


def generate_special_table_configs(rng, ret, params):
    """Returns the value assigned the WiredTiger table parameters based on the fields of the parameters in config_fuzzer_wt_limits.py for special parameters (parameters with different assignment behaviors)."""

    ret["memory_page_max_lower_bound"] = ret["leaf_page_max"]
    # Assume WT cache size of 1GB as most MDB tests specify this as the cache size.
    ret["memory_page_max_upper_bound"] = round(
        (
            rng.randint(
                params["memory_page_max_upper_bound"]["min"],
                params["memory_page_max_upper_bound"]["max"],
            )
            * 1024
            * 1024
        )
        / 10
    )  # cache_size / 10
    ret["memory_page_max"] = rng.randint(
        ret["memory_page_max_lower_bound"], ret["memory_page_max_upper_bound"]
    )
    return ret


def generate_table_configs(rng):
    """Returns a string with random configurations for WiredTiger tables."""
    from buildscripts.resmokelib.config_fuzzer_wt_limits import config_fuzzer_params

    params = config_fuzzer_params["wt_table"]

    ret = {}
    # excluded_normal_params are a list of params that we want to exclude from the for-loop because they have some different assignment behavior
    # e.g. depending on other parameters' values, having rounding, having a different distribution.
    excluded_normal_params = [
        "memory_page_max_lower_bound",
        "memory_page_max_upper_bound",
        "memory_page_max",
    ]

    ret.update(
        {
            key: generate_normal_wt_parameters(rng, value)
            for key, value in params.items()
            if key not in excluded_normal_params
        }
    )
    ret = generate_special_table_configs(rng, ret, params)

    return (
        "block_compressor={0},internal_page_max={1},leaf_page_max={2},leaf_value_max={3},"
        "memory_page_max={4},prefix_compression={5},split_pct={6}".format(
            ret["block_compressor"],
            ret["internal_page_max"],
            ret["leaf_page_max"],
            ret["leaf_value_max"],
            ret["memory_page_max"],
            ret["prefix_compression"],
            ret["split_pct"],
        )
    )


def generate_normal_mongo_parameters(rng, value):
    """Returns the value assigned the mongod or mongos parameter based on the fields of the parameters in the config_fuzzer_limits.py."""

    if "isUniform" in value:
        ret = rng.uniform(value["min"], value["max"])
    elif "isRandomizedChoice" in value:
        choices = value["choices"]
        choices.append(rng.randint(value["lower_bound"], value["upper_bound"]))
        ret = rng.choice(choices)
    elif "choices" in value:
        ret = rng.choice(value["choices"])
    elif "min" and "max" in value:
        ret = rng.randint(value["min"], value["max"])
    elif "default" in value:
        ret = value["default"]
    return ret


def generate_special_mongod_parameters(rng, ret, fuzzer_stress_mode, params):
    """Returns the value assigned the mongod parameter based on the fields of the parameters in config_fuzzer_limits.py for special parameters (parameters with different assignment behaviors)."""

    # We assign "throughputProbing[...]" first because we want to ensure that ret["throughputProbingInitialConcurrency"] has a value before we assign the value of
    # ret["throughputProbingMinConcurrency"] and ret["throughputProbingMaxConcurrency"]
    ret["throughputProbingInitialConcurrency"] = rng.randint(
        params["throughputProbingInitialConcurrency"]["min"],
        params["throughputProbingInitialConcurrency"]["max"],
    )
    ret["throughputProbingMinConcurrency"] = rng.randint(
        params["throughputProbingMinConcurrency"]["min"], ret["throughputProbingInitialConcurrency"]
    )
    ret["throughputProbingMaxConcurrency"] = rng.randint(
        ret["throughputProbingInitialConcurrency"], params["throughputProbingMaxConcurrency"]["max"]
    )

    # "mirrorReads" and "throughputProbingConcurrencyMovingAverageWeight" both are the only parameters that use rng.random().
    ret["mirrorReads"] = {"samplingRate": rng.random()}
    ret["throughputProbingConcurrencyMovingAverageWeight"] = 1 - rng.random()

    # Deal with other special cases of parameters (having to add other sources of randomization, depending on another variable, etc.).
    ret["internalQueryExecYieldIterations"] = rng.choices(
        [
            1,
            rng.randint(
                params["internalQueryExecYieldIterations"]["min"],
                params["internalQueryExecYieldIterations"]["max"],
            ),
        ],
        weights=[1, 10],
    )[0]
    ret["maxNumberOfTransactionOperationsInSingleOplogEntry"] = rng.randint(1, 10) * rng.choice(
        params["maxNumberOfTransactionOperationsInSingleOplogEntry"]["choices"]
    )
    ret["storageEngineConcurrencyAdjustmentAlgorithm"] = rng.choices(
        params["storageEngineConcurrencyAdjustmentAlgorithm"]["choices"], weights=[10, 1]
    )[0]
    ret["wiredTigerStressConfig"] = (
        False
        if fuzzer_stress_mode != "stress"
        else rng.choice(params["wiredTigerStressConfig"]["choices"])
    )
    ret["disableLogicalSessionCacheRefresh"] = rng.choice(
        params["disableLogicalSessionCacheRefresh"]["choices"]
    )
    if not ret["disableLogicalSessionCacheRefresh"]:
        ret["logicalSessionRefreshMillis"] = rng.choice(
            params["logicalSessionRefreshMillis"]["choices"]
        )
    return ret


def generate_flow_control_parameters(rng, ret, flow_control_params, params):
    """Returns an updated dictionary which assigns fuzzed flow control parameters for mongod."""

    # Assigning flow control parameters.
    ret["enableFlowControl"] = rng.choice(params["enableFlowControl"]["choices"])
    if ret["enableFlowControl"]:
        ret["flowControlThresholdLagPercentage"] = rng.random()
        for name in flow_control_params:
            ret[name] = rng.randint(params[name]["min"], params[name]["max"])
    return ret


def generate_mongod_parameters(rng, fuzzer_stress_mode):
    """Return a dictionary with values for each mongod parameter."""
    from buildscripts.resmokelib.config_fuzzer_limits import config_fuzzer_params

    params = config_fuzzer_params["mongod"]

    # Parameter sets with different behaviors.
    flow_control_params = [
        "flowControlTargetLagSeconds",
        "flowControlMaxSamples",
        "flowControlSamplePeriod",
        "flowControlMinTicketsPerSecond",
    ]

    # excluded_normal_params are params that we want to exclude from the for-loop because they have some different assignment behavior
    # e.g. depending on other parameters' values, having rounding, having a different distribution.
    excluded_normal_params = [
        "disableLogicalSessionCacheRefresh",
        "internalQueryExecYieldIterations",
        "logicalSessionRefreshMilli",
        "maxNumberOfTransactionOperationsInSingleOplogEntry",
        "mirrorReads",
        "storageEngineConcurrencyAdjustmentAlgorithm",
        "throughputProbingInitialConcurrency",
        "throughputProbingMinConcurrency",
        "throughputProbingMaxConcurrency",
        "throughputProbingConcurrencyMovingAverageWeight",
        "wiredTigerStressConfig",
    ]
    # TODO (SERVER-75632): Remove/comment out the below line to enable passthrough testing.
    excluded_normal_params.append("lockCodeSegmentsInMemory")

    ret = {}
    # Range through all other parameters and assign the parameters based on the keys that are available or the parameter set lists defined above.
    ret.update(
        {
            key: generate_normal_mongo_parameters(rng, value)
            for key, value in params.items()
            if key not in excluded_normal_params and key not in flow_control_params
        }
    )

    ret = generate_special_mongod_parameters(rng, ret, fuzzer_stress_mode, params)
    ret = generate_flow_control_parameters(rng, ret, flow_control_params, params)
    return ret


def generate_mongos_parameters(rng, fuzzer_stress_mode):
    """Return a dictionary with values for each mongos parameter."""
    from buildscripts.resmokelib.config_fuzzer_limits import config_fuzzer_params

    params = config_fuzzer_params["mongos"]
    return {key: generate_normal_mongo_parameters(rng, value) for key, value in params.items()}


def fuzz_mongod_set_parameters(fuzzer_stress_mode, seed, user_provided_params):
    """Randomly generate mongod configurations and wiredTigerConnectionString."""
    rng = random.Random(seed)

    ret = {}
    mongod_params = generate_mongod_parameters(rng, fuzzer_stress_mode)

    for key, value in mongod_params.items():
        ret[key] = value

    for key, value in utils.load_yaml(user_provided_params).items():
        ret[key] = value

    return (
        utils.dump_yaml(ret),
        generate_eviction_configs(rng, fuzzer_stress_mode),
        generate_table_configs(rng),
        generate_table_configs(rng),
    )


def fuzz_mongos_set_parameters(fuzzer_stress_mode, seed, user_provided_params):
    """Randomly generate mongos configurations."""
    rng = random.Random(seed)

    ret = {}
    params = generate_mongos_parameters(rng, fuzzer_stress_mode)
    for key, value in params.items():
        ret[key] = value

    for key, value in utils.load_yaml(user_provided_params).items():
        ret[key] = value

    return utils.dump_yaml(ret)
