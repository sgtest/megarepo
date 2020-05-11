#!/bin/sh
set -e

CONFIG_FILE=/sg_config_prometheus/prometheus.yml

if [ -e /sg_prometheus_add_ons/prometheus.yml ]; then
  CONFIG_FILE=/sg_prometheus_add_ons/prometheus.yml
fi

# shellcheck disable=SC2086
exec /bin/prometheus --config.file=$CONFIG_FILE --storage.tsdb.path=/prometheus --web.enable-admin-api $PROMETHEUS_ADDITIONAL_FLAGS --web.console.libraries=/usr/share/prometheus/console_libraries --web.console.templates=/usr/share/prometheus/consoles "$@"
