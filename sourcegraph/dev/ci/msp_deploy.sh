#!/usr/bin/env bash

set -eux -o pipefail

# Service Specific Parameters
: "${MSP_SERVICE_ID:?"MSP_SERVICE_ID is required"}"
: "${GCP_PROJECT:?"GCP_PROJECT is required"}"
: "${GCP_REGION:?"GCP_REGION is required"}"
: "${REPOSITORY:?"REPOSITORY is required"}"

# CI Variables
: "${BUILDKITE_BUILD_NUMBER:?"BUILDKITE_BUILD_NUMBER is required"}"
: "${BUILDKITE_COMMIT:?"BUILDKITE_COMMIT is required"}"
: "${BUILDKITE_BUILD_AUTHOR:?"BUILDKITE_BUILD_AUTHOR is required"}"

# Computed Variables
GCP_CLOUDRUN_SKAFFOLD_SOURCE="gs://${GCP_PROJECT}-cloudrun-skaffold/source.tar.gz"
GCP_DELIVERY_PIPELINE="${MSP_SERVICE_ID}-${GCP_REGION}-rollout"
SHORT_SHA="${BUILDKITE_COMMIT:0:12}"
TAG="${SHORT_SHA}_${BUILDKITE_BUILD_NUMBER}"
COMMIT_MESSAGE_BASE64="$(git log -n 1 --pretty=format:'%s' | base64)"
# resource ids must be lower-case letters, numbers, and hyphens,
# with the first character a letter, the last a letter or a number,
# and a 63 character maximum
RELEASE_NAME="deploy-${SHORT_SHA}-${BUILDKITE_BUILD_NUMBER}"

# Commands are passed as args to the script
push=$1
gcloud=$2

# Push a known tag so it is guaranteed to exist for the deploy
1>&2 "${push}" --tag "${TAG}" --repository "${REPOSITORY}"

# Create the Cloud Deploy release
1>&2 "${gcloud}" deploy releases create "${RELEASE_NAME}" \
    --project="${GCP_PROJECT}" \
    --region="${GCP_REGION}" \
    --delivery-pipeline="${GCP_DELIVERY_PIPELINE}" \
    --source="${GCP_CLOUDRUN_SKAFFOLD_SOURCE}" \
    --labels="commit=${BUILDKITE_COMMIT}" \
    --deploy-parameters="customTarget/tag=${TAG}" \
    --annotations="commit=${BUILDKITE_COMMIT},author=${BUILDKITE_BUILD_AUTHOR},commit_message_base64=${COMMIT_MESSAGE_BASE64}"
