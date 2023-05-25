---
aliases:
  - ../migrating-legacy-alerts/
  - ../unified-alerting/opt-in/
  - differences-and-limitations/
description: Migrate legacy dashboard alerts
title: Differences and limitations
weight: 106
---

# Differences and limitations

There are some differences between Grafana Alerting and legacy dashboard alerts, and a number of features that are no
longer supported. We refer to these as [Differences]({{< relref "#differences" >}}) and [Limitations]({{< relref "#limitations" >}}).

## Differences

1. When Grafana Alerting is enabled or upgraded to Grafana 9.0 or later, existing legacy dashboard alerts migrate in a format compatible with the Grafana Alerting. In the Alerting page of your Grafana instance, you can view the migrated alerts alongside any new alerts.
   This topic explains how legacy dashboard alerts are migrated and some limitations of the migration.

2. Read and write access to legacy dashboard alerts and Grafana alerts are governed by the permissions of the folders storing them. During migration, legacy dashboard alert permissions are matched to the new rules permissions as follows:

   - If there are dashboard permissions, a folder named `Migrated {"dashboardUid": "UID", "panelId": 1, "alertId": 1}` is created to match the permissions of the dashboard (including the inherited permissions from the folder).
   - If there are no dashboard permissions and the dashboard is in a folder, then the rule is linked to this folder and inherits its permissions.
   - If there are no dashboard permissions and the dashboard is in the General folder, then the rule is linked to the `General Alerting` folder and the rule inherits the default permissions.

3. `NoData` and `Error` settings are migrated as is to the corresponding settings in Grafana Alerting, except in two situations:

   3.1. As there is no `Keep Last State` option for `No Data` in Grafana Alerting, this option becomes `NoData`. The `Keep Last State` option for `Error` is migrated to a new option `Error`. To match the behavior of the `Keep Last State`, in both cases, during the migration Grafana automatically creates a silence for each alert rule with a duration of 1 year.

   3.2. Due to lack of validation, legacy alert rules imported via JSON or provisioned along with dashboards can contain arbitrary values for `NoData` and [`Error`](/docs/sources/alerting/alerting-rules/create-grafana-managed-rule.md#configure-no-data-and-error-handling). In this situation, Grafana will use the default setting: `NoData` for No data, and `Error` for Error.

4. Notification channels are migrated to an Alertmanager configuration with the appropriate routes and receivers. Default notification channels are added as contact points to the default route. Notification channels not associated with any Dashboard alert go to the `autogen-unlinked-channel-recv` route.

5. Unlike legacy dashboard alerts where images in notifications are enabled per contact point, images in notifications for Grafana Alerting must be enabled in the Grafana configuration, either in the configuration file or environment variables, and are enabled for either all or no contact points. Refer to [images in notifications]({{< relref "../manage-notifications/images-in-notifications" >}}).

6. The JSON format for webhook notifications has changed in Grafana Alerting and uses the format from [Prometheus Alertmanager](https://prometheus.io/docs/alerting/latest/configuration/#webhook_config).

## Limitations

1. Since `Hipchat` and `Sensu` notification channels are no longer supported, legacy alerts associated with these channels are not automatically migrated to Grafana Alerting. Assign the legacy alerts to a supported notification channel so that you continue to receive notifications for those alerts.
   Silences (expiring after one year) are created for all paused dashboard alerts.
