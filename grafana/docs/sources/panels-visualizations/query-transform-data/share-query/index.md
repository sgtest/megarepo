---
aliases:
  - ../../panels/query-a-data-source/share-query/
labels:
  products:
    - cloud
    - enterprise
    - oss
menuTitle: Share query results
title: Share query results with another panel
weight: 60
---

# Share query results with another panel

Grafana lets you use the query result from one panel for any other panel in the dashboard. Sharing query results across panels reduces the number of queries made to your data source, which can improve the performance of your dashboard.

The Dashboard data source lets you select a panel in your dashboard that contains the queries ‌you want to share the results for. Instead of sending a separate query for each panel, Grafana sends one query and other panels use the query results to construct visualizations.

This strategy can drastically reduce the number of queries being made when you for example have several panels visualizing the same data.

1. [Create a dashboard][].
1. Change the title to "Source panel". You'll use this panel as a source for the other panels.
1. Define the [query][] or queries that you want share.

   If you don't have a data source available, use the **Grafana** data source, which returns a random time series that you can use for testing.

1. Add a new panel and select the **Dashboard** data source in the query editor.
1. In the **Use results from panel list**, select the first panel you created.

All queries defined in the source panel are now available to the new panel. Queries defined in the source panel can be shared with multiple panels.

You can click on any of the queries to go to the panel where they are defined.

{{% docs/reference %}}
[Create a dashboard]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/dashboards/build-dashboards/create-dashboard"
[Create a dashboard]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/dashboards/build-dashboards/create-dashboard"

[query]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/query-transform-data#add-a-query"
[query]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/panels-visualizations/query-transform-data#add-a-query"
{{% /docs/reference %}}
