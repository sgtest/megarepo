#!/usr/bin/env bash

set -euf -o pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
REPO_DIR=$(pwd)

IMAGE_CONFIG_DIR="wolfi-images"
GCP_PROJECT="sourcegraph-ci"
GCS_BUCKET="package-repository"
TARGET_ARCH="x86_64"
MAIN_BRANCH="main"
BRANCH="${BUILDKITE_BRANCH:-'default-branch'}"
IS_MAIN=$([ "$BRANCH" = "$MAIN_BRANCH" ] && echo "true" || echo "false")

# shellcheck disable=SC2001
BRANCH_PATH=$(echo "$BRANCH" | sed 's/[^a-zA-Z0-9_-]/-/g')
if [[ "$IS_MAIN" != "true" ]]; then
  BRANCH_PATH="branches/$BRANCH_PATH"
fi

echo "~~~ :aspect: :stethoscope: Agent Health check"
/etc/aspect/workflows/bin/agent_health_check

aspectRC="/tmp/aspect-generated.bazelrc"
rosetta bazelrc >"$aspectRC"
export BAZELRC="$aspectRC"

if [ $# -eq 0 ]; then
  echo "No arguments supplied - provide the base image name to build"
  exit 0
fi

name=${1%/}
# Soft-fail if file doesn't exist, as CI step is triggered whenever base image configs are changed - including deletions/renames
if [ ! -f "wolfi-images/${name}.yaml" ]; then
  echo "File '${name}.yaml' does not exist cwd: '${PWD}'"
  exit 222
fi

# If this is a branch, check if branch-specific package repo exists on GCS
branch_repo_exists="false"
if [[ "$IS_MAIN" != "true" ]]; then
  dest_path="gs://$GCS_BUCKET/$BRANCH_PATH/$TARGET_ARCH/"
  if gsutil -q -u "$GCP_PROJECT" stat "${dest_path}APKINDEX.tar.gz"; then
    echo "A branch-specific package repo exists for this branch at ${dest_path}"
    branch_repo_exists="true"
  else
    echo "No branch-specific package repo exists for this branch, not updating apko configs"
  fi
fi

tag=${2-latest}

# On branches, if we modify a package then we'd like that modified version to be included in any base images built.
# This is a bit hacky, but we do this by modifying the base image configs and passing the branch-specific repo to apko.
add_custom_repo_cmd=()
modified_packages=()
if [[ "$IS_MAIN" != "true" && "$branch_repo_exists" == "true" ]]; then
  add_custom_repo_cmd=("--repository-append" "@branch https://packages.sgdev.org/$BRANCH_PATH" "--keyring-append" "https://packages.sgdev.org/sourcegraph-melange-dev.rsa.pub")
  echo "Adding custom repo command: ${add_custom_repo_cmd[*]}"

  # Read the branch-specific package repo and extract the names of packages that have been modified
  while IFS= read -r line; do
    modified_packages+=("$line")
  done < <(gsutil ls gs://package-repository/"$BRANCH_PATH"/x86_64/\*.apk | sed -E 's/.*\/x86_64\/([a-zA-Z0-9_-]+)-[0-9]+\..*/\1/')

  echo "List of modified packages to include in branch image: ${modified_packages[*]}"

  # In the base image configs, find and replace the packages which have been modified
  for element in "${modified_packages[@]}"; do
    echo "Replacing '$element@sourcegraph' with '$element@branch' in '${IMAGE_CONFIG_DIR}/${name}.yaml'"
    sed -i "s/$element@sourcegraph/$element@branch/g" "${IMAGE_CONFIG_DIR}/${name}.yaml"
  done

  echo -e "\nUpdated image config:"
  echo "------------"
  cat "${IMAGE_CONFIG_DIR}/${name}.yaml"
  echo -e "------------\n"
fi

#
# Build image

# Build base image with apko
# If add_custom_repo_cmd isn't empty
if [ ${#add_custom_repo_cmd[@]} -gt 0 ]; then
  echo " * Updated packages found, regenerating lockfile for base image '$name'..."
  bazel --bazelrc="$aspectRC" run //dev/sg -- wolfi lock "${add_custom_repo_cmd[@]}" "${name}"
fi

echo " * Building base image '$name' with apko..."
bazel --bazelrc="$aspectRC" run //dev/sg -- wolfi image "${name}"
local_image_name="${name}-base:latest"
remote_image_name="us.gcr.io/sourcegraph-dev/wolfi-${name}-base"

#
# Tag image and upload to GCP Artifact Registry

# Push to internal dev repo
echo "* Pushing image to internal dev repo..."
docker tag "${local_image_name}" "${remote_image_name}:${tag}"
docker push "${remote_image_name}:${tag}"
docker tag "${local_image_name}" "${remote_image_name}:latest"
docker push "${remote_image_name}:latest"

# Show image usage message on branches
if [[ "$IS_MAIN" != "true" ]]; then
  if [[ -n "$BUILDKITE" ]]; then
    mkdir -p ./annotations
    file="${name} image.md"
    cat <<-EOF >"${REPO_DIR}/annotations/${file}"

<strong>:octopus: ${name} image &bull; [View job output](#${BUILDKITE_JOB_ID})</strong>
<br />
<br />
Run the \`${name}\` base image locally using:

\`\`\`bash
docker pull us.gcr.io/sourcegraph-dev/wolfi-${name}-base:${tag}
\`\`\`

EOF

    # Add note if any packages were modified
    if [ ${#modified_packages[@]} -gt 0 ]; then
      cat <<-EOF >>"${REPO_DIR}/annotations/${file}"
NOTE: Any modified package will <strong>not</strong> be present in the image once merged - <a href="https://sourcegraph.com/docs/dev/how-to/wolfi/add_update_packages#update-an-existing-packaged-dependency">check the docs</a> for more details.
EOF
    fi
  fi
fi
