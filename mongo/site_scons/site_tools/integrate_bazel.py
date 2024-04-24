import errno
import getpass
import hashlib
from io import StringIO
import json
import os
import distro
import platform
import queue
import shlex
import shutil
import stat
import subprocess
import time
import threading
from typing import List, Dict, Set, Tuple, Any
import urllib.request
import requests
from retry import retry
import sys
from buildscripts.install_bazel import install_bazel
import atexit

import SCons

import mongo.platform as mongo_platform
import mongo.generators as mongo_generators

_SUPPORTED_PLATFORM_MATRIX = [
    "linux:arm64:gcc",
    "linux:arm64:clang",
    "linux:amd64:gcc",
    "linux:amd64:clang",
    "linux:ppc64le:gcc",
    "linux:ppc64le:clang",
    "linux:s390x:gcc",
    "linux:s390x:clang",
    "windows:amd64:msvc",
    "macos:amd64:clang",
    "macos:arm64:clang",
]

_SANITIZER_MAP = {
    "address": "asan",
    "fuzzer": "fsan",
    "memory": "msan",
    "leak": "lsan",
    "thread": "tsan",
    "undefined": "ubsan",
}

_DISTRO_PATTERN_MAP = {
    "Ubuntu 18*": "ubuntu18",
    "Ubuntu 20*": "ubuntu20",
    "Ubuntu 22*": "ubuntu22",
    "Amazon Linux 2": "amazon_linux_2",
    "Amazon Linux 2023": "amazon_linux_2023",
    "Debian GNU/Linux 10": "debian10",
    "Debian GNU/Linux 12": "debian12",
    "Red Hat Enterprise Linux Server 7*": "rhel7",
    "Red Hat Enterprise Linux 7*": "rhel7",
    "Red Hat Enterprise Linux 8*": "rhel8",
    "Red Hat Enterprise Linux 9*": "rhel9",
    "SLES 15*": "suse15",
}

_S3_HASH_MAPPING = {
    "https://mdb-build-public.s3.amazonaws.com/bazel-binaries/bazel-6.4.0-ppc64le":
        "dd21c75817533ff601bf797e64f0eb2f7f6b813af26c829f0bda30e328caef46",
    "https://mdb-build-public.s3.amazonaws.com/bazel-binaries/bazel-6.4.0-s390x":
        "6d72eabc1789b041bbe4cfc033bbac4491ec9938ef6da9899c0188ecf270a7f4",
    "https://mdb-build-public.s3.amazonaws.com/bazelisk-binaries/v1.19.0/bazelisk-darwin-amd64":
        "f2ba5f721a995b54bab68c6b76a340719888aa740310e634771086b6d1528ecd",
    "https://mdb-build-public.s3.amazonaws.com/bazelisk-binaries/v1.19.0/bazelisk-darwin-arm64":
        "69fa21cd2ccffc2f0970c21aa3615484ba89e3553ecce1233a9d8ad9570d170e",
    "https://mdb-build-public.s3.amazonaws.com/bazelisk-binaries/v1.19.0/bazelisk-linux-amd64":
        "d28b588ac0916abd6bf02defb5433f6eddf7cba35ffa808eabb65a44aab226f7",
    "https://mdb-build-public.s3.amazonaws.com/bazelisk-binaries/v1.19.0/bazelisk-linux-arm64":
        "861a16ba9979613e70bd3d2f9d9ab5e3b59fe79471c5753acdc9c431ab6c9d94",
    "https://mdb-build-public.s3.amazonaws.com/bazelisk-binaries/v1.19.0/bazelisk-windows-amd64.exe":
        "d04555245a99dfb628e33da24e2b9198beb8f46d7e7661c313eb045f6a59f5e4",
}


class Globals:

    # key: scons target, value: {bazel target, bazel output}
    scons2bazel_targets: Dict[str, Dict[str, str]] = dict()

    # key: scons output, value: bazel outputs
    scons_output_to_bazel_outputs: Dict[str, List[str]] = dict()

    # targets bazel needs to build
    bazel_targets_work_queue: queue.Queue[str] = queue.Queue()

    # targets bazel has finished building
    bazel_targets_done: Set[str] = set()

    # lock for accessing the targets done list
    bazel_target_done_CV: threading.Condition = threading.Condition()

    # bazel command line with options, but not targets
    bazel_base_build_command: List[str] = None

    # environment variables to set when invoking bazel
    bazel_env_variables: Dict[str, str] = {}

    # Flag to signal that scons is ready to build, but needs to wait on bazel
    waiting_on_bazel_flag: bool = False

    # a IO object to hold the bazel output in place of stdout
    bazel_thread_terminal_output = StringIO()

    bazel_executable = None

    bazel_fetch_thread = None

    @staticmethod
    def bazel_output(scons_node):
        return Globals.scons2bazel_targets[str(scons_node).replace("\\", "/")]['bazel_output']

    @staticmethod
    def bazel_target(scons_node):
        return Globals.scons2bazel_targets[str(scons_node).replace("\\", "/")]['bazel_target']


