---
aliases:
  - ../../fundamentals/alert-rules/state-and-health/ # /docs/grafana/<GRAFANA_VERSION>/alerting/fundamentals/alert-rules/state-and-health/
  - ../../fundamentals/state-and-health/ # /docs/grafana/<GRAFANA_VERSION>/alerting/fundamentals/state-and-health/
  - ../../unified-alerting/alerting-rules/state-and-health/ # /docs/grafana/<GRAFANA_VERSION>/alerting/unified-alerting/alerting-rules/state-and-health
canonical: https://grafana.com/docs/grafana/latest/alerting/fundamentals/alert-rule-evaluation/state-and-health/
description: Learn about the state and health of alert rules to understand several key status indicators about your alerts
keywords:
  - grafana
  - alerting
  - keep last state
  - guide
  - state
labels:
  products:
    - cloud
    - enterprise
    - oss
title: State and health of alert rules
weight: 109
---

# State and health of alert rules

The state and health of alert rules help you understand several key status indicators about your alerts.

There are three key components: [alert rule state](#alert-rule-state), [alert instance state](#alert-instance-state), and [alert rule health](#alert-rule-health). Although related, each component conveys subtly different information.

## Alert rule state

An alert rule can be in either of the following states:

| State       | Description                                                                                        |
| ----------- | -------------------------------------------------------------------------------------------------- |
| **Normal**  | None of the alert instances returned by the evaluation engine is in a `Pending` or `Firing` state. |
| **Pending** | At least one alert instances returned by the evaluation engine is `Pending`.                       |
| **Firing**  | At least one alert instances returned by the evaluation engine is `Firing`.                        |

The alert rule state is determined by the “worst case” state of the alert instances produced. For example, if one alert instance is firing, the alert rule state is also firing.

{{% admonition type="note" %}}
Alerts transition first to `pending` and then `firing`, thus it takes at least two evaluation cycles before an alert is fired.
{{% /admonition %}}

## Alert instance state

An alert instance can be in either of the following states:

| State        | Description                                                                                   |
| ------------ | --------------------------------------------------------------------------------------------- |
| **Normal**   | The state of an alert that is neither firing nor pending, everything is working correctly.    |
| **Pending**  | The state of an alert that has been active for less than the configured threshold duration.   |
| **Alerting** | The state of an alert that has been active for longer than the configured threshold duration. |
| **NoData**   | No data has been received for the configured time window.                                     |
| **Error**    | The error that occurred when attempting to evaluate an alert rule.                            |

## Keep last state

An alert rule can be configured to keep the last state when a `NoData` and/or `Error` state is encountered. This both prevents alerts from firing, and from resolving and re-firing. Just like normal evaluation, the alert rule transitions from `Pending` to `Firing` after the pending period has elapsed.

## Alert rule health

An alert rule can have one of the following health statuses:

| State                  | Description                                                                                              |
| ---------------------- | -------------------------------------------------------------------------------------------------------- |
| **Ok**                 | No error when evaluating an alerting rule.                                                               |
| **Error**              | An error occurred when evaluating an alerting rule.                                                      |
| **NoData**             | The absence of data in at least one time series returned during a rule evaluation.                       |
| **{status}, KeepLast** | The rule would have received another status but was configured to keep the last state of the alert rule. |

## Special alerts for `NoData` and `Error`

When evaluation of an alert rule produces state `NoData` or `Error`, Grafana Alerting generates alert instances that have the following additional labels:

| Label              | Description                                                            |
| ------------------ | ---------------------------------------------------------------------- |
| **alertname**      | Either `DatasourceNoData` or `DatasourceError` depending on the state. |
| **datasource_uid** | The UID of the data source that caused the state.                      |

You can handle these alerts the same way as regular alerts by adding a silence, route to a contact point, and so on.
