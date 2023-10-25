import collections
import os
import platform
import SCons
import shutil
import subprocess
import stat
import typing
import urllib.request

import mongo.generators as mongo_generators

BAZELISK_PATH = None  # Absolute path to Bazelisk executable (determined in generate(), below)


# Required boilerplate function
def exists(env):
    return True


CopyOperation = collections.namedtuple("CopyOperation", ["infile", "outfile"])


class BazelRunner:
    """Singleton class that exists to invoke Bazel a single time for all targets.
    
    We use a singleton because Bazel doesn't support concurrent execution (that is, multiple
    instances of bazel simultaneously).
    """

    def __init__(self):
        self.executed = False
        self.bazel_targets = set()
        self.copy_operations = set()
        self.compile_args = list()
        self.link_args = list()

    def add_target(self, bazel_target: str, copy_operations: typing.Set):
        """Add another target to build.

        Args:
          bazel_target: A Bazel target (e.g.: "//foo/bar:baz")
          copy_operations: Set of CopyOperation instances to be executed after a successful build
        """
        self.bazel_targets.add(bazel_target)
        self.copy_operations.update(copy_operations)

    def set_compile_args(self, compile_args: typing.List):
        self.compile_args = compile_args

    def set_link_args(self, link_args: typing.List):
        self.link_args = link_args

    def invoke_build(self, env):
        # Only run once at most.
        # Note that we're not using a Python synchronization mechanism, because testing has shown
        # that SCons will *not* invoke this function in parallel.
        if self.executed:
            return

        args = [BAZELISK_PATH, 'build'] + list(self.bazel_targets) + list(self.compile_args) + \
            list(self.link_args) + SCons.Util.CLVar(env.get("BAZEL_FLAGS", []))
        print("Running bazel build command:", *args)

        # Build it and copy output.
        subprocess.run(args, shell=False, check=True)
        for copy_operation in self.copy_operations:
            os.makedirs(os.path.dirname(copy_operation.outfile), exist_ok=True)
            shutil.copyfile(copy_operation.infile, copy_operation.outfile)

        # Record the fact that it ran:
        self.executed = True


BAZEL_RUNNER = BazelRunner()
BAZEL_ACTION = SCons.Action.Action(lambda target, source, env: BAZEL_RUNNER.invoke_build(env))


# Establishes logic for BazelLibrary build rule
def generate(env):
    def bazel_library(env, target, source, *args, **kwargs):
        # Get info about targets & paths.
        # For a target such as 'fsync_locked', the variables would be:
        #   target_to_build:       fsync_locked
        #   cwd:                   /home/ubuntu/mongo/src/mongo/db/commands
        #   bazel_dir:             src/mongo/db/commands
        #   bazel_target:          //src/mongo/db/commands:fsync_locked
        #   bazel_outfile_base:    bazel-bin/src/mongo/db/commands/libfsync_locked
        #   base_dir_from_scons:   build/fast
        #   scons_outfile_relpath: build/fast/mongo/db/commands
        #   scons_outfile_base:    /home/ubuntu/mongo/build/fast/mongo/db/commands/libfsync_locked
        target_to_build = target[0]
        cwd = os.getcwd()
        bazel_dir = os.path.dirname(env.File(os.path.join(cwd, target_to_build)).srcnode().path)
        bazel_target = f"//{bazel_dir}:{target_to_build}"
        bazel_outfile_base = f"bazel-bin/{bazel_dir}/lib{target_to_build}"
        # Note: The IDLCFLAGS are of the form: [..., '--base_dir', 'build/fast', ...]; the logic
        # below fetches the value immediately after "--base_dir".
        base_dir_from_scons = env.Dictionary()['IDLCFLAGS'][
            env.Dictionary()['IDLCFLAGS'].index('--base_dir') + 1]
        scons_outfile_relpath = bazel_dir.replace('src/', f'{base_dir_from_scons}/')
        workspace_root = env.Dir('#').abspath
        scons_outfile_base = f"{workspace_root}/{scons_outfile_relpath}/lib{target_to_build}"
        # TODO(PM-3332): Remove this debugging info once Bazel adoption is complete
        print(f"Bazel debugging info:")
        print(f"\tworkspace_root: {workspace_root}")
        print(f"\ttarget_to_build: {target_to_build}")
        print(f"\tcwd: {cwd}")
        print(f"\tbazel_dir: {bazel_dir}")
        print(f"\tbazel_target: {bazel_target}")
        print(f"\tbazel_outfile_base: {bazel_outfile_base}")
        print(f"\tbase_dir_from_scons: {base_dir_from_scons}")
        print(f"\tscons_outfile_relpath: {scons_outfile_relpath}")
        print(f"\tscons_outfile_base: {scons_outfile_base}")

        # Add this target to the global Bazel runner:
        global BAZEL_RUNNER
        BAZEL_RUNNER.add_target(
            bazel_target,
            set([
                CopyOperation(f"{bazel_outfile_base}.a", f"{scons_outfile_base}.a"),
                CopyOperation(f"{bazel_outfile_base}.so", f"{scons_outfile_base}.so")
            ]))

        # Invoke the build (note that we only specify static library target; the build action
        # will build both static and dynamic as a result):
        return env._BazelBuild(target=f"{scons_outfile_base}.a", source=source, *args, **kwargs)

    if env.get("BAZEL_BUILD_ENABLED"):
        # === Architecture ===

        # Bail if current architecture not supported for Bazel:
        current_architecture = platform.machine()
        supported_architectures = ['aarch64']
        if current_architecture not in supported_architectures:
            raise Exception(
                f'Bazel not supported on this architecture ({current_architecture}); supported architectures are: [{supported_architectures}]'
            )

        # === Bazelisk ===

        # TODO(SERVER-81038): remove once bazel/bazelisk is self-hosted.
        if not os.path.exists("bazelisk"):
            urllib.request.urlretrieve(
                "https://github.com/bazelbuild/bazelisk/releases/download/v1.17.0/bazelisk-linux-arm64",
                "bazelisk")
            os.chmod("bazelisk", stat.S_IXUSR)

        global BAZELISK_PATH
        BAZELISK_PATH = os.path.abspath("bazelisk")

        # === Build settings ===

        static_link = env.GetOption("link-model") in ["auto", "static"]
        if not env.ToolchainIs('gcc', 'clang'):
            raise Exception(f"Toolchain {env.ToolchainName()} is neither `gcc` nor `clang`")

        compiler_args = [f"--//bazel/config:compiler_type={env.ToolchainName()}"]
        if env.GetOption("use-libunwind") == "on":
            compiler_args += ["--//bazel/config:use_libunwind"]

        global BAZEL_RUNNER
        if env.GetOption("release") is not None:
            build_mode = "release"
        elif env.GetOption("dbg") == "on":
            build_mode = "dbg"
        else:
            build_mode = f"opt_{mongo_generators.get_opt_options(env)}"  # one of "on", "size", "debug"
        compiler_args.append(f"--//bazel/config:build_mode={build_mode}")
        BAZEL_RUNNER.set_compile_args(compiler_args)
        BAZEL_RUNNER.set_link_args(
            ["--dynamic_mode=off"] if static_link else ["--dynamic_mode=fully"])

        # === Builders ===

        env['BUILDERS']['BazelLibrary'] = bazel_library
        global BAZEL_ACTION
        env['BUILDERS']['_BazelBuild'] = SCons.Builder.Builder(action=BAZEL_ACTION)
    else:
        env['BUILDERS']['BazelLibrary'] = env['BUILDERS']['Library']