def bazel_debug(msg: str):
    pass


# Required boilerplate function
def exists(env: SCons.Environment.Environment) -> bool:
    return True


def convert_scons_node_to_bazel_target(scons_node: SCons.Node.FS.File) -> str:
    """Convert a scons node object into a bazel target label."""

    # gets the SCons.Environment for the node
    env = scons_node.get_env()

    # convert to the source path i.e.: src/mongo/db/libcommands.so
    bazel_path = scons_node.srcnode().path
    # bazel uses source paths in the output i.e.: src/mongo/db, replace backslashes on windows
    bazel_dir = os.path.dirname(bazel_path).replace("\\", "/")

    # extract the platform prefix for a given file so we can remove it i.e.: libcommands.so -> 'lib'
    prefix = env.subst(scons_node.get_builder().get_prefix(env), target=[scons_node],
                       source=scons_node.sources) if scons_node.has_builder() else ""

    # the shared archive builder hides the prefix added by their parent builder, set it manually
    if scons_node.name.endswith(".so.a") or scons_node.name.endswith(".dylib.a"):
        prefix = "lib"

    # now get just the file name without and prefix or suffix i.e.: libcommands.so -> 'commands'
    prefix_suffix_removed = scons_node.name[len(prefix):].split(".")[0]

    # i.e.: //src/mongo/db:commands>
    return f"//{bazel_dir}:{prefix_suffix_removed}"


def bazel_target_emitter(
        target: List[SCons.Node.Node], source: List[SCons.Node.Node],
        env: SCons.Environment.Environment) -> Tuple[List[SCons.Node.Node], List[SCons.Node.Node]]:
    """This emitter will map any scons outputs to bazel outputs so copy can be done later."""

    for t in target:
        # Bug in Windows shared library emitter returns a string rather than a node
        if type(t) == str:
            t = env.arg2nodes(t)[0]

        # normally scons emitters conveniently build-ify the target paths so it will
        # reference the output location, but we actually want the node path
        # from the original source tree location, so srcnode() will do this for us
        bazel_path = t.srcnode().path
        bazel_dir = os.path.dirname(bazel_path)

        # the new builders are just going to copy, so we are going to calculate the bazel
        # output location and then set that as the new source for the builders.
        bazel_out_dir = env.get("BAZEL_OUT_DIR")
        bazel_out_target = f'{bazel_out_dir}/{bazel_dir}/{os.path.basename(bazel_path)}'

        Globals.scons2bazel_targets[t.path.replace('\\', '/')] = {
            'bazel_target': convert_scons_node_to_bazel_target(t),
            'bazel_output': bazel_out_target.replace('\\', '/')
        }

        # scons isn't aware of bazel build definition files, so cache won't be invalidated when they change
        # force scons to always request bazel to build any converted targets
        # since bazel maintains its own cache, this won't result in redundant build executions
        env.AlwaysBuild(t)
        env.NoCache(t)

    return (target, source)


def bazel_builder_action(env: SCons.Environment.Environment, target: List[SCons.Node.Node],
                         source: List[SCons.Node.Node]):

    # now copy all the targets out to the scons tree, note that target is a
    # list of nodes so we need to stringify it for copyfile
    for t in target:
        s = Globals.bazel_output(t)
        shutil.copy(s, str(t))
        os.chmod(str(t), os.stat(str(t)).st_mode | stat.S_IWUSR)


BazelCopyOutputsAction = SCons.Action.FunctionAction(
    bazel_builder_action,
    {"cmdstr": "Copying $TARGET from bazel build directory.", "varlist": ['BAZEL_FLAGS_STR']},
)

total_query_time = 0
total_queries = 0


def bazel_query_func(env: SCons.Environment.Environment, query_command_args: List[str],
                     query_name: str = "query"):

    bazel_debug(f"Running query: {' '.join(query_command_args)}")
    global total_query_time, total_queries
    start_time = time.time()
    # these args prune the graph we need to search through a bit since we only care about our
    # specific library target dependencies
    query_command_args += ['--implicit_deps=False', '--tool_deps=False', '--include_aspects=False']
    # prevent remote connection and invocations since we just want to query the graph
    query_command_args += [
        "--remote_executor=", "--remote_cache=", '--bes_backend=', '--bes_results_url='
    ]
    results = subprocess.run([Globals.bazel_executable] + query_command_args, capture_output=True,
                             text=True, cwd=env.Dir('#').abspath)
    delta = time.time() - start_time
    bazel_debug(f"Spent {delta} seconds running {query_name}")
    total_query_time += delta
    total_queries += 1
    return results


