---
description: Tracing in Explore
keywords:
  - explore
  - trace
title: Tracing in Explore
weight: 20
---

# Tracing in Explore

You can use Explore to query and visualize traces from tracing data sources.

Supported data sources are:

- [Tempo]({{< relref "../datasources/tempo/" >}})
- [Jaeger]({{< relref "../datasources/jaeger/" >}})
- [Zipkin]({{< relref "../datasources/zipkin/" >}})
- [X-Ray](https://grafana.com/grafana/plugins/grafana-x-ray-datasource)

For information on how to configure queries for the data sources listed above, refer to the documentation for specific data source.

## Query editor

You can query and search tracing data using a data source's query editor.

Each data source can have it's own query editor. The query editor for the Tempo data source is slightly different than the query editor for the Jaegar data source.

For information on querying each data source, refer to their documentation:

- [Tempo query editor]({{< relref "../datasources/tempo/query-editor" >}})
- [Jaeger query editor]({{< relref "../datasources/jaeger/#query-the-data-source" >}})
- [Zipkin query editor]({{< relref "../datasources/zipkin/#query-the-data-source" >}})

## Trace View

This section explains the elements of the Trace View.

{{< figure src="/static/img/docs/explore/explore-trace-view-full-8-0.png" class="docs-image--no-shadow" max-width= "900px" caption="Screenshot of the trace view" >}}

### Header

{{< figure src="/static/img/docs/v70/explore-trace-view-header.png" class="docs-image--no-shadow" max-width= "750px" caption="Screenshot of the trace view header" >}}

- Header title: Shows the name of the root span and trace ID.
- Search: Highlights spans containing the searched text.
- Metadata: Various metadata about the trace.

### Minimap

{{< figure src="/static/img/docs/v70/explore-trace-view-minimap.png" class="docs-image--no-shadow" max-width= "900px" caption="Screenshot of the trace view minimap" >}}

Shows condensed view or the trace timeline. Drag your mouse over the minimap to zoom into smaller time range. Zooming will also update the main timeline, so it is easy to see shorter spans. Hovering over the minimap, when zoomed, will show Reset Selection button which resets the zoom.

### Span Filters

{{% admonition type="note" %}}
This feature is behind the `newTraceViewHeader` [feature toggle]({{< relref "../../setup-grafana/configure-grafana#feature_toggles" >}}).
If you use Grafana Cloud, open a [support ticket in the Cloud Portal](/profile/org#support) to access this feature.
{{% /admonition %}}

![Screenshot of span filtering](/media/docs/tempo/screenshot-grafana-tempo-span-filters.png)

Using span filters, you can filter your spans in the trace timeline viewer. The more filters you add, the more specific are the filtered spans.

You can add one or more of the following filters:

- Service name
- Span name
- Duration
- Tags (which include tags, process tags, and log fields)

### Timeline

{{< figure src="/static/img/docs/v70/explore-trace-view-timeline.png" class="docs-image--no-shadow" max-width= "900px"  caption="Screenshot of the trace view timeline" >}}

Shows list of spans within the trace. Each span row consists of these components:

- Expand children button: Expands or collapses all the children spans of selected span.
- Service name: Name of the service logged the span.
- Operation name: Name of the operation that this span represents.
- Span duration bar: Visual representation of the operation duration within the trace.

Clicking anywhere on the span row shows span details.

### Span details

{{< figure src="/static/img/docs/v70/explore-trace-view-span-details.png" class="docs-image--no-shadow" max-width= "900px"  caption="Screenshot of the trace view span details" >}}

- Operation name.
- Span metadata.
- Tags: Any tags associated with this span.
- Process metadata: Metadata about the process that logged this span.
- Logs: List of logs logged by this span and associated key values. In case of Zipkin logs section shows Zipkin annotations.

### Trace to logs

{{% admonition type="note" %}}
Available in Grafana 7.4 and later versions.
{{% /admonition %}}

You can navigate from a span in a trace view directly to logs relevant for that span. This feature is available for Tempo, Jaeger, and Zipkin data sources. Refer to their [relevant documentation](/docs/grafana/latest/datasources/tempo/#trace-to-logs) for configuration instructions.

{{< figure src="/static/img/docs/explore/trace-to-log-7-4.png" class="docs-image--no-shadow" max-width= "600px"  caption="Screenshot of the trace view in Explore with icon next to the spans" >}}

Click the document icon to open a split view in Explore with the configured data source and query relevant logs for the span.

### Trace to metrics

{{% admonition type="note" %}}
This feature is currently in beta & behind the `traceToMetrics` feature toggle.
{{% /admonition %}}

You can navigate from a span in a trace view directly to metrics relevant for that span. This feature is available for Tempo, Jaeger, and Zipkin data sources. Refer to their [relevant documentation](/docs/grafana/latest/datasources/tempo/#trace-to-metrics) for configuration instructions.

## Node Graph

You can optionally expand the node graph for the displayed trace. Depending on the data source, this can show spans of the trace as nodes in the graph, or as some additional context like service graph based on the current trace.

![Node graph](/static/img/docs/explore/explore-trace-view-node-graph-8-0.png 'Node graph')

## Service Graph

The Service Graph visualizes the span metrics (traces data for rates, error rates, and durations (RED)) and service graphs.
Once the requirements are set up, this pre-configured view is immediately available.

For more information, refer to the [Service Graph view section]({{< relref "/docs/grafana/latest/datasources/tempo/#open-the-service-graph-view" >}}) of the Tempo data source page and the [service graph view page]({{< relref "/docs/tempo/latest/metrics-generator/service-graph-view/" >}}) in the Tempo documentation.

{{< figure src="/static/img/docs/grafana-cloud/apm-overview.png" class="docs-image--no-shadow" max-width= "900px" caption="Screenshot of the Service Graph view" >}}

## Data API

This visualization needs a specific shape of the data to be returned from the data source in order to correctly display it.

Data source needs to return data frame and set `frame.meta.preferredVisualisationType = 'trace'`.

### Data frame structure

Required fields:

| Field name   | Type                | Description                                                                                                                         |
| ------------ | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| traceID      | string              | Identifier for the entire trace. There should be only one trace in the data frame.                                                  |
| spanID       | string              | Identifier for the current span. SpanIDs should be unique per trace.                                                                |
| parentSpanID | string              | SpanID of the parent span to create child parent relationship in the trace view. Can be `undefined` for root span without a parent. |
| serviceName  | string              | Name of the service this span is part of.                                                                                           |
| serviceTags  | TraceKeyValuePair[] | List of tags relevant for the service.                                                                                              |
| startTime    | number              | Start time of the span in millisecond epoch time.                                                                                   |
| duration     | number              | Duration of the span in milliseconds.                                                                                               |

Optional fields:

| Field name     | Type                | Description                                                        |
| -------------- | ------------------- | ------------------------------------------------------------------ |
| logs           | TraceLog[]          | List of logs associated with the current span.                     |
| tags           | TraceKeyValuePair[] | List of tags associated with the current span.                     |
| warnings       | string[]            | List of warnings associated with the current span.                 |
| stackTraces    | string[]            | List of stack traces associated with the current span.             |
| errorIconColor | string              | Color of the error icon in case span is tagged with `error: true`. |

For details about the types see [TraceSpanRow](https://github.com/grafana/grafana/blob/main/packages/grafana-data/src/types/trace.ts#L28), [TraceKeyValuePair](https://github.com/grafana/grafana/blob/main/packages/grafana-data/src/types/trace.ts#L4) and [TraceLog](https://github.com/grafana/grafana/blob/main/packages/grafana-data/src/types/trace.ts#L12).
