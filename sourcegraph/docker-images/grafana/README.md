# Grafana image

Vanilla Grafana image with provisioned Sourcegraph dashboards and config. For more details, refer to [the handbook](https://about.sourcegraph.com/handbook/engineering/distribution/observability/monitoring#grafana).

## Image API

```shell script
docker run  \
    -v ${GRAFANA_DISK}:/var/lib/grafana \
    -v %{GRAFANA_DATA_SOURCES}:/sg_config_grafana/provisioning/datasources \
    sourcegraph/grafana
```

Image expects two volumes mounted:

- at `/var/lib/grafana` a data directory where logs, the Grafana db and other Grafana data files will live
- at `/sg_config_grafana/provisioning/datasources` a directory with data source yaml files.

A directory containing dashboard json specifications can be mounted at
`/sg_grafana_additional_dashboards` and they will be picked up automatically. Changes to files in that directory
will be detected automatically while Grafana is running.

More behavior can be controlled with
[environmental variables](https://grafana.com/docs/installation/configuration/).
