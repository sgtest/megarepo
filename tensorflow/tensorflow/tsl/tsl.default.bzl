"""Default (OSS) build versions of TSL general-purpose build extensions."""

load(
    "//tensorflow/tsl:tsl.bzl",
    _filegroup = "filegroup",
    _get_compatible_with_portable = "get_compatible_with_portable",
    _if_not_mobile_or_arm_or_lgpl_restricted = "if_not_mobile_or_arm_or_lgpl_restricted",
    _internal_hlo_deps = "internal_hlo_deps",
    _tsl_grpc_cc_dependencies = "tsl_grpc_cc_dependencies",
    _tsl_pybind_extension = "tsl_pybind_extension",
)

get_compatible_with_portable = _get_compatible_with_portable
filegroup = _filegroup
if_not_mobile_or_arm_or_lgpl_restricted = _if_not_mobile_or_arm_or_lgpl_restricted
internal_hlo_deps = _internal_hlo_deps
tsl_grpc_cc_dependencies = _tsl_grpc_cc_dependencies
tsl_pybind_extension = _tsl_pybind_extension
