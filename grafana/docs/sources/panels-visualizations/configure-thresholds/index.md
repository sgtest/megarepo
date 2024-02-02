---
aliases:
  - ../panels/
  - ../panels/configure-thresholds/
  - ../panels/specify-thresholds/about-thresholds/
  - ../panels/specify-thresholds/add-a-threshold/
  - ../panels/specify-thresholds/add-threshold-to-graph/
  - ../panels/specify-thresholds/delete-a-threshold/
  - ../panels/thresholds/
description: Configure thresholds in your visualizations
labels:
  products:
    - cloud
    - enterprise
    - oss
menuTitle: Configure thresholds
title: Configure thresholds
weight: 100
---

# Configure thresholds

In dashboards, a threshold is a value or limit you set for a metric that's reflected visually when it's met or exceeded. Thresholds are one way you can conditionally style and color your visualizations based on query results.

Using thresholds, you can color grid lines and regions in a time series visualization:
![Time series visualization with green, blue, and purple threshold lines and regions](/media/docs/grafana/panels-visualizations/screenshot-thresholds-lines-regions-v10.4.png)

You can color the background or value text in a stat visualization:
![Stat visualization with three values in green and orange](/media/docs/grafana/panels-visualizations/screenshot-thresholds-value-v10.4.png)

You can define regions and region colors in a state timeline:
![State timeline with green, blue, and pink region thresholds](/media/docs/grafana/panels-visualizations/screenshot-thresholds-state-timeline-v10.4.png)

You can also use thresholds to:

- Color lines in a time series visualization
- Color the gauge and threshold markers in a gauge
- Color markers in a geomap
- Color cell text or background in a table

## Supported visualizations

You can set thresholds in the following visualizations:

|                            |                                  |                                  |
| -------------------------- | -------------------------------- | -------------------------------- |
| [Bar chart][bar chart]     | [Geomap][geomap]                 | [Status history][status history] |
| [Bar gauge][bar gauge]     | [Histogram][histogram]           | [Table][table]                   |
| [Candlestick][candlestick] | [Stat][stat]                     | [Time series][time series]       |
| [Canvas][canvas]           | [State timeline][state timeline] | [Trend][trend]                   |
| [Gauge][gauge]             |

## Default thresholds

On visualizations that support thresholds, Grafana has the following default threshold settings:

- 80 = red
- Base = green
- Mode = Absolute
- Show thresholds = Off (for some visualizations); for more information, see the [Show thresholds](#show-threshold) option.

## Thresholds options

You can set the following options to further define how thresholds look.

### Threshold value

This number is the value that triggers the threshold. You can also set the color associated with the threshold in this field.

The **Base** value represents minus infinity. By default, it's set to the color green, which is generally the “good” color.

### Thresholds mode

There are two threshold modes:

- **Absolute** thresholds are defined by a number. For example, 80 on a scale of 1 to 150.
- **Percentage** thresholds are defined relative to minimum or maximum. For example, 80 percent.

### Show thresholds

{{< admonition type="note" >}}
This option is supported for the bar chart, candlestick, time series, and trend visualizations.
{{< /admonition>}}

Set if and how thresholds are shown with the following options.

| Option                               | Example                                                                                                                                                                                              |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Off                                  |                                                                                                                                                                                                      |
| As lines                             | {{< figure max-width="500px" src="/media/docs/grafana/panels-visualizations/screenshot-thresholds-lines-v10.4.png" alt="Visualization with threshold as a line" >}}                                  |
| As lines (dashed)                    | {{< figure max-width="500px" src="/media/docs/grafana/panels-visualizations/screenshot-thresholds-dashed-lines-v10.4.png" alt="Visualization with threshold as a dashed line" >}}                    |
| As filled regions                    | {{< figure max-width="500px" src="/media/docs/grafana/panels-visualizations/screenshot-thresholds-regions-v10.4.png" alt="Visualization with threshold as a region" >}}                              |
| As filled regions and lines          | {{< figure max-width="500px" src="/media/docs/grafana/panels-visualizations/screenshot-thresholds-lines-regions-v10.4.png" alt="Visualization with threshold as a region and line" >}}               |
| As filled regions and lines (dashed) | {{< figure max-width="500px" src="/media/docs/grafana/panels-visualizations/screenshot-thresholds-dashed-lines-regions-v10.4.png" alt="Visualization with threshold as a region and dashed line" >}} |

## Add a threshold

You can add as many thresholds to a visualization as you want. Grafana automatically sorts thresholds values from highest to lowest.

1. Navigate to the panel you want to update.
1. Hover over any part of the panel you want to work on to display the menu on the top right corner.
1. Click the menu and select **Edit**.
1. Scroll to the **Thresholds** section or enter `thresholds` in the search bar at the top of the panel edit pane.
1. Click **+ Add threshold**.
1. Enter a new threshold value or use the up and down arrows at the right side of the field to increase or decrease the value incrementally.
1. Click the colored circle to the left of the threshold value to open the color picker, where you can update the threshold color.
1. Under **Thresholds mode**, select either **Absolute** or **Percentage**.
1. Under **Show thresholds**, set how the threshold is displayed or turn it off.

To delete a threshold, navigate to the panel that contains the threshold and click the trash icon next to the threshold you want to remove.

## Add a threshold to a legacy graph panel

{{< admonition type="caution" >}}
Starting with Grafana v11, the legacy graph panel will be deprecated along with all other Angular panel plugins. For more information, refer to [Angular support deprecation](https://grafana.com/docs/grafana/<GRAFANA_VERSION>/developers/angular_deprecation/).
{{< /admonition >}}

In the Graph panel visualization, thresholds enable you to add lines or sections to a graph to make it easier to recognize when the graph crosses a threshold.

1. Navigate to the graph panel to which you want to add a threshold.
1. On the **Panel** tab, click **Thresholds**.
1. Click **Add threshold**.
1. Complete the following fields:
   - **T1 -** Both values are required to display a threshold.
     - **lt** or **gt** - Select **lt** for less than or **gt** for greater than to indicate what the threshold applies to.
     - **Value -** Enter a threshold value. Grafana draws a threshold line along the Y-axis at that value.
   - **Color -** Choose a condition that corresponds to a color, or define your own color.
     - **custom -** You define the fill color and line color.
     - **critical -** Fill and line color are red.
     - **warning -** Fill and line color are yellow.
     - **ok -** Fill and line color are green.
   - **Fill -** Toggle the display of the threshold fill.
   - **Line -** Toggle the display of the threshold line.
   - **Y-Axis -** Choose to display the y-axis on either the **left** or **right** of the panel.
1. Click **Save** to save the changes in the dashboard.

{{% docs/reference %}}
[bar chart]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/bar-chart"
[bar chart]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/bar-chart"

[bar gauge]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/bar-gauge"
[bar gauge]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/bar-gauge"

[candlestick]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/candlestick"
[candlestick]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/candlestick"

[canvas]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/canvas"
[canvas]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/canvas"

[gauge]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/gauge"
[gauge]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/gauge"

[geomap]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/geomap"
[geomap]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/geomap"

[histogram]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/histogram"
[histogram]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/histogram"

[stat]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/stat"
[stat]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/stat"

[state timeline]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/state-timeline"
[state timeline]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/state-timeline"

[status history]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/status-history"
[status history]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/status-history"

[table]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/table"
[table]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/table"

[time series]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/time-series"
[time series]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/time-series"

[trend]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/trend"
[trend]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/visualizations/panels-visualizations/visualizations/trend"
{{% /docs/reference %}}
