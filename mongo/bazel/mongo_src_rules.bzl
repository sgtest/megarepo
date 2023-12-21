# Common mongo-specific bazel build rules intended to be used in individual BUILD files in the "src/" subtree.
load("@poetry//:dependencies.bzl", "dependency")

# config selection
load("@bazel_skylib//lib:selects.bzl", "selects")

# === Windows-specific compilation settings ===

# /RTC1              Enable Stack Frame Run-Time Error Checking; Reports when a variable is used without having been initialized (implies /Od: no optimizations)
# /O1                optimize for size
# /O2                optimize for speed (as opposed to size)
# /Oy-               disable frame pointer optimization (overrides /O2, only affects 32-bit)
# /INCREMENTAL: NO - disable incremental link - avoid the level of indirection for function calls

WINDOWS_DBG_COPTS = [
    "/MDd",
    "/RTC1",
    "/Od",
]
WINDOWS_OPT_ON_COPTS = [
    "/MD",
    "/O2",
    "/Oy-",
]
WINDOWS_OPT_OFF_COPTS = [
    "-O0",
]

WINDOWS_OPT_DBG_COPTS = [
    "/MD",
    "/RTC1",
    "/Ox",
    "/Zo",
    "/Oy-",
]
WINDOWS_OPT_SIZE_COPTS = [
    "/MD",
    "/Os",
    "/Oy-",
]
WINDOWS_RELEASE_COPTS = [
    "/MD",
    "/Od",
]

MONGO_GLOBAL_COPTS = ["-Isrc"] + select({
    "//bazel/config:windows_dbg": WINDOWS_DBG_COPTS,
    "//bazel/config:windows_opt_on": WINDOWS_OPT_ON_COPTS,
    "//bazel/config:windows_opt_off": WINDOWS_OPT_OFF_COPTS,
    "//bazel/config:windows_opt_debug": WINDOWS_OPT_DBG_COPTS,
    "//bazel/config:windows_opt_size": WINDOWS_OPT_SIZE_COPTS,
    "//bazel/config:windows_release": WINDOWS_RELEASE_COPTS,
    "//conditions:default": [],
})

MONGO_GLOBAL_DEFINES = select({
    "//bazel/config:dbg": ["MONGO_CONFIG_DEBUG_BUILD"],
    "//conditions:default": ["NDEBUG"],
})

LIBUNWIND_DEPS = select({
    "//bazel/config:use_libunwind_enabled": ["//src/third_party/unwind:unwind"],
    "//conditions:default": [],
})

LIBUNWIND_DEFINES = select({
    "//bazel/config:use_libunwind_enabled": ["MONGO_CONFIG_USE_LIBUNWIND"],
    "//conditions:default": [],
})

ADDRESS_SANITIZER_ERROR_MESSAGE = (
    "\nError:\n" +
    "    address sanitizer requires these configurations:\n"+
    "    --//bazel/config:compiler_type=clang\n" +
    "    --//bazel/config:allocator=system\n" +
    "    --//bazel/config:build_mode=opt_on [OR] --//bazel/config:build_mode=opt_debug"
)

ADDRESS_SANITIZER_COPTS = select({
    ("//bazel/config:sanitize_address_required_settings"): ["-fsanitize=address", "-fno-omit-frame-pointer"],
    ("//bazel/config:sanitize_disabled"): [],
}, no_match_error = ADDRESS_SANITIZER_ERROR_MESSAGE)

ADDRESS_SANITIZER_LINKFLAGS = select({
    ("//bazel/config:sanitize_address_required_settings"): ["-fsanitize=address"],
    ("//bazel/config:sanitize_disabled"): [],
}, no_match_error = ADDRESS_SANITIZER_ERROR_MESSAGE)

# Unfortunately, abseil requires that we make these macros
# (this, and THREAD_ and UNDEFINED_BEHAVIOR_ below) set,
# because apparently it is too hard to query the running
# compiler. We do this unconditionally because abseil is
# basically pervasive via the 'base' library.
ADDRESS_SANITIZER_DEFINES = select({
    ("//bazel/config:sanitize_address_required_settings"): ["ADDRESS_SANITIZER"],
    ("//bazel/config:sanitize_disabled"): [],
}, no_match_error = ADDRESS_SANITIZER_ERROR_MESSAGE)


