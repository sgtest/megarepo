---
keywords:
  - grafana
  - schema
labels:
  products:
    - cloud
    - enterprise
    - oss
title: PrometheusDataQuery kind
---
> Both documentation generation and kinds schemas are in active development and subject to change without prior notice.

## PrometheusDataQuery

#### Maturity: [experimental](../../../maturity/#experimental)
#### Version: 0.0



| Property         | Type    | Required | Default | Description                                                                                                                                                                                                                                             |
|------------------|---------|----------|---------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `expr`           | string  | **Yes**  |         | The actual expression/query that will be evaluated by Prometheus                                                                                                                                                                                        |
| `refId`          | string  | **Yes**  |         | A unique identifier for the query within the list of targets.<br/>In server side expressions, the refId is used as a variable name to identify results.<br/>By default, the UI will assign A->Z; however setting meaningful names may be useful.        |
| `datasource`     |         | No       |         | For mixed data sources the selected datasource is on the query level.<br/>For non mixed scenarios this is undefined.<br/>TODO find a better way to do this ^ that's friendly to schema<br/>TODO this shouldn't be unknown but DataSourceRef &#124; null |
| `editorMode`     | string  | No       |         | Possible values are: `code`, `builder`.                                                                                                                                                                                                                 |
| `exemplar`       | boolean | No       |         | Execute an additional query to identify interesting raw samples relevant for the given expr                                                                                                                                                             |
| `format`         | string  | No       |         | Possible values are: `time_series`, `table`, `heatmap`.                                                                                                                                                                                                 |
| `hide`           | boolean | No       |         | true if query is disabled (ie should not be returned to the dashboard)<br/>Note this does not always imply that the query should not be executed since<br/>the results from a hidden query may be used as the input to other queries (SSE etc)          |
| `instant`        | boolean | No       |         | Returns only the latest value that Prometheus has scraped for the requested time series                                                                                                                                                                 |
| `intervalFactor` | number  | No       |         | @deprecated Used to specify how many times to divide max data points by. We use max data points under query options<br/>See https://github.com/grafana/grafana/issues/48081                                                                             |
| `legendFormat`   | string  | No       |         | Series name override or template. Ex. {{hostname}} will be replaced with label value for hostname                                                                                                                                                       |
| `queryType`      | string  | No       |         | Specify the query flavor<br/>TODO make this required and give it a default                                                                                                                                                                              |
| `range`          | boolean | No       |         | Returns a Range vector, comprised of a set of time series containing a range of data points over time for each time series                                                                                                                              |


