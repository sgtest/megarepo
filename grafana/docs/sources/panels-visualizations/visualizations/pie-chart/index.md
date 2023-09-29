---
aliases:
  - ../../panels/visualizations/pie-chart-pane/
  - ../../visualizations/pie-chart-panel/
keywords:
  - grafana
  - pie chart
labels:
  products:
    - cloud
    - enterprise
    - oss
title: Pie chart
weight: 100
---

# Pie chart

{{< figure src="/static/img/docs/pie-chart-panel/pie-chart-example.png" max-width="1200px" lightbox="true" caption="Pie charts" >}}

Pie charts display reduced series, or values in a series, from one or more queries, as they relate to each other, in the form of slices of a pie. The arc length, area and central angle of a slice are all proportional to the slices value, as it relates to the sum of all values. This type of chart is best used when you want a quick comparison of a small set of values in an aesthetically pleasing form.

## Value options

Use the following options to refine the value in your visualization.

### Show

Choose how much information to show.

- **Calculate -** Reduces each value to a single value per series.
- **All values -** Displays every value from a single series.

### Calculation

Select a calculation to reduce each series when Calculate has been selected. For information about available calculations, refer to [Calculation types][].

### Limit

When displaying every value from a single series, this limits the number of values displayed.

### Fields

Select which field or fields to display in the visualization. Each field name is available on the list, or you can select one of the following options:

- **Numeric fields -** All fields with numerical values.
- **All fields -** All fields that are not removed by transformations.
- **Time -** All fields with time values.

## Pie chart options

Use these options to refine how your visualization looks.

### Pie chart type

Select the pie chart display style.

### Pie

![Pie type chart](/static/img/docs/pie-chart-panel/pie-type-chart-7-5.png)

### Donut

![Donut type chart](/static/img/docs/pie-chart-panel/donut-type-chart-7-5.png)

### Labels

Select labels to display on the pie chart. You can select more than one.

- **Name -** The series or field name.
- **Percent -** The percentage of the whole.
- **Value -** The raw numerical value.

Labels are displayed in white over the body of the chart. You might need to select darker chart colors to make them more visible. Long names or numbers might be clipped.

The following example shows a pie chart with **Name** and **Percent** labels displayed.

![Pie chart labels](/static/img/docs/pie-chart-panel/pie-chart-labels-7-5.png)

{{< docs/shared lookup="visualizations/tooltip-mode.md" source="grafana" version="<GRAFANA VERSION>" >}}

## Legend options

Use these settings to define how the legend appears in your visualization. For more information about the legend, refer to [Configure a legend]({{< relref "../../configure-legend" >}}).

### Legend visibility

Use the **Visibility** switch to show or hide the legend.

### Legend mode

Set the display mode of the legend:

- **List -** Displays the legend as a list. This is a default display mode of the legend.
- **Table -** Displays the legend as a table.

### Legend placement

Choose where to display the legend.

- **Bottom -** Below the graph.
- **Right -** To the right of the graph.

### Legend values

Select values to display in the legend. You can select more than one.

- **Percent:** The percentage of the whole.
- **Value:** The raw numerical value.

{{% docs/reference %}}
[Calculation types]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/calculation-types"
[Calculation types]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/calculation-types"
{{% /docs/reference %}}
