#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset

ci_dir=$(cd $(dirname $0) && pwd)
. "$ci_dir/shared.sh"

REPO_DIR="$1"
CACHE_DIR="$2"

cache_src_dir="$CACHE_DIR/src"

if [ ! -d "$REPO_DIR" -o ! -d "$REPO_DIR/.git" ]; then
    echo "Error: $REPO_DIR does not exist or is not a git repo"
    exit 1
fi
cd $REPO_DIR
if [ ! -d "$CACHE_DIR" ]; then
    echo "Error: $CACHE_DIR does not exist or is not an absolute path"
    exit 1
fi

rm -rf "$CACHE_DIR"
mkdir "$CACHE_DIR"

# On the beta channel we'll be automatically calculating the prerelease version
# via the git history, so unshallow our shallow clone from CI.
if [ "$(releaseChannel)" = "beta" ]; then
  git fetch origin --unshallow beta master
fi

# Duplicated in docker/dist-various-2/shared.sh
function fetch_github_commit_archive {
    local module=$1
    local cached="download-${module//\//-}.tar.gz"
    retry sh -c "rm -f $cached && \
        curl -f -sSL -o $cached $2"
    mkdir $module
    touch "$module/.git"
    # On Windows, the default behavior is to emulate symlinks by copying
    # files. However, that ends up being order-dependent while extracting,
    # which can cause a failure if the symlink comes first. This env var
    # causes tar to use real symlinks instead, which are allowed to dangle.
    export MSYS=winsymlinks:nativestrict
    tar -C $module --strip-components=1 -xf $cached
    rm $cached
}

# Archive downloads are temporarily disabled due to sudden 504
# gateway timeout errors.
# included="src/llvm-project src/doc/book src/doc/rust-by-example"
included=""
modules="$(git config --file .gitmodules --get-regexp '\.path$' | cut -d' ' -f2)"
modules=($modules)
use_git=""
urls="$(git config --file .gitmodules --get-regexp '\.url$' | cut -d' ' -f2)"
urls=($urls)
# shellcheck disable=SC2068
for i in ${!modules[@]}; do
    module=${modules[$i]}
    if [[ " $included " = *" $module "* ]]; then
        commit="$(git ls-tree HEAD $module | awk '{print $3}')"
        git rm $module
        url=${urls[$i]}
        url=${url/\.git/}
        fetch_github_commit_archive $module "$url/archive/$commit.tar.gz" &
        bg_pids[${i}]=$!
        continue
    else
        use_git="$use_git $module"
    fi
done
retry sh -c "git submodule deinit -f $use_git && \
    git submodule sync && \
    git submodule update -j 16 --init --recursive --depth 1 $use_git"
# STATUS=0
# for pid in ${bg_pids[*]}
# do
#     wait $pid || STATUS=1
# done
# exit ${STATUS}