# the ninja tool has some API that doesn't support using SCons env methods
# instead of adding more API to the ninja tool which has a short life left
# we just add the unused arg _dup_env
def ninja_bazel_builder(env: SCons.Environment.Environment, _dup_env: SCons.Environment.Environment,
                        node: SCons.Node.Node) -> Dict[str, Any]:
    """
    Translator for ninja which turns the scons bazel_builder_action
    into a build node that ninja can digest.
    """

    outs = env.NinjaGetOutputs(node)
    ins = [Globals.bazel_output(out) for out in outs]

    # this represents the values the ninja_syntax.py will use to generate to real
    # ninja syntax defined in the ninja manaul: https://ninja-build.org/manual.html#ref_ninja_file
    return {
        "outputs": outs,
        "inputs": ins,
        "rule": "BAZEL_COPY_RULE",
        "variables": {
            "cmd":
                ' & '.join([
                    f"$COPY {input_node.replace('/',os.sep)} {output_node}"
                    for input_node, output_node in zip(ins, outs)
                ])
        },
    }


def bazel_build_thread_func(log_dir: str, verbose: bool) -> None:
    """This thread runs the bazel build up front."""

    done_with_temp = False
    # removed the log directory creation which was intermittently
    # erroring on a race-condition
    # FIX: The fix is in https://github.com/10gen/mongo/pull/21020

    if verbose:
        extra_args = []
    else:
        extra_args = ["--output_filter=DONT_MATCH_ANYTHING"]

    bazel_cmd = Globals.bazel_base_build_command + extra_args + ['//src/...']
    bazel_debug(f"BAZEL_COMMAND: {' '.join(bazel_cmd)}")
    print("Starting bazel build thread...")

    try:
        import pty
        parent_fd, child_fd = pty.openpty()  # provide tty
        bazel_proc = subprocess.Popen(bazel_cmd, stdin=child_fd, stdout=child_fd,
                                      stderr=subprocess.STDOUT,
                                      env={**os.environ.copy(), **Globals.bazel_env_variables})

        os.close(child_fd)
        try:
            while True:
                try:
                    data = os.read(parent_fd, 512)
                except OSError as e:
                    if e.errno != errno.EIO:
                        raise
                    break  # EIO means EOF on some systems
                else:
                    if not data:  # EOF
                        break

                    if Globals.waiting_on_bazel_flag:
                        if not done_with_temp:
                            done_with_temp = True
                            Globals.bazel_thread_terminal_output.seek(0)
                            sys.stdout.write(Globals.bazel_thread_terminal_output.read())
                            Globals.bazel_thread_terminal_output = None
                        sys.stdout.write(data.decode())
                    else:
                        Globals.bazel_thread_terminal_output.write(data.decode())
        finally:
            os.close(parent_fd)
            if bazel_proc.poll() is None:
                bazel_proc.kill()
            bazel_proc.wait()

            if bazel_proc.returncode != 0:
                print("ERROR: Bazel build failed:")
                stdout = ""

                if not done_with_temp:
                    Globals.bazel_thread_terminal_output.seek(0)
                    stdout += Globals.bazel_thread_terminal_output.read()
                    Globals.bazel_thread_terminal_output = None
                    print(stdout)

                raise subprocess.CalledProcessError(bazel_proc.returncode, bazel_cmd, stdout, "")

    except ImportError:
        bazel_proc = subprocess.Popen(bazel_cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                      env={**os.environ.copy(),
                                           **Globals.bazel_env_variables}, text=True)
        while True:
            line = bazel_proc.stdout.readline()
            if not line:
                break
            if Globals.waiting_on_bazel_flag:
                if not done_with_temp:
                    done_with_temp = True
                    Globals.bazel_thread_terminal_output.seek(0)
                    sys.stdout.write(Globals.bazel_thread_terminal_output.read())
                    Globals.bazel_thread_terminal_output = None
                sys.stdout.write(line)
            else:
                Globals.bazel_thread_terminal_output.write(line)

        stdout, stderr = bazel_proc.communicate()

        if bazel_proc.returncode != 0:
            print("ERROR: Bazel build failed:")

            if not done_with_temp:
                Globals.bazel_thread_terminal_output.seek(0)
                stdout += Globals.bazel_thread_terminal_output.read()
                Globals.bazel_thread_terminal_output = None
                print(stdout)

            raise subprocess.CalledProcessError(bazel_proc.returncode, bazel_cmd, stdout, stderr)


def create_bazel_builder(builder: SCons.Builder.Builder) -> SCons.Builder.Builder:
    return SCons.Builder.Builder(
        action=BazelCopyOutputsAction,
        prefix=builder.prefix,
        suffix=builder.suffix,
        src_suffix=builder.src_suffix,
        source_scanner=builder.source_scanner,
        target_scanner=builder.target_scanner,
        emitter=SCons.Builder.ListEmitter([builder.emitter, bazel_target_emitter]),
    )


