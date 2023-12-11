"""TensorFlow workspace initialization. Consult the WORKSPACE on how to use it."""

# Import TSL Workspaces
load("@local_tsl//:workspace2.bzl", "tsl_workspace2")

# Import third party config rules.
load("@bazel_skylib//lib:versions.bzl", "versions")
load("//third_party:repo.bzl", "tf_http_archive", "tf_mirror_urls")

# Import third party repository rules. See go/tfbr-thirdparty.
load("//third_party/dlpack:workspace.bzl", dlpack = "repo")
load("//third_party/gloo:workspace.bzl", gloo = "repo")
load("//third_party/stablehlo:workspace.bzl", stablehlo = "repo")
load("//third_party/triton:workspace.bzl", triton = "repo")

def _initialize_third_party():
    """ Load third party repositories.  See above load() statements. """
    dlpack()
    gloo()
    stablehlo()
    triton()

# Define all external repositories required by TensorFlow
def _tf_repositories():
    """All external dependencies for TF builds."""

    # To update any of the dependencies below:
    # a) update URL and strip_prefix to the new git commit hash
    # b) get the sha256 hash of the commit by running:
    #    curl -L <url> | sha256sum
    # and update the sha256 with the result.

    tf_http_archive(
        name = "cudnn_frontend_archive",
        build_file = "//third_party:cudnn_frontend.BUILD",
        patch_file = ["//third_party:cudnn_frontend_header_fix.patch"],
        sha256 = "d8dba9e2607a0c256aa8eacb45b39986ab6f3f24a4d431d4397047a3cb0cd4fb",
        strip_prefix = "cudnn-frontend-0.9",
        urls = tf_mirror_urls("https://github.com/NVIDIA/cudnn-frontend/archive/refs/tags/v0.9.zip"),
    )

    tf_http_archive(
        name = "cutlass_archive",
        build_file = "//third_party:cutlass.BUILD",
        sha256 = "ea1b7f96919460a5d80b09c1b246652539a8605600b2be4cccc02c254bccbe50",
        strip_prefix = "cutlass-5783d6dbd0c34032371cce2bd999fc76007520d7",
        urls = tf_mirror_urls("https://github.com/chsigg/cutlass/archive/5783d6dbd0c34032371cce2bd999fc76007520d7.tar.gz"),
    )

    tf_http_archive(
        name = "boringssl",
        sha256 = "9dc53f851107eaf87b391136d13b815df97ec8f76dadb487b58b2fc45e624d2c",
        strip_prefix = "boringssl-c00d7ca810e93780bd0c8ee4eea28f4f2ea4bcdc",
        system_build_file = "//third_party/systemlibs:boringssl.BUILD",
        urls = tf_mirror_urls("https://github.com/google/boringssl/archive/c00d7ca810e93780bd0c8ee4eea28f4f2ea4bcdc.tar.gz"),
    )

    tf_http_archive(
        name = "com_google_ortools",
        sha256 = "bc4b07dc9c23f0cca43b1f5c889f08a59c8f2515836b03d4cc7e0f8f2c879234",
        strip_prefix = "or-tools-9.6",
        patch_file = ["//third_party/ortools:ortools.patch"],
        urls = tf_mirror_urls("https://github.com/google/or-tools/archive/v9.6.tar.gz"),
        repo_mapping = {
            "@com_google_protobuf_cc": "@com_google_protobuf",
            "@eigen": "@eigen_archive",
        },
    )

    tf_http_archive(
        name = "glpk",
        sha256 = "9a5dab356268b4f177c33e00ddf8164496dc2434e83bd1114147024df983a3bb",
        build_file = "//third_party/ortools:glpk.BUILD",
        urls = [
            "https://storage.googleapis.com/mirror.tensorflow.org/ftp.gnu.org/gnu/glpk/glpk-4.52.tar.gz",
            "http://ftp.gnu.org/gnu/glpk/glpk-4.52.tar.gz",
        ],
    )

    tf_http_archive(
        name = "scip",
        sha256 = "fe7636f8165a8c9298ff55ed3220d084d4ea31ba9b69d2733beec53e0e4335d6",
        strip_prefix = "scip-803",
        build_file = "//third_party/ortools:scip.BUILD",
        patch_file = ["//third_party/ortools:scip.patch"],
        urls = tf_mirror_urls("https://github.com/scipopt/scip/archive/refs/tags/v803.tar.gz"),
    )

    tf_http_archive(
        name = "bliss",
        build_file = "//third_party/ortools:bliss.BUILD",
        sha256 = "f57bf32804140cad58b1240b804e0dbd68f7e6bf67eba8e0c0fa3a62fd7f0f84",
        urls = tf_mirror_urls("https://github.com/google/or-tools/releases/download/v9.0/bliss-0.73.zip"),
        #url = "http://www.tcs.hut.fi/Software/bliss/bliss-0.73.zip",
    )

# buildifier: disable=function-docstring
# buildifier: disable=unnamed-macro
def workspace():
    tsl_workspace2()

    # Check the bazel version before executing any repository rules, in case
    # those rules rely on the version we require here.
    versions.check("1.0.0")

    # Import third party repositories according to go/tfbr-thirdparty.
    _initialize_third_party()

    # Import all other repositories. This should happen before initializing
    # any external repositories, because those come with their own
    # dependencies. Those recursive dependencies will only be imported if they
    # don't already exist (at least if the external repository macros were
    # written according to common practice to query native.existing_rule()).
    _tf_repositories()

# Alias so it can be loaded without assigning to a different symbol to prevent
# shadowing previous loads and trigger a buildifier warning.
xla_workspace2 = workspace
