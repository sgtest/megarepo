---
canonical: https://grafana.com/docs/grafana/latest/whatsnew/whats-new-next/
description: Feature and improvement highlights for Grafana Cloud
keywords:
  - grafana
  - new
  - documentation
  - cloud
  - release notes
labels:
  products:
    - cloud
title: What's new in Grafana Cloud
weight: -37
---

# What’s new in Grafana Cloud

Welcome to Grafana Cloud! Read on to learn about the newest changes to Grafana Cloud.

## Support for dashboard variables in transformations

<!-- Oscar Kilhed, Victor Marin -->
<!-- already in on-prem -->

October 24, 2023

_Experimental in Grafana Cloud_

Previously, the only transformation that supported [dashboard variables](https://grafana.com/docs/grafana/<GRAFANA_VERSION>/dashboards/variables/) was the **Add field from calculation** transformation. We've now extended the support for variables to the **Filter by value**, **Create heatmap**, **Histogram**, **Sort by**, **Limit**, **Filter by name**, and **Join by field** transformations.

We've also made it easier to find the correct dashboard variable by displaying available variables in the fields that support them, either in the drop-down or as a suggestion when you type **$** or press Ctrl + Space:

{{< figure src="/media/docs/grafana/transformations/completion.png" caption="Input with dashboard variable suggestions" >}}

## Role mapping support for Google OIDC

<!-- Jo Guerreiro -->
<!-- already in on-prem -->

October 24, 2023

_Generally available in Grafana Cloud_

You can now map Google groups to Grafana organizational roles when using Google OIDC.
This is useful if you want to limit the access users have to your Grafana instance.

We've also added support for controlling allowed groups when using Google OIDC.

Refer to the [Google Authentication documentation](https://grafana.com/docs/grafana/<GRAFANA_VERSION>/setup-grafana/configure-security/configure-authentication/google/) to learn how to use these new options.

## Distributed tracing in Grafana Cloud k6

<!-- Heitor Tashiro Sergent -->

_Generally available in Grafana Cloud_

You can now use the Grafana Cloud Traces integration with Grafana Cloud k6 to quickly debug failed performance tests and proactively improve application reliability.

Distributed tracing in Grafana Cloud k6 only requires two things:

- An application instrumented for tracing with Grafana Cloud Traces.
- Adding a few lines of code to your existing k6 scripts.

The integration works by having k6 inject tracing metadata into the requests it sends to your backend services when you run a test. The tracing data is then correlated with k6 test run data, so you can understand how your services and operations behaved during the whole test run. The collected tracing data is aggregated to generate real-time metrics—such as frequency of calls, error rates, and percentile latencies—that can help you narrow your search space and quickly spot anomalies.

To learn more, refer to the [Integration with Grafana Cloud Traces documentation](/docs/grafana-cloud/k6/analyze-results/integration-with-grafana-cloud-traces/) and [Distributed Tracing in Grafana Cloud k6 blog post](https://grafana.com/blog/2023/09/19/troubleshoot-failed-performance-tests-faster-with-distributed-tracing-in-grafana-cloud-k6/).

## Tenant database instance name and number for SAP HANA® data source

<!-- Miguel Palau -->
<!-- OSS, Enterprise -->

_Generally available in Grafana Cloud_

The SAP HANA® data source now supports tenant databases connections by using the database name and/or instance number. For more information, refer to [SAP HANA® configuration](/docs/plugins/grafana-saphana-datasource/latest/#configuration).

{{< video-embed src="/media/docs/sap-hana/tenant.mp4" >}}

## Log aggregation for Datadog data source

<!-- Taewoo Kim -->
<!-- OSS, Enterprise -->

_Generally available in Grafana Cloud_

The Datadog data source now supports log aggregation. This feature helps aggregate logs/events into buckets and compute metrics and time series. For more information, refer to [Datadog log aggregation](/docs/plugins/grafana-datadog-datasource/latest#logs-analytics--aggregation).

{{< video-embed src="/media/docs/datadog/datadog-log-aggregation.mp4" >}}

## API throttling for Datadog data source

<!-- Taewoo Kim -->
<!-- OSS, Enterprise -->

_Generally available in Grafana Cloud_

The Datadog data source supports blocking API requests based on upstream rate limits (for metric queries). With this update, you can set a rate limit percentage at which the plugin stops sending queries.

To learn more, refer to [Datadog data source settings](/docs/plugins/grafana-datadog-datasource/latest#configure-the-data-source), as well as the following video demo.

{{< video-embed src="/media/docs/datadog/datadog-rate-limit.mp4" >}}

## Query-type template variables for Tempo data source

<!-- Fabrizio Casati -->
<!-- OSS, Enterprise -->

_Generally available in Grafana Cloud_

The Tempo data source now supports query-type template variables. With this update, you can create variables for which the values are a list of attribute names or attribute values seen on spans received by Tempo.

To learn more, refer to the following video demo, as well as the [Grafana Variables documentation](/docs/grafana/next/dashboards/variables/).

{{< video-embed src="/media/docs/tempo/screen-recording-grafana-10.2-tempo-query-type-template-variables.mp4" >}}

## Improved TraceQL query editor

<!-- Fabrizio Casati -->
<!-- OSS, Enterprise -->

_Generally available in Grafana Cloud_

The [TraceQL query editor](https://grafana.com/docs/tempo/latest/traceql/#traceql-query-editor) has been improved to facilitate the creation of TraceQL queries. In particular, it now features improved autocompletion, syntax highlighting, and error reporting.

{{< video-embed src="/media/docs/tempo/screen-recording-grafana-10.2-traceql-query-editor-improvements.mp4" >}}

## Grafana OnCall integration for Alerting

<!-- Brenda Muir -->
<!-- OSS, Enterprise -->

_Generally available in Grafana Cloud_

Use the Grafana Alerting - Grafana OnCall integration to effortlessly connect alerts generated by Grafana Alerting with Grafana OnCall. From there, you can route them according to defined escalation chains and schedules.

To learn more, refer to the [Grafana OnCall integration for Alerting documentation](/docs/grafana/next/alerting/alerting-rules/manage-contact-points/configure-oncall/).

## New browse dashboards view

<!-- Yaelle Chaudy for Frontend Platform -->
<!-- OSS, Enterprise -->

_Available in public preview in Grafana Cloud_

We are gradually rolling out our new browse dashboards user interface. With this new feature, we removed the **General** folder, and dashboards now sit at the root level. The feature also provides easier editing functionality, as well as faster search renders.

To learn more, refer to the following video demo.

{{< video-embed src="/media/docs/grafana/2023-09-11-New-Browse-Dashboards-Enablement-Video.mp4" >}}

## Temporary credentials in CloudWatch data source

<!-- Michael Mandrus, Ida Štambuk, Sarah Zinger  -->
<!-- Cloud -->

_Available in private preview in Grafana Cloud_

The Grafana Assume Role authentication provider lets Grafana Cloud users of the CloudWatch data source authenticate with AWS without having to create and maintain long term AWS Users. Using the new assume role authentication method, you no longer have to rotate access and secret keys in your CloudWatch data source. Instead, Grafana Cloud users can create an identity access and management (IAM) role that has a trust relationship with Grafana's AWS account; Grafana's AWS account will then use AWS Secure Token Service (STS) to create temporary credentials to access the user's AWS data.

To learn more, refer to the [CloudWatch authentication documentation](/docs/grafana/next/datasources/aws-cloudwatch/aws-authentication).

## Permission validation on custom role creation and update

<!-- Mihaly Gyongyosi -->
<!-- Cloud -->

<!-- already in on-prem -->

August 25, 2023

_Generally available in Grafana Cloud_

With the current release, we enabled RBAC permission validation (`rbac.permission_validation_enabled` setting) by default. This means that the permissions provided in the request during custom role creation or update are validated against the list of [available permissions and their scopes](https://grafana.com/docs/grafana/<GRAFANA_VERSION>/administration/roles-and-permissions/access-control/custom-role-actions-scopes/#action-definitions). If the request contains a permission that is not available or the scope of the permission is not valid, the request is rejected with an error message.