# The next section of builders are hook builders. These
# will be standin place holders for the original scons builders, and if bazel build is enabled
# these simply copy out the target from the underlying bazel build
def create_library_builder(env: SCons.Environment.Environment) -> None:
    if env.GetOption("link-model") in ["auto", "static"]:
        env['BUILDERS']['BazelLibrary'] = create_bazel_builder(env['BUILDERS']["StaticLibrary"])
    else:
        env['BUILDERS']['BazelSharedLibrary'] = create_bazel_builder(
            env['BUILDERS']["SharedLibrary"])
        env['BUILDERS']['BazelSharedArchive'] = create_bazel_builder(
            env['BUILDERS']["SharedArchive"])

        def sharedArchiveAndSharedLibrary(env, target, source, *args, **kwargs):
            sharedLibrary = env.BazelSharedLibrary(target, source, *args, **kwargs)
            sharedArchive = env.BazelSharedArchive(target, source=sharedLibrary[0].sources, *args,
                                                   **kwargs)
            sharedLibrary.extend(sharedArchive)
            return sharedLibrary

        env['BUILDERS']['BazelLibrary'] = sharedArchiveAndSharedLibrary


def create_program_builder(env: SCons.Environment.Environment) -> None:
    env['BUILDERS']['BazelProgram'] = create_bazel_builder(env['BUILDERS']["Program"])


def create_idlc_builder(env: SCons.Environment.Environment) -> None:
    env['BUILDERS']['BazelIdlc'] = create_bazel_builder(env['BUILDERS']["Idlc"])


def validate_remote_execution_certs(env: SCons.Environment.Environment) -> bool:
    running_in_evergreen = os.environ.get("CI")

    if running_in_evergreen and not os.path.exists("./engflow.cert"):
        print(
            "ERROR: ./engflow.cert not found, which is required to build in evergreen without BAZEL_FLAGS=--config=local set. Please reach out to #server-dev-platform for help."
        )
        return False

    if not running_in_evergreen and not os.path.exists(
            f"/home/{getpass.getuser()}/.engflow/creds/engflow.crt"):
        # Temporary logic to copy over the credentials for users that ran the installation steps using the old directory (/engflow/).
        if os.path.exists("/engflow/creds/engflow.crt") and os.path.exists(
                "/engflow/creds/engflow.key"):
            print(
                "Moving EngFlow credentials from the legacy directory (/engflow/) to the new directory (~/.engflow/)."
            )
            try:
                os.makedirs(f"/home/{getpass.getuser()}/.engflow/creds/", exist_ok=True)
                shutil.move("/engflow/creds/engflow.crt",
                            f"/home/{getpass.getuser()}/.engflow/creds/engflow.crt")
                shutil.move("/engflow/creds/engflow.key",
                            f"/home/{getpass.getuser()}/.engflow/creds/engflow.key")
                with open(f"/home/{getpass.getuser()}/.bazelrc", "a") as bazelrc:
                    bazelrc.write(
                        f"build --tls_client_certificate=/home/{getpass.getuser()}/.engflow/creds/engflow.crt\n"
                    )
                    bazelrc.write(
                        f"build --tls_client_key=/home/{getpass.getuser()}/.engflow/creds/engflow.key\n"
                    )
            except OSError as exc:
                print(exc)
                print(
                    "Failed to update cert location, please move them manually. Otherwise you can pass 'BAZEL_FLAGS=\"--config=local\"' on the SCons command line."
                )

            return True

        # Pull the external hostname of the system from aws
        try:
            response = requests.get(
                "http://instance-data.ec2.internal/latest/meta-data/public-hostname")
            status_code = response.status_code
        except Exception as _:
            status_code = 500
        if status_code == 200:
            public_hostname = response.text
        else:
            public_hostname = "{{REPLACE_WITH_WORKSTATION_HOST_NAME}}"
        print(
            f"""\nERROR: ~/.engflow/creds/engflow.crt not found. Please reach out to #server-dev-platform if you need help with the steps below.

(If the below steps are not working, remote execution can be disabled by passing BAZEL_FLAGS=--config=local at the end of your scons.py invocation)

Please complete the following steps to generate a certificate:
- (If not in the Engineering org) Request access to the MANA group https://mana.corp.mongodbgov.com/resources/659ec4b9bccf3819e5608712
- Go to https://sodalite.cluster.engflow.com/gettingstarted (Uses mongodbcorp.okta.com auth URL)
- Login with OKTA, then click the \"GENERATE AND DOWNLOAD MTLS CERTIFICATE\" button
  - (If logging in with OKTA doesn't work) Login with Google using your MongoDB email, then click the "GENERATE AND DOWNLOAD MTLS CERTIFICATE" button
- On your local system (usually your MacBook), open a terminal and run:

ZIP_FILE=~/Downloads/engflow-mTLS.zip

curl https://raw.githubusercontent.com/mongodb/mongo/master/buildscripts/setup_engflow_creds.sh -o setup_engflow_creds.sh
chmod +x ./setup_engflow_creds.sh
./setup_engflow_creds.sh {getpass.getuser()} {public_hostname} $ZIP_FILE\n""")
        return False

    if not running_in_evergreen and \
        (not os.access(f"/home/{getpass.getuser()}/.engflow/creds/engflow.crt", os.R_OK) or
        not os.access(f"/home/{getpass.getuser()}/.engflow/creds/engflow.key", os.R_OK)):
        print(
            "Invalid permissions set on ~/.engflow/creds/engflow.crt or ~/.engflow/creds/engflow.key"
        )
        print("Please run the following command to fix the permissions:\n")
        print(
            f"sudo chown {getpass.getuser()}:{getpass.getuser()} /home/{getpass.getuser()}/.engflow/creds/engflow.crt /home/{getpass.getuser()}/.engflow/creds/engflow.key"
        )
        print(
            f"sudo chmod 600 /home/{getpass.getuser()}/.engflow/creds/engflow.crt /home/{getpass.getuser()}/.engflow/creds/engflow.key"
        )
        return False
    return True