def force_includes_copt(package_name, name):

    if package_name.startswith("src/mongo"):
        basic_h = "mongo/platform/basic.h"
        return select({
            "@platforms//os:windows": ["/FI", basic_h],
            "//conditions:default": ["-include", basic_h],
        })

    if package_name.startswith("src/third_party/mozjs"):
        return select({
            "//bazel/config:linux_aarch64": ["-include", "third_party/mozjs/platform/aarch64/linux/build/js-confdefs.h"],
            "//bazel/config:linux_x86_64": ["-include", "third_party/mozjs/platform/x86_64/linux/build/js-confdefs.h"],
            "//bazel/config:linux_ppc64le": ["-include", "third_party/mozjs/platform/ppc64le/linux/build/js-confdefs.h"],
            "//bazel/config:linux_s390x": ["-include", "third_party/mozjs/platform/s390x/linux/build/js-confdefs.h"],
            "//bazel/config:windows_x86_64": ["/FI", "third_party/mozjs/platform/x86_64/windows/build/js-confdefs.h"],
            "//bazel/config:macos_x86_64": ["-include", "third_party/mozjs/platform/x86_64/macOS/build/js-confdefs.h"],
            "//bazel/config:macos_aarch64": ["-include", "third_party/mozjs/platform/aarch64/macOS/build/js-confdefs.h"],
        })

    if name in ['scripting', 'scripting_mozjs_test', 'encrypted_dbclient']:
        return select({
            "//bazel/config:linux_aarch64": ["-include", "third_party/mozjs/platform/aarch64/linux/build/js-config.h"],
            "//bazel/config:linux_x86_64": ["-include", "third_party/mozjs/platform/x86_64/linux/build/js-config.h"],
            "//bazel/config:linux_ppc64le": ["-include", "third_party/mozjs/platform/ppc64le/linux/build/js-config.h"],
            "//bazel/config:linux_s390x": ["-include", "third_party/mozjs/platform/s390x/linux/build/js-config.h"],
            "//bazel/config:windows_x86_64": ["/FI", "third_party/mozjs/platform/x86_64/windows/build/js-config.h"],
            "//bazel/config:macos_x86_64": ["-include", "third_party/mozjs/platform/x86_64/macOS/build/js-config.h"],
            "//bazel/config:macos_aarch64": ["-include", "third_party/mozjs/platform/aarch64/macOS/build/js-config.h"],
        })

    return []

def force_includes_hdr(package_name, name):
    if package_name.startswith("src/mongo"):
        return select({
            "@platforms//os:windows": ["//src/mongo/platform:basic.h", "//src/mongo/platform:windows_basic.h"],
            "//conditions:default": ["//src/mongo/platform:basic.h"],
        })
        return

    if package_name.startswith("src/third_party/mozjs"):
        return select({
            "//bazel/config:linux_aarch64": ["//src/third_party/mozjs:platform/aarch64/linux/build/js-confdefs.h"],
            "//bazel/config:linux_x86_64": ["//src/third_party/mozjs:platform/x86_64/linux/build/js-confdefs.h"],
            "//bazel/config:linux_ppc64le": ["//src/third_party/mozjs:platform/ppc64le/linux/build/js-confdefs.h"],
            "//bazel/config:linux_s390x": ["//src/third_party/mozjs:platform/s390x/linux/build/js-confdefs.h"],
            "//bazel/config:windows_x86_64": ["//src/third_party/mozjs:/platform/x86_64/windows/build/js-confdefs.h"],
            "//bazel/config:macos_x86_64": ["//src/third_party/mozjs:platform/x86_64/macOS/build/js-confdefs.h"],
            "//bazel/config:macos_aarch64": ["//src/third_party/mozjs:platform/aarch64/macOS/build/js-confdefs.h"],
        })

    if name in ['scripting', 'scripting_mozjs_test', 'encrypted_dbclient']:
        return select({
            "//bazel/config:linux_aarch64": ["//src/third_party/mozjs:platform/aarch64/linux/build/js-config.h"],
            "//bazel/config:linux_x86_64": ["//src/third_party/mozjs:platform/x86_64/linux/build/js-config.h"],
            "//bazel/config:linux_ppc64le": ["//src/third_party/mozjs:platform/ppc64le/linux/build/js-config.h"],
            "//bazel/config:linux_s390x": ["//src/third_party/mozjs:platform/s390x/linux/build/js-config.h"],
            "//bazel/config:windows_x86_64": ["//src/third_party/mozjs:/platform/x86_64/windows/build/js-config.h"],
            "//bazel/config:macos_x86_64": ["//src/third_party/mozjs:platform/x86_64/macOS/build/js-config.h"],
            "//bazel/config:macos_aarch64": ["//src/third_party/mozjs:platform/aarch64/macOS/build/js-config.h"],
        })

    return []

