#!/usr/bin/env bash
set -euxo pipefail

# setup DIR for easier pathing /Users/dax/work/sourcegraph/test/cluster
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)""
# cd to repo root
root_dir="$(dirname "${BASH_SOURCE[0]}")/../../../.."
cd "$root_dir"
root_dir=$(pwd)

export NAMESPACE="cluster-ci-$BUILDKITE_BUILD_NUMBER-$BUILDKITE_JOB_ID"

# Capture information about the state of the test cluster
function cluster_capture_state() {
  # Get overview of all pods
  kubectl get pods

  pushd "$root_dir"
  # Get specifics of pods
  kubectl describe pods >'describe_pods.log'
  chmod 744 'describe_pods.log'

  # Get logs for some deployments
  FRONTEND_LOGS="frontend_logs.log"
  kubectl logs deployment/sourcegraph-frontend --all-containers >$FRONTEND_LOGS
  chmod 744 $FRONTEND_LOGS
  popd
}

# Cleanup the cluster
function cluster_cleanup() {
  cluster_capture_state
  kubectl delete namespace "$NAMESPACE"
}

function cluster_setup() {
  git clone --depth 1 \
    https://github.com/sourcegraph/deploy-sourcegraph.git \
    "$DIR/deploy-sourcegraph"

  gcloud container clusters get-credentials default-buildkite --zone=us-central1-c --project=sourcegraph-ci

  kubectl create ns "$NAMESPACE" -oyaml --dry-run | kubectl apply -f -
  trap cluster_cleanup exit
  kubectl apply -f "$DIR/storageClass.yaml"
  kubectl config set-context --current --namespace="$NAMESPACE"
  kubectl config current-context
  sleep 15 #wait for namespace to come up
  kubectl get -n "$NAMESPACE" pods

  pushd "$DIR/deploy-sourcegraph/"
  set +e
  set +o pipefail
  pushd base
  # Remove cAdvisor, it deploys on all Buildkite nodes as a daemonset and is non-critical.
  rm -rf ./cadvisor
  # See $DOCKER_CLUSTER_IMAGES_TXT in pipeline-steps.go for env var
  # replace all docker image tags with previously built candidate images
  while IFS= read -r line; do
    echo "$line"
    grep -lr '.' -e "index.docker.io/sourcegraph/$line" --include \*.yaml | xargs sed -i -E "s#index.docker.io/sourcegraph/$line:.*#us.gcr.io/sourcegraph-dev/$line:$CANDIDATE_VERSION#g"
  done < <(printf '%s\n' "$DOCKER_CLUSTER_IMAGES_TXT")
  popd
  ./create-new-cluster.sh
  popd

  kubectl get pods
  time kubectl wait --for=condition=Ready -l app=sourcegraph-frontend pod --timeout=20m
  set -e
  set -o pipefail
}

function test_setup() {

  set +x +u
  # shellcheck disable=SC1091
  source /root/.profile

  dev/ci/test/setup-deps.sh

  sleep 15
  export SOURCEGRAPH_BASE_URL="http://sourcegraph-frontend.$NAMESPACE.svc.cluster.local:30080"
  curl "$SOURCEGRAPH_BASE_URL"

  # setup admin users, etc
  pushd internal/cmd/init-sg
  go build
  ./init-sg initSG -baseurl="$SOURCEGRAPH_BASE_URL"
  popd

  # Load variables set up by init-server, disabling `-x` to avoid printing variables, setting +u to avoid blowing up on ubound ones
  set +x +u
  # shellcheck disable=SC1091
  source /root/.profile
  set -x -u

  echo "TEST: Checking Sourcegraph instance is accessible"

  curl --fail "$SOURCEGRAPH_BASE_URL"
  curl --fail "$SOURCEGRAPH_BASE_URL/healthz"
}

function e2e() {
  echo "TEST: Running tests"
  pushd client/web
  echo "TEST: Downloading Puppeteer"
  yarn --cwd client/shared run download-puppeteer-browser
  echo "$SOURCEGRAPH_BASE_URL"
  yarn run test:regression:core
  yarn run test:regression:config-settings
  # yarn run test:regression:integrations
  # yarn run test:regression:search
  popd
}

# main
cluster_setup
test_setup
# TODO: Failing tests do not fail the build
set +o pipefail
e2e || true
