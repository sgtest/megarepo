---
aliases:
  - ../fundamentals/alertmanager/
  - ../metrics/
  - ../unified-alerting/fundamentals/alertmanager/
  - alerting/manage-notifications/alertmanager/
canonical: https://grafana.com/docs/grafana/latest/alerting/fundamentals/alertmanager/
description: Intro to the different Alertmanagers
labels:
  products:
    - cloud
    - enterprise
    - oss
title: Alertmanager
weight: 140
---

# Alertmanager

Alertmanager enables you to quickly and efficiently manage and respond to alerts. It receives alerts, handles silencing, inhibition, grouping, and routing by sending notifications out via your channel of choice, for example, email or Slack.

In Grafana, you can use the Cloud Alertmanager, Grafana Alertmanager, or an external Alertmanager. You can also run multiple Alertmanagers; your decision depends on your set up and where your alerts are being generated.

**Cloud Alertmanager**

Cloud Alertmanager runs in Grafana Cloud and it can receive alerts from Grafana, Mimir, and Loki.

**Grafana Alertmanager**

Grafana Alertmanager is an internal Alertmanager that is pre-configured and available for selection by default if you run Grafana on-premises or open-source.

The Grafana Alertmanager can receive alerts from Grafana, but it cannot receive alerts from outside Grafana, for example, from Mimir or Loki.

**Note that inhibition rules are not supported in the Grafana Alertmanager.**

**External Alertmanager**

If you want to use a single Alertmanager to receive all your Grafana, Loki, Mimir, and Prometheus alerts, you can set up Grafana to use an external Alertmanager. This external Alertmanager can be configured and administered from within Grafana itself.

Here are two examples of when you may want to configure your own external alertmanager and send your alerts there instead of the Grafana Alertmanager:

1. You may already have Alertmanagers on-premises in your own Cloud infrastructure that you have set up and still want to use, because you have other alert generators, such as Prometheus.

2. You want to use both Prometheus on-premises and hosted Grafana to send alerts to the same Alertmanager that runs in your Cloud infrastructure.

Alertmanagers are visible from the drop-down menu on the Alerting Contact Points, Notification Policies, and Silences pages.

If you are provisioning your data source, set the flag `handleGrafanaManagedAlerts` in the `jsonData` field to `true` to send Grafana-managed alerts to this Alertmanager.

**Useful links**

[Prometheus Alertmanager documentation](https://prometheus.io/docs/alerting/latest/alertmanager/)

[Add an external Alertmanager][configure-alertmanager]

{{% docs/reference %}}
[configure-alertmanager]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/alerting/set-up/configure-alertmanager"
[configure-alertmanager]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/alerting-and-irm/alerting/set-up/configure-alertmanager"
{{% /docs/reference %}}
