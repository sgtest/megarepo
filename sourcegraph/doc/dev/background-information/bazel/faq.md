# FAQ

## General

### I just used `[no-bazel]` to merge my PR

While using `[no-bazel]` will enable you to get your pull request merged, the subsequent builds will be with Bazel unless they also have that flag.

Therefore you need to follow-up quickly with a fix to ensure `main` is not broken.

### The analysis cache is being busted because of `--action_env`

Typically you'll see this (in CI or locally):

```
INFO: Build option --action_env has changed, discarding analysis cache.
```

- If you added a `build --action_env=VAR` to one of the `bazelrc`s, and `$VAR` is not stable across builds, it will break the analysis cache. You should never pass a variable that is not stable, otherwise, the cache being busted is totally expected and there is no way around it.
  - Use `build --action_env=VAR=123` instead to pin it down if it's not stable in your environment.
- If you added a `test --action_env=VAR`, running `bazel build [...]` will have a different `--action_env` and because the analysis cache is the same for `build` and `test` that will automatically bust the cache.
  - Use `build --test_env=VAR` instead, so that env is used only in tests, and doesn't affect builds, while avoiding to bust the cache.

### My JetBrains IDE becomes unresponsive after Bazel builds

By default, JetBrains IDEs such as GoLand will try and index the files in your project workspace. If you run Bazel locally, the resulting artifacts will be indexed, which will likely hog the full heap size that the IDE is allocated.

There is no reason to index these files, so you can just exclude them from indexing by right-clicking artifact directories, then choosing **Mark directory as** &rarr; **Excluded** from the context menu. A restart is required to stop the indexing process.

### My local `bazel configure` or `./dev/ci/bazel-configure.sh` run has diff with a result of Bazel CI step

This could happen when there are any files which are not tracked by Git. These files affect the run of `bazel configure` and typically add more items to `BUILD.bazel` file.

Solution: run `git clean -ffdx` then run `bazel configure` again.

### How do I clean up all local Bazel cache?

1. The simplest way to clean up the cache is to use the clean command: `bazel clean`. This command will remove all output files and the cache in the bazel-* directories within your workspace. Use the `--expunge` flag to remove the entire working tree, including the cache directory, and force a full rebuild.
2. To manually clear the global Bazel cache, you need to remove the respective folders from your machine. On macOS, the global cache is typically located at either `~/.cache/bazel` or `/var/tmp/_bazel_$(whoami)`.

### Where do I fine Bazel rules locally on disk?

Use `bazel info output_base` to find the output base directory. From there go to the `external` folder to find Bazel rules saved locally.

### How do I build a container on MacOS 

Our containers are only built for `linux/amd64`, therefore we need to cross-compile on MacOS to produce correct images. To simplify this process, a configuration flag is available: `--config darwin-docker` to swap the toolchains for you. 

Example: 

```
# Create a tarball that can be loaded in Docker of the worker service:
bazel build //cmd/worker:image_tarball --config darwin-docker

# Load the image in Docker: 
docker load --input $(bazel cquery //cmd/worker:image_tarball  --config darwin-docker --output=files)
```

You can also use the same configuration flag to run the container tests on MacOS: 

```
bazel test //cmd/worker:image_test --config darwin-docker
```

### Can I run integration tests (`bazel test //testing/...`) locally? 

At the time of writing this documentation, it's not possible to do so, because we need to cross compile to produce `linux/amd64` container images, but the test runners need to run against your host architecture. If your host isn't `linux/amd64` you won't be able to run those tests. 

This is caused by the fact that there is no straightforward way of telling Bazel to use a given toolchain for certain targets and another one for others, in a consistent fashion across the various binaries we produce (rust+go).