def generate_bazel_info_for_ninja(env: SCons.Environment.Environment) -> None:
    # create a json file which contains all the relevant info from this generation
    # that bazel will need to construct the correct command line for any given targets
    ninja_bazel_build_json = {
        'bazel_cmd': Globals.bazel_base_build_command,
        'defaults': [str(t) for t in SCons.Script.DEFAULT_TARGETS],
        'targets': Globals.scons2bazel_targets
    }
    with open('.bazel_info_for_ninja.txt', 'w') as f:
        json.dump(ninja_bazel_build_json, f)

    # we also store the outputs in the env (the passed env is intended to be
    # the same main env ninja tool is constructed with) so that ninja can
    # use these to contruct a build node for running bazel where bazel list the
    # correct bazel outputs to be copied to the scons tree. We also handle
    # calculating the inputs. This will be the all the inputs of the outs,
    # but and input can not also be an output. If a node is found in both
    # inputs and outputs, remove it from the inputs, as it will be taken care
    # internally by bazel build.
    ninja_bazel_outs = []
    ninja_bazel_ins = []
    for scons_t, bazel_t in Globals.scons2bazel_targets.items():
        ninja_bazel_outs += [bazel_t['bazel_output']]
        ninja_bazel_ins += env.NinjaGetInputs(env.File(scons_t))
        if scons_t in ninja_bazel_ins:
            ninja_bazel_ins.remove(scons_t)

    # This is to be used directly by ninja later during generation of the ninja file
    env["NINJA_BAZEL_OUTPUTS"] = ninja_bazel_outs
    env["NINJA_BAZEL_INPUTS"] = ninja_bazel_ins


@retry(tries=5, delay=3)
def download_path_with_retry(*args, **kwargs):
    urllib.request.urlretrieve(*args, **kwargs)


install_query_cache = {}


def bazel_deps_check_query_cache(env, bazel_target):
    return install_query_cache.get(bazel_target, None)


def bazel_deps_add_query_cache(env, bazel_target, results):
    install_query_cache[bazel_target] = results


def sha256_file(filename: str) -> str:
    sha256_hash = hashlib.sha256()
    with open(filename, "rb") as f:
        for block in iter(lambda: f.read(4096), b""):
            sha256_hash.update(block)
        return sha256_hash.hexdigest()


def verify_s3_hash(s3_path: str, local_path: str) -> None:
    if s3_path not in _S3_HASH_MAPPING:
        raise Exception(
            f"S3 path not found in hash mapping, unable to verify downloaded for s3 path: s3_path")

    hash = sha256_file(local_path)
    if hash != _S3_HASH_MAPPING[s3_path]:
        raise Exception(
            f"Hash mismatch for {s3_path}, expected {_S3_HASH_MAPPING[s3_path]} but got {hash}")


def find_distro_match(distro_str: str) -> str:
    for distro_pattern, simplified_name in _DISTRO_PATTERN_MAP.items():
        if "*" in distro_pattern:
            prefix_suffix = distro_pattern.split("*")
            if distro_str.startswith(prefix_suffix[0]) and distro_str.endswith(prefix_suffix[1]):
                return simplified_name
        elif distro_str == distro_pattern:
            return simplified_name
    return None


time_auto_installing = 0
count_of_auto_installing = 0


def timed_auto_install_bazel(env, libdep, shlib_suffix):
    global time_auto_installing, count_of_auto_installing
    start_time = time.time()
    auto_install_bazel(env, libdep, shlib_suffix)
    time_auto_installing += time.time() - start_time
    count_of_auto_installing += 1


