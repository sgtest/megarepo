---
aliases:
  - ../../panels/visualizations/state-timeline/
  - ../../visualizations/state-timeline/
description: State timeline visualization
keywords:
  - grafana
  - docs
  - state timeline
  - panel
labels:
  products:
    - cloud
    - enterprise
    - oss
title: State timeline
weight: 100
---

# State timeline

State timelines show discrete state changes over time. Each field or series is rendered as its unique horizontal band. State regions can either be rendered with or without values. This visualization works well with string or boolean states but can also be used with time series. When used with time series, the thresholds are used to turn the numerical values into discrete state regions.

{{< figure src="/static/img/docs/v8/state_timeline_strings.png" max-width="1025px" caption="state timeline with string states" >}}

## State timeline options

Use these options to refine the visualization.

### Merge equal consecutive values

Controls whether Grafana merges identical values if they are next to each other.

### Show values

Controls whether values are rendered inside the state regions. Auto will render values if there is sufficient space.

### Align values

Controls value alignment inside state regions.

### Row height

Controls how much space between rows there are. 1 = no space = 0.5 = 50% space.

### Line width

Controls line width of state regions.

### Fill opacity

Controls the opacity of state regions.

{{< docs/shared lookup="visualizations/connect-null-values.md" source="grafana" version="<GRAFANA VERSION>" >}}

{{< docs/shared lookup="visualizations/disconnect-values.md" source="grafana" version="<GRAFANA VERSION>" >}}

## Value mappings

To assign colors to boolean or string values, you can use [Value mappings][].

{{< figure src="/static/img/docs/v8/value_mappings_side_editor.png" max-width="300px" caption="Value mappings side editor" >}}

## Time series data with thresholds

The visualization can be used with time series data as well. In this case, the thresholds are used to turn the time series into discrete colored state regions.

{{< figure src="/static/img/docs/v8/state_timeline_time_series.png" max-width="1025px" caption="state timeline with time series" >}}

## Legend options

When the legend option is enabled it can show either the value mappings or the threshold brackets. To show the value mappings in the legend, it's important that the `Color scheme` as referenced in [Color scheme][] is set to `Single color` or `Classic palette`. To see the threshold brackets in the legend set the `Color scheme` to `From thresholds`.

{{< docs/shared lookup="visualizations/legend-mode.md" source="grafana" version="<GRAFANA VERSION>" >}}

{{% docs/reference %}}
[Color scheme]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-standard-options#color-scheme"
[Color scheme]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-standard-options#color-scheme"

[Value mappings]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-value-mappings"
[Value mappings]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/configure-value-mappings"
{{% /docs/reference %}}