See [this issue](https://github.com/sourcegraph/sourcegraph/issues/52914) to track progress on this particular problem. 

### When using nix, when protobuf gets compiled I get a C/C++ compiler issue `fatal error: 'algorithm' file not found` or some core header files are not found
Nix sets the CC environment variable to a clang version use by nix which is independent of the host system. You can verify this by running the following commands in your nix shell.
```
$ echo $CC
clang

$ which $CC
/nix/store/agjhf1m0xsvmdjkk8kc7bp3pic9lsfrb-clang-wrapper-11.1.0/bin/clang

$ cat bazel-sourcegraph/external/local_config_cc/cc_wrapper.sh | grep "# Call the C++ compiler" -A 2
# Call the C++ compiler
/nix/store/agjhf1m0xsvmdjkk8kc7bp3pic9lsfrb-clang-wrapper-11.1.0/bin/clang "$@"
```
Bazel runs a target called `locate_cc_config` which adheres to the CC environment variable. The variable defines the compiler to be used to perform C/C++ compilation. At time of writing, the compiler is incorrectly configured and the stdlib doesn't get referenced properly. Therefore, we currently recommend to unset the `CC` variable in your nix shell. The `locate_cc_config` will then find the system C/C++ compiler (which on my system resolved to `/usr/bin/gcc`) and compile protobuf.

You can also verify that the correct compiler is used by running the following command:
```
cat bazel-sourcegraph/external/local_config_cc/cc_wrapper.sh | grep "# Call the C++ compiler" -A 2
# Call the C++ compiler
/usr/bin/gcc "$@"
```

### On MacOS, you get `Bad CPU type for executable` when running Bazel

This typically happens when `bazel` tries to use `protoc` to compile protobuf definitions and you're on an arm64 mac. The full error will look like the following:
```
src/main/tools/process-wrapper-legacy.cc:80: "execvp(bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/com_google_protobuf_protoc_macos_aarch64/protoc.exe, ...)": Bad CPU type in executable
ERROR: /private/var/tmp/_bazel_ec2-user/c27d26456d2e68ea0aaccfcda2d35b4e/external/go_googleapis/google/api/BUILD.bazel:22:14: Generating Descriptor Set proto_library @go_googleapis//google/api:api_proto failed: (Exit 1): protoc.exe failed: error executing command (from target @go_googleapis//google/api:api_proto) bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/com_google_protobuf_protoc_macos_aarch64/protoc.exe --direct_dependencies google/api/launch_stage.proto ... (remaining 5 arguments skipped)
```

If you run the `file` utility on the `protoc` binary you'll see the CPU architecture mismatch.
```
file bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/com_google_protobuf_protoc_macos_aarch64/protoc.exe
bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/com_google_protobuf_protoc_macos_aarch64/protoc.exe: Mach-O 64-bit executable x86_64
```

To fix this, you need to install Rosetta 2. There are various ways of doing that, but below is the CLI way to do it.
```
/usr/sbin/softwareupdate --install-rosetta --agree-to-license
```

__Tested on Darwin 22.3.0 Darwin Kernel Version 22.3.0__

## Queries 

Bazel queries (`bazel query`, `bazel cquery` and `bazel aqueries`) are powerful tools that can assist you to visualize dependencies and understand how targets are being built or tested. 

### Errors about not being able to fetch a manifest for an OCI rule 

When running the following query: 

```
bazel query 'kind("go_binary", rdeps(//..., //internal/database/migration/cliutil))'
```

We get the following error. 

```
WARNING: Could not fetch the manifest. Either there was an authentication issue or trying to pull an image with OCI image media types. 
Falling back to using `curl`. See https://github.com/bazelbuild/bazel/issues/17829 for the context.
INFO: Repository wolfi_redis_base_single instantiated at:
  /home/noah/Sourcegraph/sourcegraph/WORKSPACE:376:9: in <toplevel>
  /home/noah/Sourcegraph/sourcegraph/dev/oci_deps.bzl:73:13: in oci_deps
  /home/noah/.cache/bazel/_bazel_noah/8fd1d20666a46767e7f29541678514a0/external/rules_oci/oci/pull.bzl:133:18: in oci_pull
Repository rule oci_pull defined at:
  /home/noah/.cache/bazel/_bazel_noah/8fd1d20666a46767e7f29541678514a0/external/rules_oci/oci/private/pull.bzl:434:27: in <toplevel>
WARNING: Download from https://us.gcr.io/v2/sourcegraph-dev/wolfi-redis-base/manifests/sha256:08e80c858fe3ef9b5ffd1c4194a771b6fd45f9831ad40dad3b5f5b53af880582 failed: class com.google.devtools.build.lib.bazel.repository.downloader.UnrecoverableHttpException GET returned 401 Unauthorized
ERROR: An error occurred during the fetch of repository 'wolfi_redis_base_single':
   Traceback (most recent call last):
        File "/home/noah/.cache/bazel/_bazel_noah/8fd1d20666a46767e7f29541678514a0/external/rules_oci/oci/private/pull.bzl", line 357, column 46, in _oci_pull_impl
                mf, mf_len = downloader.download_manifest(rctx.attr.identifier, "manifest.json")
```

This query requires to analyse external dependencies for the [base images](containers.md), which are not yet publicly available.  

Solution: `gcloud auth configure-docker us.gcr.io` to get access to the registry. 

Solution: Pass the `--keep_going` additional flag to your `bazel query` command, so the evaluation doesn't stop at the first error. 

## Go

### It complains about some missing symbols, but I'm sure they are there since I can see my files

```
ERROR: /Users/tech/work/sourcegraph/internal/redispool/BUILD.bazel:3:11: GoCompilePkg internal/redispool/redispool.a failed: (Exit 1): builder failed: error executing command (from target //internal/redispool:redispool) bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/go_sdk/builder compilepkg -sdk external/go_sdk -installsuffix darwin_arm64 -src internal/redispool/redispool.go -src internal/redispool/sysreq.go ... (remaining 30 arguments skipped)

Use --sandbox_debug to see verbose messages from the sandbox and retain the sandbox build root for debugging
internal/redispool/redispool.go:78:13: undefined: RedisKeyValue
internal/redispool/redispool.go:94:13: undefined: RedisKeyValue
```

OR

```
~/work/sourcegraph U bzl/build-go $ bazel build //dev/sg
INFO: Analyzed target //dev/sg:sg (955 packages loaded, 16719 targets configured).
INFO: Found 1 target...
ERROR: /Users/tech/work/sourcegraph/internal/conf/confdefaults/BUILD.bazel:3:11: GoCompilePkg internal/conf/confdefaults/confdefaults.a failed: (Exit 1): builder failed: error executing command (from target //internal/conf/confdefaults:confdefaults) bazel-out/darwin_arm64-opt-exec-2B5CBBC6/bin/external/go_sdk/builder compilepkg -sdk external/go_sdk -installsuffix darwin_arm64 -src internal/conf/confdefaults/confdefaults.go -embedroot '' -embedroot ... (remaining 19 arguments skipped)

Use --sandbox_debug to see verbose messages from the sandbox and retain the sandbox build root for debugging
compilepkg: missing strict dependencies:
	/private/var/tmp/_bazel_tech/3eea80c6015362974b7d423d1f30cb62/sandbox/darwin-sandbox/10/execroot/__main__/internal/conf/confdefaults/confdefaults.go: import of "github.com/russellhaering/gosaml2/uuid"
No dependencies were provided.
Check that imports in Go sources match importpath attributes in deps.
Target //dev/sg:sg failed to build
Use --verbose_failures to see the command lines of failed build steps.
INFO: Elapsed time: 11.559s, Critical Path: 2.93s
INFO: 36 processes: 2 internal, 34 darwin-sandbox.
```


Solution: run `bazel configure` to update the buildfiles automatically.

### My go tests complains about missing testdata

In the case where your testdata lives in `../**`, Gazelle cannot see those on its own, and you need to create a filegroup manually, see https://github.com/sourcegraph/sourcegraph/pull/47605/commits/93c838aad5436dc69f6695cec933bfb84b8ba59a

### Manually adding a `go_repository`

Sometimes Gazelle won't be able to generate a `go_repository` for your dependency and you'll need to fill in the attributes yourself. Most of the fields are easy to get, except when you need to provide values for the sum and version.

To retrieve these values:
1. Create a go.mod in the directory where the dependency is imported.
2. Run `go mod tidy`. This will populate the `go.mod` file and also generate a `go.sum` file.
3. You can then locate the version you should use for `go_repository` from the `go.mod` file and the sum from the `go.sum` file.
4. Delete the `go.mod` and `go.sum` files as they're no longer needed.

### How to update to the latest recommended bazelrc?

```
bazel run //.aspect/bazelrc:update_aspect_bazelrc_presets
```

## Rust

### I'm getting `Error in path: Not a regular file: docker-images/syntax-highlighter/Cargo.Bazel.lock` when I try to build `syntax-highlighter`

Below is a full example of this error:
```
ERROR: An error occurred during the fetch of repository 'crate_index':
   Traceback (most recent call last):
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/crates_repository.bzl", line 34, column 30, in _crates_repository_impl
                lockfiles = get_lockfiles(repository_ctx)
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/generate_utils.bzl", line 311, column 36, in get_lockfiles
                bazel = repository_ctx.path(repository_ctx.attr.lockfile) if repository_ctx.attr.lockfile else None,
Error in path: Not a regular file: /Users/william/code/sourcegraph/docker-images/syntax-highlighter/Cargo.Bazel.lock
ERROR: /Users/william/code/sourcegraph/WORKSPACE:197:18: fetching crates_repository rule //external:crate_index: Traceback (most recent call last):
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/crates_repository.bzl", line 34, column 30, in _crates_repository_impl
                lockfiles = get_lockfiles(repository_ctx)
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/generate_utils.bzl", line 311, column 36, in get_lockfiles
                bazel = repository_ctx.path(repository_ctx.attr.lockfile) if repository_ctx.attr.lockfile else None,
Error in path: Not a regular file: /Users/william/code/sourcegraph/docker-images/syntax-highlighter/fake.lock
ERROR: Error computing the main repository mapping: no such package '@crate_index//': Not a regular file: /Users/william/code/sourcegraph/docker-images/syntax-highlighter/Cargo.Bazel.lock
```
The error happens when the file specified in the lockfiles attribute of `crates_repository` (see WORKSPACE file for the definition) does not exist on disk. Currently this rule does not generate the file, instead it just generates the _content_ of the file. So to get passed this error you should create the file `touch docker-images/syntax-highlighter/Cargo.Bazel.lock`. With the file create it we can now populate `Cargo.Bazel.lock` with content using bazel by running `CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index`.

### When I build `syntax-highlighter` it complains that the current `lockfile` is out of date
The error will look like this:
```
INFO: Repository crate_index instantiated at:
  /Users/william/code/sourcegraph/WORKSPACE:197:18: in <toplevel>
Repository rule crates_repository defined at:
  /private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/crates_repository.bzl:106:36: in <toplevel>
INFO: repository @crate_index' used the following cache hits instead of downloading the corresponding file.
 * Hash 'dc2d47b42cbe92ebdb144555603dad08eae505fc459bae5e2503647919067ac8' for https://github.com/bazelbuild/rules_rust/releases/download/0.16.1/cargo-bazel-aarch64-apple-darwin
If the definition of 'repository @crate_index' was updated, verify that the hashes were also updated.
ERROR: An error occurred during the fetch of repository 'crate_index':
   Traceback (most recent call last):
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/crates_repository.bzl", line 45, column 28, in _crates_repository_impl
                repin = determine_repin(
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/generate_utils.bzl", line 374, column 13, in determine_repin
                fail(("\n".join([
Error in fail: Digests do not match: Digest("3e9e0f927c955efa39a58c472a2eac60e3f89a7f3eafc7452e9acf23adf8ce5a") != Digest("ef858ae49063d5c22e0ee0b7632a8ced4994315395b17fb3c61f3e6bfb6deb27")

The current `lockfile` is out of date for 'crate_index'. Please re-run bazel using `CARGO_BAZEL_REPIN=true` if this is expected and the lockfile should be updated.
ERROR: /Users/william/code/sourcegraph/WORKSPACE:197:18: fetching crates_repository rule //external:crate_index: Traceback (most recent call last):
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/crates_repository.bzl", line 45, column 28, in _crates_repository_impl
                repin = determine_repin(
        File "/private/var/tmp/_bazel_william/c92ec739369034d3064b6df55c419545/external/rules_rust/crate_universe/private/generate_utils.bzl", line 374, column 13, in determine_repin
                fail(("\n".join([
Error in fail: Digests do not match: Digest("3e9e0f927c955efa39a58c472a2eac60e3f89a7f3eafc7452e9acf23adf8ce5a") != Digest("ef858ae49063d5c22e0ee0b7632a8ced4994315395b17fb3c61f3e6bfb6deb27")

The current `lockfile` is out of date for 'crate_index'. Please re-run bazel using `CARGO_BAZEL_REPIN=true` if this is expected and the lockfile should be updated.
ERROR: Error computing the main repository mapping: no such package '@crate_index//': Digests do not match: Digest("3e9e0f927c955efa39a58c472a2eac60e3f89a7f3eafc7452e9acf23adf8ce5a") != Digest("ef858ae49063d5c22e0ee0b7632a8ced4994315395b17fb3c61f3e6bfb6deb27")

The current `lockfile` is out of date for 'crate_index'. Please re-run bazel using `CARGO_BAZEL_REPIN=true` if this is expected and the lockfile should be updated.
```
Bazel uses a separate lock file to keep track of the dependencies and needs to be updated. To update the `lockfile` run `CARGO_BAZEL_REPIN=1 CARGO_BAZEL_REPIN_ONLY=crate_index bazel sync --only=crate_index`. This command takes a while to execute as it fetches all the dependencies specified in `Cargo.lock` and populates `Cargo.Bazel.lock`.

### `syntax-highlighter` fails to build and has the error `failed to resolve: use of undeclared crate or module`
The error looks something like this:
```
error[E0433]: failed to resolve: use of undeclared crate or module `scip_treesitter_languages`
  --> docker-images/syntax-highlighter/src/main.rs:56:5
   |
56 |     scip_treesitter_languages::highlights::CONFIGURATIONS
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared crate or module `scip_treesitter_languages`

error[E0433]: failed to resolve: use of undeclared crate or module `scip_treesitter_languages`
  --> docker-images/syntax-highlighter/src/main.rs:57:15
   |
57 |         .get(&scip_treesitter_languages::parsers::BundledParser::Go);
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^ use of undeclared crate or module `scip_treesitter_languages`

error: aborting due to 2 previous errors
```
Bazel doesn't know about the module/crate being use in the rust code. If you do a git blame `Cargo.toml` you'll probably see that a new dependency has been added, but the build files were not updated. There are two ways to solve this:
1. Run `bazel configure` and `CARGO_BAZEL_REPIN=1 CARGO_BAZEL_REPIN_ONLY=crate_index bazel sync --only=crate_index`. Once the commands have completed you can check that the dependency has been picked up and syntax-highlighter can be built by running `bazel build //docker-images/syntax-highlighter/...`. **Note** this will usually work if the dependency is an *external* dependency.
2. You're going to have to update the `BUILD.bazel` file yourself. Which one you might ask? From the above error we can see the file `src/main.rs` is where the error is encountered, so we need to tell *its BUILD.bazel* about the new dependency.
For the above dependency, the crate is defined in `docker-images/syntax-highlighter/crates`. You'll also see that each of those crates have their own `BUILD.bazel` files in them, which means we can reference them as targets! Take a peak at `scip-treesitter-languages` `BUILD.bazel` file and take note of the name - that is its target. Now that we have the name of the target we can add it as a dep to `docker-images/syntax-highlighter`. In the snippet below the `syntax-highlighter` `rust_binary` rule is updated with the `scip-treesitter-languages` dependency. Note that we need to refer to the full target path when adding it to the dep list in the `BUILD.bazel` file.
```
rust_binary(
    name = "syntect_server",
    srcs = ["src/main.rs"],
    aliases = aliases(),
    proc_macro_deps = all_crate_deps(
        proc_macro = True,
    ),
    deps = all_crate_deps(
        normal = True,
    ) + [
        "//docker-images/syntax-highlighter/crates/sg-syntax:sg-syntax",
        "//docker-images/syntax-highlighter/crates/scip-treesitter-languages:scip-treesitter-languages",
    ],
)
```

