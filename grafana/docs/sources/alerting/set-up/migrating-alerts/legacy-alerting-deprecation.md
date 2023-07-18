---
aliases:
  - alerting/legacy-alerting-deprecation/
canonical: https://grafana.com/docs/grafana/latest/alerting/set-up/migrating-alerts/legacy-alerting-deprecation/
description: Legacy alerting deprecation notice
keywords:
  - grafana
  - alerting
labels:
  products:
    - enterprise
    - oss
title: Legacy alerting deprecation
weight: 109
---

# Legacy alerting deprecation

Starting with Grafana v9.0.0, legacy alerting is deprecated, meaning that it is no longer actively maintained or supported by Grafana. As of Grafana v10.0.0, we do not contribute or accept external contributions to the codebase apart from CVE fixes.

Legacy alerting refers to the old alerting system that was used prior to the introduction of Grafana Alerting; the new alerting system in Grafana.

The decision to deprecate legacy alerting was made to encourage users to migrate to the new alerting system, which offers a more powerful and flexible alerting experience based on Prometheus Alertmanager.

Users who are still using legacy alerting are encouraged to migrate their alerts to the new system as soon as possible to ensure that they continue to receive new features, bug fixes, and support.

However, we will still patch CVEs until legacy alerting is completely removed in Grafana 11; honoring our commitment to building and distributing secure software.

We have provided [instructions][migrating-alerts] on how to migrate to the new alerting system, making the process as easy as possible for users.

## Why are we deprecating legacy alerting?

The new Grafana alerting system is more powerful and flexible than the legacy alerting feature.

The new system is based on Prometheus Alertmanager, which offers a more comprehensive set of features for defining and managing alerts. With the new alerting system, users can create alerts based on complex queries, configure alert notifications via various integrations, and set up sophisticated alerting rules with support for conditional expressions, aggregation, and grouping.

Overall, the new alerting system in Grafana is a major improvement over the legacy alerting feature, providing users with a more powerful and flexible alerting experience.

Additionally, legacy alerting still requires Angular to function and we are [planning to remove support for it][angular_deprecation] in Grafana 11.

## When will we remove legacy alerting completely?

Legacy alerting will be removed from the code-base in Grafana 11, following the same timeline as the [Angular deprecation][angular_deprecation].

## How do I migrate to the new Grafana alerting?

Refer to our [upgrade instructions][migrating-alerts].

### Useful links

- [Upgrade Alerting][migrating-alerts]
- [Angular support deprecation][angular_deprecation]

{{% docs/reference %}}
[angular_deprecation]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/developers/angular_deprecation"
[angular_deprecation]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/developers/angular_deprecation"

[migrating-alerts]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/alerting/set-up/migrating-alerts"
[migrating-alerts]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/alerting-and-irm/alerting/set-up/migrating-alerts"
{{% /docs/reference %}}