def auto_install_bazel(env, libdep, shlib_suffix):

    # we are only interested in queries for shared library thin targets
    if not str(libdep).endswith(shlib_suffix) or (
            libdep.has_builder() and libdep.get_builder().get_name(env) != "ThinTarget"):
        return

    bazel_target = env["SCONS2BAZEL_TARGETS"].bazel_target(libdep)
    bazel_libdep = env.File(f"#/{env['SCONS2BAZEL_TARGETS'].bazel_output(libdep)}")

    query_results = env.CheckBazelDepsCache(bazel_target)

    if query_results is None:
        bazel_query = ["cquery"] + env['BAZEL_FLAGS_STR'] + [
            f"kind('extract_debuginfo', deps(@{bazel_target}))", "--output=files"
        ]
        query_results = env.RunBazelQuery(bazel_query)
        if query_results.returncode == 0:
            env.AddBazelDepsCache(bazel_target, query_results)
        else:
            print("ERROR: bazel libdeps query failed:")
            print(query_results)
            print("\n\n*** Please ask about this in #ask-devprod-build channel. ***\n")
            sys.exit(1)

    # We are only interested in installing shared libs and their debug files, so for example
    # .so or .debug
    for line in query_results.stdout.splitlines():

        if not line.endswith(shlib_suffix):
            continue
        sep_dbg = env.subst("$SEPDBG_SUFFIX")
        if not sep_dbg or not line.endswith(sep_dbg):
            continue

    bazel_node = env.File(f"#/{line}")
    auto_install_mapping = env["AIB_SUFFIX_MAP"].get(shlib_suffix)
    new_installed_files = env.AutoInstall(
        auto_install_mapping.directory,
        bazel_node,
        AIB_COMPONENT="AIB_DEFAULT_COMPONENT",
        AIB_ROLE=auto_install_mapping.default_role,
        AIB_COMPONENTS_EXTRA=env.get("AIB_COMPONENTS_EXTRA", []),
    )

    if not new_installed_files:
        new_installed_files = getattr(bazel_node.attributes, "AIB_INSTALLED_FILES", [])
    installed_files = getattr(bazel_libdep.attributes, "AIB_INSTALLED_FILES", [])
    setattr(bazel_libdep.attributes, "AIB_INSTALLED_FILES", new_installed_files + installed_files)


def load_bazel_builders(env):
    # === Builders ===
    create_library_builder(env)
    create_program_builder(env)
    create_idlc_builder(env)

    if env.GetOption('ninja') != "disabled":
        env.NinjaRule("BAZEL_COPY_RULE", "$env$cmd", description="Copy from Bazel",
                      pool="local_pool")


ran_fetch = False


# Required boilerplate function
def exists(env: SCons.Environment.Environment) -> bool:

    # === Bazelisk ===
    global ran_fetch

    if not ran_fetch:
        ran_fetch = True

        def setup_bazel_thread():

            bazel_bin_dir = env.GetOption("evergreen-tmp-dir") if env.GetOption(
                "evergreen-tmp-dir") else os.path.expanduser("~/.local/bin")
            if not os.path.exists(bazel_bin_dir):
                os.makedirs(bazel_bin_dir)

            Globals.bazel_executable = install_bazel(bazel_bin_dir)

        Globals.bazel_fetch_thread = threading.Thread(target=setup_bazel_thread)
        Globals.bazel_fetch_thread.start()

    env.AddMethod(load_bazel_builders, "LoadBazelBuilders")
    return True


