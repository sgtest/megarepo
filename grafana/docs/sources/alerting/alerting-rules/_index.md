---
menuTitle: Configure
aliases:
  - old-alerting/create-alerts/
  - rules/
  - unified-alerting/alerting-rules/
description: Configure alerting
title: Configure Alerting
weight: 130
---

# Configure Alerting

Configure the features and integrations that you need to create and manage your alerts.

**Configure alert rules**

An alert rule is a set of evaluation criteria that determines whether an alert will fire. The alert rule consists of one or more queries and expressions, a condition, the frequency of evaluation, and optionally, the duration over which the condition is met.

While queries and expressions select the data set to evaluate, a condition sets the threshold that an alert must meet or exceed to create an alert. An interval specifies how frequently an alert rule is evaluated. Duration, when configured, indicates how long a condition must be met. Alert rules can also define alerting behavior in the absence of data.

You can:

- [Create Grafana Mimir or Loki managed alert rules]({{< relref "./create-mimir-loki-managed-rule" >}})
- [Create Grafana Mimir or Loki managed recording rules]({{< relref "./create-mimir-loki-managed-recording-rule" >}})
- [Edit Grafana Mimir or Loki rule groups and namespaces]({{< relref "./edit-mimir-loki-namespace-group" >}})
- [Create Grafana managed alert rules]({{< relref "./create-grafana-managed-rule" >}})

**Note:**
Grafana managed alert rules can only be edited or deleted by users with Edit permissions for the folder storing the rules.

Alert rules for an external Grafana Mimir or Loki instance can be edited or deleted by users with Editor or Admin roles.

**Configure contact points**

For information on how to configure contact points, see [Configure contact points]({{< relref "./manage-contact-points/_index.md" >}})

**Configure notification policies**

For information on how to configure notification policies, see [Configure notification policies]({{< relref "./create-notification-policy" >}})
