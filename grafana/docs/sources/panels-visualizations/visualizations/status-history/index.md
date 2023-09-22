---
aliases:
  - ../../panels/visualizations/status-history/
  - ../../visualizations/status-history/
description: Status history visualization
keywords:
  - grafana
  - docs
  - status history
  - panel
labels:
  products:
    - cloud
    - enterprise
    - oss
title: Status history
weight: 100
---

# Status history

Status histories show periodic states over time. Each field or series is rendered as a horizontal row. Boxes are rendered and centered around each value.

{{< figure src="/static/img/docs/status-history-panel/status-history-example-v8-0.png" max-width="1025px" caption="Status history example" >}}

## Supported data

A status history works with string, boolean and numerical fields or time series. A time field is required. You can use value mappings to color strings or assign text values to numerical ranges.

## Display options

Use these options to refine the visualization.

### Show values

Controls whether values are rendered inside the value boxes. Auto will render values if there is sufficient space.

### Column width

Controls the width of boxes. 1 = maximum space and 0 = minimum space.

### Line width

Controls line width of state regions.

### Fill opacity

Controls the opacity of state regions.

## Value mappings

To assign colors to boolean or string values, use the [Value mappings][].

{{< figure src="/static/img/docs/v8/value_mappings_side_editor.png" max-width="300px" caption="Value mappings side editor" >}}

## Time series data with thresholds

The visualization can be used with time series data as well. In this case, the thresholds are used to color the boxes. You can also
use gradient color schemes to color values.

{{< figure src="/static/img/docs/v8/state_timeline_time_series.png" max-width="1025px" caption="state timeline with time series" >}}

## Legend options

When the legend option is enabled it can show either the value mappings or the threshold brackets. To show the value mappings in the legend, it's important that the `Color scheme` as referenced in [Color scheme][] is set to `Single color` or `Classic palette`. To see the threshold brackets in the legend set the `Color scheme` to `From thresholds`.

{{< docs/shared lookup="visualizations/legend-mode.md" source="grafana" version="<GRAFANA VERSION>" >}}

{{% docs/reference %}}
[Value mappings]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-value-mappings"
[Value mappings]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-value-mappings"

[Color scheme]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-standard-options#color-scheme"
[Color scheme]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-standard-options#color-scheme"
{{% /docs/reference %}}
