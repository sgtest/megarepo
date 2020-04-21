#!/usr/bin/env bash
#
# Build commands, optionally with or without race detector.  a list of every
# command we know about, to use by default
#
# This will install binaries into the `.bin` directory under the repository root.
#

all_oss_commands=" gitserver query-runner github-proxy searcher replacer frontend repo-updater symbols "

# handle options
verbose=false
while getopts 'v' o; do
  case $o in
    v) verbose=true ;;
    \?)
      echo >&2 "usage: go-install.sh [-v] [commands]"
      exit 1
      ;;
  esac
done
# shellcheck disable=SC2003
shift "$(expr $OPTIND - 1)"

# check provided commands
ok=true
case $# in
  0) commands=$all_oss_commands ;;
  *)
    commands=" $* "
    for cmd in $commands; do
      case $all_oss_commands in
        *" $cmd "*) ;;
        *)
          echo >&2 "unknown command: $cmd"
          ok=false
          ;;
      esac
    done
    ;;
esac

$ok || exit 1

# For the core Go packages, point $GOBIN to the final location.
# This must be done BEFORE building the target commands.
mkdir -p .bin
export GOBIN="${PWD}/.bin"
export GO111MODULE=on

INSTALL_GO_TOOLS=(
  "github.com/mattn/goreman@v0.3.4"
)

# Need to go to a temp directory for tools or we update our go.mod. We use
# GOPROXY=direct to avoid always consulting a proxy for dlv.
pushd "${TMPDIR:-/tmp}" >/dev/null || exit 1
if ! GOPROXY=direct go get -v "${INSTALL_GO_TOOLS[@]}" 2>go-install.log; then
  cat go-install.log
  echo >&2 "failed to install prerequisite tools, aborting."
  exit 1
fi
popd >/dev/null || exit 1

INSTALL_GO_PKGS=(
  "github.com/mattn/goreman"
  "github.com/google/zoekt/cmd/zoekt-archive-index"
  "github.com/google/zoekt/cmd/zoekt-sourcegraph-indexserver"
  "github.com/google/zoekt/cmd/zoekt-webserver"
)

if ! go install "${INSTALL_GO_PKGS[@]}"; then
  echo >&2 "failed to install prerequisite packages, aborting."
  exit 1
fi

# For the target commands, build into a temp directory for comparison, so that
# we can update only those packages that change. Clean up the temp at exit.
tmpdir="$(mktemp -d -t src-binaries.XXXXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT
export GOBIN="$tmpdir"

TAGS='dev'
if [ -n "$DELVE" ]; then
  echo >&2 'Building with optimizations disabled (for debugging). Make sure you have at least go1.10 installed.'
  GCFLAGS='all=-N -l'
  TAGS="$TAGS delve"
fi

# build a list of "cmd,true" and "cmd,false" pairs to indicate whether each command
# wants its own flags. we can't use variable names with the command in them because
# some commands have hyphens.
raced=()
unraced=()
case $GORACED in
  "all")
    for cmd in $commands; do
      raced+=("$cmd")
    done
    ;;
  *)
    for cmd in $commands; do
      case " $GORACED " in
        *" $cmd "*)
          raced+=("$cmd")
          ;;
        *)
          unraced+=("$cmd")
          ;;
      esac
    done
    ;;
esac

# Shared logic for the go install part
do_install() {
  race=$1
  shift
  cmdlist="$*"
  cmds=()
  for cmd in $cmdlist; do
    replaced=false
    for enterpriseCmd in $ENTERPRISE_COMMANDS; do
      if [ "$cmd" == "$enterpriseCmd" ]; then
        cmds+=("github.com/sourcegraph/sourcegraph/enterprise/cmd/$enterpriseCmd")
        replaced=true
      fi
    done
    if [ $replaced == false ]; then
      cmds+=("github.com/sourcegraph/sourcegraph/cmd/$cmd")
    fi
  done
  if (go install -v -gcflags="$GCFLAGS" -tags "$TAGS" -race="$race" "${cmds[@]}"); then
    for cmd in $cmdlist; do
      # Check whether the binary of each command has changed
      if ! cmp -s "${GOBIN}/${cmd}" "${PWD}/.bin/${cmd}"; then
        # Binary updated. Move it to correct location.
        mv "${GOBIN}/${cmd}" "${PWD}/.bin/${cmd}"

        if $verbose; then
          echo "$cmd"
        fi
      fi
    done
  else
    failed="$failed $cmdlist"
  fi
}

if [ ${#raced[@]} -ge 1 ]; then
  echo >&2 "Go race detector enabled for: $GORACED."
  do_install true "${raced[@]}"
else
  echo >&2 "Go race detector disabled. You can enable it for specific commands by setting GORACED (e.g. GORACED=frontend,searcher or GORACED=all for all commands)"
fi

if [ ${#unraced[@]} -ge 1 ]; then
  do_install false "${unraced[@]}"
fi

if [ -n "$failed" ]; then
  echo >&2 "failed to build:$failed"
  exit 1
fi