def mongo_cc_library(
        name,
        srcs = [],
        hdrs = [],
        deps = [],
        testonly = False,
        visibility = None,
        data = [],
        tags = [],
        copts = [],
        linkopts = [],
        linkstatic = False,
        local_defines = []):
    """Wrapper around cc_library.

    Args:
      name: The name of the library the target is compiling.
      srcs: The source files to build.
      hdrs: The headers files of the target library.
      deps: The targets the library depends on.
      testonly: Whether or not the target is purely for tests.
      visibility: The visibility of the target library.
      data: Data targets the library depends on.
      tags: Tags to add to the rule.
      copts: Any extra compiler options to pass in.
      linkopts: Any extra link options to pass in.
      linkstatic: Whether or not linkstatic should be passed to the native bazel cc_test rule. This argument
        is ignored on windows since linking into DLLs is not currently supported.
      local_defines: macro definitions passed to all source and header files.
    """

    # Avoid injecting into unwind/libunwind_asm to avoid a circular dependency.
    if name not in ["unwind", "libunwind_asm"]:
        deps += LIBUNWIND_DEPS
        local_defines += LIBUNWIND_DEFINES

    fincludes_copt = force_includes_copt(native.package_name(), name)
    fincludes_hdr = force_includes_hdr(native.package_name(), name)

    native.cc_library(
        name = name,
        srcs = srcs,
        hdrs = hdrs + fincludes_hdr,
        deps = deps,
        visibility = visibility,
        testonly = testonly,
        copts = MONGO_GLOBAL_COPTS + ADDRESS_SANITIZER_COPTS + copts + fincludes_copt,
        data = data,
        tags = tags,
        linkstatic = select({
            "@platforms//os:windows": True,
            "//conditions:default": linkstatic,
        }),
        local_defines = MONGO_GLOBAL_DEFINES + ADDRESS_SANITIZER_DEFINES + local_defines,
        includes = [],
    )

