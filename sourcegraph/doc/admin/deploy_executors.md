# Deploying Sourcegraph executors

>NOTE: **Sourcegraph executors are currently experimental.** We're exploring this feature set. 
>Let us know what you think! [File an issue](https://github.com/sourcegraph/sourcegraph/issues/new/choose)
>with feedback/problems/questions, or [contact us directly](https://about.sourcegraph.com/contact).

Executors are an experimental service that powers automatically indexing a repository for precise code intelligence.

We supply Terraform modules to provision executors on multiple clouds:

- [Google Cloud](https://github.com/sourcegraph/terraform-google-executors)
- [AWS](https://github.com/sourcegraph/terraform-aws-executors)

## Configuring executors and instance communication

In order for the executors to dequeue and perform work, they must be able to reach the target Sourcegraph instance. Set the following variables with a unique username and password value.

The `frontend` service must define the following environment variables:

- `EXECUTOR_FRONTEND_PASSWORD`

The `executor` service must define the following environment variables:

- `EXECUTOR_FRONTEND_URL`
- `EXECUTOR_FRONTEND_PASSWORD`

When using a Sourcegraph Terraform module to provision executors, the required executor environment variables can be set via:

- `sourcegraph_external_url`: [Google](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-google-executors%24+variable+%22sourcegraph_external_url%22&patternType=literal); [AWS](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-aws-executors%24+variable+%22sourcegraph_external_url%22&patternType=literal)
- `sourcegraph_executor_proxy_username`: [Google](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-google-executors%24+variable+%22sourcegraph_executor_proxy_username%22&patternType=literal); [AWS](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-aws-executors%24+variable+%22sourcegraph_executor_proxy_username%22&patternType=literal)
- `sourcegraph_executor_proxy_password`: [Google](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-google-executors%24+variable+%22sourcegraph_executor_proxy_password%22&patternType=literal); [AWS](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-aws-executors%24+variable+%22sourcegraph_executor_proxy_password%22&patternType=literal)

## Configuring auto scaling

### Google

The GCE auto-scaling groups configured by the Sourcegraph Terraform module to respond to changes in metric values written to Cloud Monitoring. The target Sourcegraph instance is expected to continuously write these values.

To write the metric to Cloud Monitoring, the `worker` service must define the following environment variables:

- `EXECUTOR_METRIC_ENVIRONMENT_LABEL`: Must use same value as [`metrics_environment_label`](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-google-executors%24+variable+%22metrics_environment_label%22&patternType=literal)
- `EXECUTOR_METRIC_GCP_PROJECT_ID`
- `EXECUTOR_METRIC_GOOGLE_APPLICATION_CREDENTIALS_FILE`

### AWS

The EC2 auto-scaling groups configured by the Sourcegraph Terraform module to respond to changes in metric values written to CloudWatch. The target Sourcegraph instance is expected to continuously write these values.

To write the metric to CloudWatch, the `worker` service must define the following environment variables:

- `EXECUTOR_METRIC_ENVIRONMENT_LABEL`: Must use same value as [`metrics_environment_label`](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-aws-executors%24+variable+%22metrics_environment_label%22&patternType=literal)
- `EXECUTOR_METRIC_AWS_NAMESPACE`: Must be set to `sourcegraph-executor`
- `EXECUTOR_METRIC_AWS_REGION`
- `EXECUTOR_METRIC_AWS_ACCESS_KEY_ID`
- `EXECUTOR_METRIC_AWS_SECRET_ACCESS_KEY`

## Configuring observability

Sourcegraph ships with dashboards to display executor metrics. To populate these dashboards, the target Prometheus instance must be able to scrape the executor metrics endpoint.

### Google

The Prometheus configuration must add the following scraping job that uses [GCE service discovery configuration](https://prometheus.io/docs/prometheus/latest/configuration/configuration/#gce_sd_config):

```yaml
- job_name: 'sourcegraph-executors'
  gce_sd_configs:
    - project: {GCP_PROJECT}
      port: 6060
      zone: {GCP_ZONE}
      filter: '(labels.executor_tag = {INSTANCE_TAG})'
  relabel_configs:
    - source_labels: [__meta_gce_public_ip]
      target_label: __address__
      replacement: "${1}${2}:6060"
      separator: ''
    - source_labels: [__meta_gce_zone]
      regex: ".+/([^/]+)"
      target_label: zone
      separator: ''
    - source_labels: [__meta_gce_project]
      target_label: project
    - source_labels: [__meta_gce_instance_name]
      target_label: instance
      separator: ''
    - regex: "__meta_gce_metadata_(image_.+)"
      action: labelmap
```

The `{INSTANCE_TAG}` value above must be the same as [`instance_tag`](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-google-executors%24+variable+%22instance_tag%22&patternType=literal).

### AWS

The Prometheus configuration must add the following scraping job that uses [EC2 service discovery configuration](https://prometheus.io/docs/prometheus/latest/configuration/configuration/#ec2_sd_config).

```yaml
- job_name: 'sourcegraph-executors'
  ec2_sd_configs:
    - region: {AWS_REGION}
      port: 6060
      filters:
        - name: tag:executor_tag
          values: [{INSTANCE_TAG}]
  relabel_configs:
    - source_labels: [__meta_ec2_public_ip]
      target_label: __address__
      replacement: "${1}${2}:6060"
      separator: ''
    - source_labels: [__meta_ec2_availability_zone]
      regex: ".+/([^/]+)"
      target_label: zone
      separator: ''
    - source_labels: [__meta_ec2_instance_id]
      target_label: instance
      separator: ''
    - source_labels: [__meta_ec2_ami]
      target_label: version
```

The `{INSTANCE_TAG}` value above must be the same as [`instance_tag`](https://sourcegraph.com/search?q=context:global+repo:%5Egithub.com/sourcegraph/terraform-aws-executors%24+variable+%22instance_tag%22&patternType=literal).
