---
aliases:
  - ../dashboards/add-organize-panels/
  - ../dashboards/dashboard-create/
  - ../features/dashboard/dashboards/
  - ../panels/add-panels-dynamically/about-repeating-panels-rows/
  - ../panels/add-panels-dynamically/configure-repeating-panels/
  - ../panels/add-panels-dynamically/configure-repeating-rows/
  - ../panels/working-with-panels/
  - ../panels/working-with-panels/add-panel/
  - ../panels/working-with-panels/navigate-inspector-panel/
  - ../panels/working-with-panels/navigate-panel-editor/
  - add-organize-panels/
keywords:
  - panel
  - dashboard
  - dynamic
  - rows
  - add
labels:
  products:
    - cloud
    - enterprise
    - oss
menuTitle: Panel editor overview
title: Panel editor overview
weight: 1
---

# Panel editor overview

In the panel editor, you can update all the elements of a visualization including the data source, queries, time range, and visualization display options.

![Panel editor](/media/docs/grafana/panels-visualizations/screenshot-panel-editor-view.png)

To add a panel in a new dashboard click **+ Add visualization** in the middle of the dashboard. To add a panel to an existing dashboard, click **Add** in the dashboard header and select **Visualization** in the dropdown:

![Add dropdown](/media/docs/grafana/dashboards/screenshot-add-dropdown-10.0.png)

## Panel editor

This section describes the areas of the Grafana panel editor.

1. Panel header: The header section lists the dashboard in which the panel appears and the following controls:

   - **Discard:** Discards changes you have made to the panel since you last saved the dashboard.
   - **Save:** Saves changes you made to the panel.
   - **Apply:** Applies changes you made and closes the panel editor, returning you to the dashboard. You will have to save the dashboard to persist the applied changes.

1. Visualization preview: The visualization preview section contains the following options:

   - **Table view:** Convert any visualization to a table so you can see the data. Table views are helpful for troubleshooting. This view only contains the raw data. It does not include transformations you might have applied to the data or the formatting options available in the [Table][] visualization.
   - **Fill:** The visualization preview fills the available space. If you change the width of the side pane or height of the bottom pane the visualization changes to fill the available space.
   - **Actual:** The visualization preview will have the exact size as the size on the dashboard. If not enough space is available, the visualization will scale down preserving the aspect ratio.
   - **Time range controls:** **Default** is either the browser local timezone or the timezone selected at a higher level.

1. Data section: The data section contains tabs where you enter queries, transform your data, and create alert rules (if applicable).

   - **Query tab:** Select your data source and enter queries here. For more information, refer to [Add a query][]. When you create a new dashboard, you'll be prompted to select a data source before you get to the panel editor. You set or update the data source in existing dashboards using the dropdown in the **Query** tab.
   - **Transform tab:** Apply data transformations. For more information, refer to [Transform data][].
   - **Alert tab:** Write alert rules. For more information, refer to [the overview of Grafana Alerting][].

1. Panel display options: The display options section contains tabs where you configure almost every aspect of your data visualization.

## Panel inspect drawer

The inspect drawer helps you understand and troubleshoot your panels. You can view the raw data for any panel, export that data to a comma-separated values (CSV) file, view query requests, and export panel and data JSON.

To access the panel inspect drawer from the edit view, hover over any part of the panel to display the actions menu on the top right corner. Click the menu and select **Inspect**.

{{% admonition type="note" %}}
Not all panel types include all tabs. For example, dashboard list panels do not have raw data to inspect, so they do not display the Stats, Data, or Query tabs.
{{% /admonition %}}

The panel inspector consists of the following options:

1. The panel inspect drawer displays opens a drawer on the right side. Click the arrow in the upper right corner to expand or reduce the drawer pane.

1. **Data tab -** Shows the raw data returned by the query with transformations applied. Field options such as overrides and value mappings are not applied by default.

1. **Stats tab -** Shows how long your query takes and how much it returns.

1. **JSON tab -** Allows you to view and copy the panel JSON, panel data JSON, and data frame structure JSON. This is useful if you are provisioning or administering Grafana.

1. **Query tab -** Shows you the requests to the server sent when Grafana queries the data source.

1. **Error tab -** Shows the error. Only visible when query returns error.

{{% docs/reference %}}
[Table]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/table"
[Table]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/visualizations/table"

[Transform data]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/query-transform-data/transform-data"
[Transform data]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/query-transform-data/transform-data"

[Add a query]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/query-transform-data#add-a-query"
[Add a query]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/query-transform-data#add-a-query"

[the overview of Grafana Alerting]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/alerting"
[the overview of Grafana Alerting]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/alerting"
{{% /docs/reference %}}
