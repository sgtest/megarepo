# Experimental support for developing in nix. Please reach out to @keegan or @noah if
# you encounter any issues.
#
# Things it does differently:
#
# - Runs postgres under ~/.sourcegraph with a unix socket. No need to manage a
#   service. Must remember to run "pg_ctl stop" if you want to stop it.
#
# Status: everything works on linux & darwin.
{ pkgs }:
let
  # pkgs.universal-ctags installs the binary as "ctags", not "universal-ctags"
  # like zoekt expects.
  universal-ctags = pkgs.writeShellScriptBin "universal-ctags" ''
    exec ${pkgs.universal-ctags}/bin/ctags "$@"
  '';

  # On darwin, we let bazelisk manage the bazel version since we actually need to run two
  # different versions thanks to aspect. Additionally bazelisk allows us to do
  # things like "bazel configure". So we just install a script called bazel
  # which calls bazelisk.
  #
  # Additionally bazel seems to break when CC and CXX is set to a nix managed
  # compiler on darwin. So the script unsets those.
  bazel-wrapper = pkgs.writeShellScriptBin "bazel" (if pkgs.hostPlatform.isMacOS then ''
    unset CC CXX
    exec ${pkgs.bazelisk}/bin/bazelisk "$@"
  '' else ''
    if [ "$1" == "configure" ]; then
      exec env --unset=USE_BAZEL_VERSION ${pkgs.bazelisk}/bin/bazelisk "$@"
    fi
    exec ${pkgs.bazel_6}/bin/bazel "$@"
  '');
  bazel-watcher = pkgs.writeShellScriptBin "ibazel" ''
    ${pkgs.lib.optionalString pkgs.hostPlatform.isMacOS "unset CC CXX"}
    exec ${pkgs.bazel-watcher}/bin/ibazel \
      ${pkgs.lib.optionalString pkgs.hostPlatform.isLinux "-bazel_path=${bazel-fhs}/bin/bazel"} "$@"
  '';
  bazel-fhs = pkgs.buildFHSEnv {
    name = "bazel";
    runScript = "bazel";
    targetPkgs = pkgs: (with pkgs; [
      bazel-wrapper
      zlib.dev
    ]);
    # unsharePid required to preserve bazel server between bazel invocations,
    # the rest are disabled just in case
    unsharePid = false;
    unshareUser = false;
    unshareIpc = false;
    unshareNet = false;
    unshareUts = false;
    unshareCgroup = false;
  };
in
pkgs.mkShell {
  name = "sourcegraph-dev";

  # The packages in the `buildInputs` list will be added to the PATH in our shell
  nativeBuildInputs = with pkgs; [
    # nix language server
    nil

    # Our core DB.
    postgresql_13

    # Cache and some store data
    redis

    # Used by symbols and zoekt-git-index to extract symbols from sourcecode.
    universal-ctags

    # Build our backend.
    go_1_20

    # Lots of our tooling and go tests rely on git et al.
    git
    git-lfs
    parallel
    nssTools

    # CI lint tools you need locally
    shfmt
    shellcheck

    # Web tools. Need node 16.7 so we use unstable. Yarn should also be built against it.
    nodejs-16_x
    (nodejs-16_x.pkgs.pnpm.override {
      version = "8.1.0";
      src = fetchurl {
        url = "https://registry.npmjs.org/pnpm/-/pnpm-8.1.0.tgz";
        sha512 = "sha512-e2H73wTRxmc5fWF/6QJqbuwU6O3NRVZC1G1WFXG8EqfN/+ZBu8XVHJZwPH6Xh0DxbEoZgw8/wy2utgCDwPu4Sg==";
      };
    })
    nodePackages.typescript

    # Rust utils for syntax-highlighter service, currently not pinned to the same versions.
    cargo
    rustc
    rustfmt
    libiconv
    clippy

    # special sauce bazel stuff.
    bazelisk # needed to please sg, but not used directly by us
    (if pkgs.hostPlatform.isLinux then bazel-fhs else bazel-wrapper)
    bazel-watcher
    bazel-buildtools
  ];

  # Startup postgres, redis & set nixos specific stuff
  shellHook = ''
    set -h # command hashmap is not guaranteed to be enabled, but required by sg
    . ./dev/nix/shell-hook.sh
  '';

  # Fix for using Delve https://github.com/sourcegraph/sourcegraph/pull/35885
  hardeningDisable = [ "fortify" ];

  # By explicitly setting this environment variable we avoid starting up
  # universal-ctags via docker.
  CTAGS_COMMAND = "${universal-ctags}/bin/universal-ctags";

  RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";

  DEV_WEB_BUILDER = "esbuild";

  # Some of the bazel actions require some tools assumed to be in the PATH defined by the "strict action env" that we enable
  # through --incompatible_strict_action_env. We can poke a custom PATH through with --action_env=PATH=$BAZEL_ACTION_PATH.
  # See https://sourcegraph.com/github.com/bazelbuild/bazel@6.1.2/-/blob/src/main/java/com/google/devtools/build/lib/bazel/rules/BazelRuleClassProvider.java?L532-547
  BAZEL_ACTION_PATH = with pkgs; lib.makeBinPath [ bash stdenv.cc coreutils unzip zip curl gzip gnutar git patch openssh ];

  # bazel complains when the bazel version differs even by a patch version to whats defined in .bazelversion,
  # so we tell it to h*ck off here.
  # https://sourcegraph.com/github.com/bazelbuild/bazel@1a4da7f331c753c92e2c91efcad434dc29d10d43/-/blob/scripts/packages/bazel.sh?L23-28
  USE_BAZEL_VERSION =
    if pkgs.hostPlatform.isMacOS then "" else pkgs.bazel_6.version;
}