def mongo_cc_binary(
        name,
        srcs = [],
        deps = [],
        testonly = False,
        visibility = None,
        data = [],
        tags = [],
        copts = [],
        linkopts = [],
        linkstatic = False,
        local_defines = []):
    """Wrapper around cc_binary.

    Args:
      name: The name of the library the target is compiling.
      srcs: The source files to build.
      deps: The targets the library depends on.
      testonly: Whether or not the target is purely for tests.
      visibility: The visibility of the target library.
      data: Data targets the library depends on.
      tags: Tags to add to the rule.
      copts: Any extra compiler options to pass in.
      linkopts: Any extra link options to pass in.
      linkstatic: Whether or not linkstatic should be passed to the native bazel cc_test rule. This argument
        is ignored on windows since linking into DLLs is not currently supported.
      local_defines: macro definitions passed to all source and header files.
    """

    fincludes_copt = force_includes_copt(native.package_name(), name)
    fincludes_hdr = force_includes_hdr(native.package_name(), name)

    native.cc_binary(
        name = name,
        srcs = srcs + fincludes_hdr,
        deps = deps + LIBUNWIND_DEPS,
        visibility = visibility,
        testonly = testonly,
        copts = MONGO_GLOBAL_COPTS + ADDRESS_SANITIZER_COPTS + copts + fincludes_copt,
        data = data,
        tags = tags,
        linkstatic = select({
            "@platforms//os:windows": True,
            "//conditions:default": linkstatic,
        }),
        local_defines = MONGO_GLOBAL_DEFINES + ADDRESS_SANITIZER_DEFINES + LIBUNWIND_DEFINES + local_defines,
        malloc = select({
          "//bazel/config:tcmalloc_allocator": "//src/third_party/gperftools:tcmalloc_minimal",
          "//bazel/config:auto_allocator_windows": "//src/third_party/gperftools:tcmalloc_minimal",
          "//bazel/config:auto_allocator_linux": "//src/third_party/gperftools:tcmalloc_minimal",
          "//conditions:default": "@bazel_tools//tools/cpp:malloc",
        }),
        includes = [],
    )

IdlInfo = provider(
    fields = {
        "idl_deps": "depset of idl files",
    },
)

def idl_generator_impl(ctx):
    base = ctx.attr.src.files.to_list()[0].basename.removesuffix(".idl")
    gen_source = ctx.actions.declare_file(base + "_gen.cpp")
    gen_header = ctx.actions.declare_file(base + "_gen.h")

    python = ctx.toolchains["@bazel_tools//tools/python:toolchain_type"].py3_runtime
    idlc_path = ctx.attr.idlc.files.to_list()[0].path
    dep_depsets = [dep[IdlInfo].idl_deps for dep in ctx.attr.deps]

    # collect deps from python modules and setup the corresponding
    # path so all modules can be found by the toolchain.
    python_path = []
    for py_dep in ctx.attr.py_deps:
        for dep in py_dep[PyInfo].transitive_sources.to_list():
            if dep.path not in  python_path:
                python_path.append(dep.path)
    py_depsets = [py_dep[PyInfo].transitive_sources for py_dep in ctx.attr.py_deps]

    inputs = depset(transitive=[
        ctx.attr.src.files,
        ctx.attr.idlc.files,
        python.files] + dep_depsets + py_depsets)

    ctx.actions.run(
        executable = python.interpreter.path,
        outputs = [gen_source, gen_header],
        inputs = inputs,
        arguments = [
            'buildscripts/idl/idlc.py',
            '--include', 'src',
            '--base_dir', ctx.bin_dir.path + '/src',
            '--target_arch', ctx.var['TARGET_CPU'],
            '--header', gen_header.path,
            '--output', gen_source.path,
            ctx.attr.src.files.to_list()[0].path
        ],
        mnemonic = "IdlcGenerator",
         env={"PYTHONPATH":ctx.configuration.host_path_separator.join(python_path)}
    )

    return [
        DefaultInfo(
            files = depset([gen_source, gen_header]),
        ),
        IdlInfo(
            idl_deps = depset(ctx.attr.src.files.to_list(), transitive=[dep[IdlInfo].idl_deps for dep in ctx.attr.deps])
        )
    ]

idl_generator = rule(
    idl_generator_impl,
    attrs = {
        "src": attr.label(
            doc = "The idl file to generate cpp/h files from.",
            allow_single_file=True,
        ),
        "idlc" : attr.label(
            doc = "The idlc generator to use.",
            default = "//buildscripts/idl:idlc",
        ),
        "py_deps" : attr.label_list(
            doc = "Python modules that should be imported.",
            providers = [PyInfo],
            default=[dependency("pyyaml", group="core"), dependency("pymongo", group="core")]
        ),
        "deps" : attr.label_list(
            doc = "Other idl files that need to be imported.",
            providers = [IdlInfo],
        ),
    },
    doc = "Generates header/source files from IDL files.",
    toolchains = ["@bazel_tools//tools/python:toolchain_type"],
    fragments = ["py"]
)