# Establishes logic for BazelLibrary build rule
def generate(env: SCons.Environment.Environment) -> None:

    if env.get("BAZEL_BUILD_ENABLED"):
        if env["BAZEL_INTEGRATION_DEBUG"]:
            global bazel_debug

            def bazel_debug_func(msg: str):
                print("[BAZEL_INTEGRATION_DEBUG] " + str(msg))

            bazel_debug = bazel_debug_func

        # this should be populated from the sconscript and include list of targets scons
        # indicates it wants to build
        env["SCONS_SELECTED_TARGETS"] = []

        # === Architecture/platform ===

        # Bail if current architecture not supported for Bazel:
        normalized_arch = platform.machine().lower().replace("aarch64", "arm64").replace(
            "x86_64", "amd64")
        normalized_os = sys.platform.replace("win32", "windows").replace("darwin", "macos")
        current_platform = f"{normalized_os}:{normalized_arch}:{env.ToolchainName()}"
        if current_platform not in _SUPPORTED_PLATFORM_MATRIX:
            raise Exception(
                f'Bazel not supported on this platform ({current_platform}); supported platforms are: [{", ".join(_SUPPORTED_PLATFORM_MATRIX)}]'
            )

        # === Build settings ===

        # We don't support DLL generation on Windows, but need shared object generation in dynamic-sdk mode
        # on linux.
        linkstatic = env.GetOption("link-model") in [
            "auto", "static"
        ] or (normalized_os == "windows" and env.GetOption("link-model") == "dynamic-sdk")

        allocator = env.get('MONGO_ALLOCATOR', 'tcmalloc-google')

        distro_or_os = normalized_os
        if normalized_os == "linux":
            distro_id = find_distro_match(f"{distro.name()} {distro.version()}")
            if distro_id is not None:
                distro_or_os = distro_id

        bazel_internal_flags = [
            f'--//bazel/config:compiler_type={env.ToolchainName()}',
            f'--//bazel/config:opt={env.GetOption("opt")}',
            f'--//bazel/config:dbg={env.GetOption("dbg") == "on"}',
            f'--//bazel/config:separate_debug={True if env.GetOption("separate-debug") == "on" else False}',
            f'--//bazel/config:libunwind={env.GetOption("use-libunwind")}',
            f'--//bazel/config:use_gdbserver={False if env.GetOption("gdbserver") is None else True}',
            f'--//bazel/config:spider_monkey_dbg={True if env.GetOption("spider-monkey-dbg") == "on" else False}',
            f'--//bazel/config:allocator={allocator}',
            f'--//bazel/config:use_lldbserver={False if env.GetOption("lldb-server") is None else True}',
            f'--//bazel/config:use_wait_for_debugger={False if env.GetOption("wait-for-debugger") is None else True}',
            f'--//bazel/config:use_ocsp_stapling={True if env.GetOption("ocsp-stapling") == "on" else False}',
            f'--//bazel/config:use_disable_ref_track={False if env.GetOption("disable-ref-track") is None else True}',
            f'--//bazel/config:use_wiredtiger={True if env.GetOption("wiredtiger") == "on" else False}',
            f'--//bazel/config:use_glibcxx_debug={env.GetOption("use-glibcxx-debug") is not None}',
            f'--//bazel/config:build_grpc={True if env["ENABLE_GRPC_BUILD"] else False}',
            f'--//bazel/config:use_libcxx={env.GetOption("libc++") is not None}',
            f'--//bazel/config:detect_odr_violations={env.GetOption("detect-odr-violations") is not None}',
            f'--//bazel/config:linkstatic={linkstatic}',
            f'--//bazel/config:use_diagnostic_latches={env.GetOption("use-diagnostic-latches") == "on"}',
            f'--//bazel/config:shared_archive={env.GetOption("link-model") == "dynamic-sdk"}',
            f'--//bazel/config:linker={env.GetOption("linker")}',
            f'--//bazel/config:streams_release_build={env.GetOption("streams-release-build") is not None}',
            f'--//bazel/config:build_enterprise={env.GetOption("modules") == "enterprise"}',
            f'--//bazel/config:visibility_support={env.GetOption("visibility-support")}',
            f'--platforms=//bazel/platforms:{distro_or_os}_{normalized_arch}_{env.ToolchainName()}',
            f'--host_platform=//bazel/platforms:{distro_or_os}_{normalized_arch}_{env.ToolchainName()}',
            '--compilation_mode=dbg',  # always build this compilation mode as we always build with -g
        ]

        if env["DWARF_VERSION"]:
            bazel_internal_flags.append(f"--//bazel/config:dwarf_version={env['DWARF_VERSION']}")

        if normalized_os == "macos":
            minimum_macos_version = "11.0" if normalized_arch == "arm64" else "10.14"
            bazel_internal_flags.append(f'--macos_minimum_os={minimum_macos_version}')

        http_client_option = env.GetOption("enable-http-client")
        if http_client_option is not None:
            if http_client_option in ["on", "auto"]:
                bazel_internal_flags.append(f'--//bazel/config:http_client=True')
            elif http_client_option == "off":
                bazel_internal_flags.append(f'--//bazel/config:http_client=False')

        sanitizer_option = env.GetOption("sanitize")

        if sanitizer_option is not None:
            options = sanitizer_option.split(",")
            formatted_options = [f'--//bazel/config:{_SANITIZER_MAP[opt]}=True' for opt in options]
            bazel_internal_flags.extend(formatted_options)

        # Disable RE for external developers and when executing on non-linux amd64/arm64 platforms
        is_external_developer = not os.path.exists("/opt/mongodbtoolchain")
        if normalized_os != "linux" or normalized_arch not in ["arm64", "amd64"
                                                               ] or is_external_developer:
            bazel_internal_flags.append('--config=local')

        # Disable remote execution for public release builds.
        if env.GetOption("release") == "on" and (
                env.GetOption("cache-dir") is None
                or env.GetOption("cache-dir") == "$BUILD_ROOT/scons/cache"):
            bazel_internal_flags.append('--config=public-release')

        evergreen_tmp_dir = env.GetOption("evergreen-tmp-dir")
        if normalized_os == "macos" and evergreen_tmp_dir:
            bazel_internal_flags.append(f"--sandbox_writable_path={evergreen_tmp_dir}")

        Globals.bazel_fetch_thread.join()
        Globals.bazel_base_build_command = [
            os.path.abspath(Globals.bazel_executable),
            'build',
        ] + bazel_internal_flags + shlex.split(env.get("BAZEL_FLAGS", ""))

        # Set the JAVA_HOME directories for ppc64le and s390x since their bazel binaries are not compiled with a built-in JDK.
        if normalized_arch == "ppc64le":
            Globals.bazel_env_variables[
                "JAVA_HOME"] = "/usr/lib/jvm/java-11-openjdk-11.0.4.11-2.el8.ppc64le"
        elif normalized_arch == "s390x":
            Globals.bazel_env_variables[
                "JAVA_HOME"] = "/usr/lib/jvm/java-11-openjdk-11.0.11.0.9-0.el8_3.s390x"

        # Store the bazel command line flags so scons can check if it should rerun the bazel targets
        # if the bazel command line changes.
        env['BAZEL_FLAGS_STR'] = bazel_internal_flags + [env.get("BAZEL_FLAGS", "")]

        if "--config=local" not in env['BAZEL_FLAGS_STR'] and "--config=public-release" not in env[
                'BAZEL_FLAGS_STR']:
            print(
                "Running bazel with remote execution enabled. To disable bazel remote execution, please add BAZEL_FLAGS=--config=local to the end of your scons command line invocation."
            )
            if not validate_remote_execution_certs(env):
                sys.exit(1)

        # We always use --compilation_mode debug for now as we always want -g, so assume -dbg location
        out_dir_platform = "$TARGET_ARCH"
        if normalized_os == "macos":
            out_dir_platform = "darwin_arm64" if normalized_arch == "arm64" else "darwin"
        elif normalized_os == "windows":
            out_dir_platform = "x64_windows"
        elif normalized_os == "linux" and normalized_arch == "amd64":
            # For c++ toolchains, bazel has some wierd behaviour where it thinks the default
            # cpu is "k8" which is another name for x86_64 cpus, so its not wrong, but abnormal
            out_dir_platform = "k8"
        elif normalized_arch == "ppc64le":
            out_dir_platform = "ppc"

        env["BAZEL_OUT_DIR"] = env.Dir(f"#/bazel-out/{out_dir_platform}-dbg/bin/")

        def print_total_query_time():
            global total_query_time, total_queries
            global time_auto_installing, count_of_auto_installing
            global total_libdeps_linking_time, count_of_libdeps_links
            bazel_debug(
                f"Bazel integration spent {total_query_time} seconds in total performing {total_queries} queries."
            )
            bazel_debug(
                f"Bazel integration spent {time_auto_installing} seconds in total performing {count_of_auto_installing} auto_install."
            )

        atexit.register(print_total_query_time)

        # === Builders ===
        load_bazel_builders(env)
        if env.GetOption('ninja') == "disabled":

            # ninja will handle the build so do not launch the bazel batch thread
            bazel_build_thread = threading.Thread(
                target=bazel_build_thread_func, args=(env.Dir("$BUILD_ROOT/scons/bazel").path,
                                                      env["VERBOSE"]))
            bazel_build_thread.start()

            def wait_for_bazel(env):
                nonlocal bazel_build_thread
                Globals.waiting_on_bazel_flag = True
                print("SCons done, switching to bazel build thread...")
                bazel_build_thread.join()
                if Globals.bazel_thread_terminal_output is not None:
                    Globals.bazel_thread_terminal_output.seek(0)
                    sys.stdout.write(Globals.bazel_thread_terminal_output.read())

            env.AddMethod(wait_for_bazel, "WaitForBazel")
        else:
            env.NinjaRule("BAZEL_COPY_RULE", "$env$cmd", description="Copy from Bazel",
                          pool="local_pool")

        env.AddMethod(generate_bazel_info_for_ninja, "GenerateBazelInfoForNinja")
        env.AddMethod(bazel_deps_check_query_cache, "CheckBazelDepsCache")
        env.AddMethod(bazel_deps_add_query_cache, "AddBazelDepsCache")
        env.AddMethod(bazel_query_func, 'RunBazelQuery')
        env.AddMethod(ninja_bazel_builder, "NinjaBazelBuilder")
        env.AddMethod(timed_auto_install_bazel, "BazelAutoInstall")

    else:
        env['BUILDERS']['BazelLibrary'] = env['BUILDERS']['Library']
        env['BUILDERS']['BazelProgram'] = env['BUILDERS']['Program']
        env['BUILDERS']['BazelIdlc'] = env['BUILDERS']['Idlc']
        env['BUILDERS']['BazelSharedArchive'] = env['BUILDERS']['SharedArchive']
