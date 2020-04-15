#!/usr/bin/env bash

# Description: Prometheus collects metrics and aggregates them into graphs.
#

set -euf -o pipefail
pushd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null

CONFIG_DIR="$(pwd)/docker-images/prometheus/config"

PROMETHEUS_DISK="${HOME}/.sourcegraph-dev/data/prometheus"

IMAGE=sourcegraph/prometheus:10.0.10@sha256:4d73d99f3b18d8e4e09f63671c51eabcba956e357ab33d682de8cf1540e3b863
CONTAINER=prometheus

CID_FILE="${PROMETHEUS_DISK}/prometheus.cid"

mkdir -p ${PROMETHEUS_DISK}/logs
rm -f ${CID_FILE}

function finish() {
  if test -f ${CID_FILE}; then
    echo 'trapped CTRL-C: stopping docker prometheus container'
    docker stop $(cat ${CID_FILE})
    rm -f ${CID_FILE}
  fi
  rm -f ${CONFIG_DIR}/prometheus_targets.yml
  docker rm -f $CONTAINER
}
trap finish EXIT

NET_ARG=""
PROM_TARGETS="dev/prometheus/all/prometheus_targets.yml"

if [[ "$OSTYPE" == "linux-gnu" ]]; then
  NET_ARG="--net=host"
  PROM_TARGETS="dev/prometheus/linux/prometheus_targets.yml"
fi

cp ${PROM_TARGETS} ${CONFIG_DIR}/prometheus_targets.yml

docker inspect $CONTAINER >/dev/null 2>&1 && docker rm -f $CONTAINER

# Generate Grafana dashboards
pushd observability
DEV=true RELOAD=false go generate
popd

docker run --rm ${NET_ARG} --cidfile ${CID_FILE} \
  --name=prometheus \
  --cpus=1 \
  --memory=4g \
  --user=$UID \
  -p 0.0.0.0:9090:9090 \
  -v ${PROMETHEUS_DISK}:/prometheus \
  -v ${CONFIG_DIR}:/sg_prometheus_add_ons \
  -e PROMETHEUS_ADDITIONAL_FLAGS=--web.enable-lifecycle \
  ${IMAGE} >>${PROMETHEUS_DISK}/logs/prometheus.log 2>&1 &
wait $!
