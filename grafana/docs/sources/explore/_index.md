---
aliases:
  - features/explore/
keywords:
  - explore
  - loki
  - logs
title: Explore
weight: 90
---

# Explore

Grafana's dashboard UI is all about building dashboards for visualization. Explore strips away the dashboard and panel options so that you can focus on the query. It helps you iterate until you have a working query and then think about building a dashboard.

> Refer to [Role-based access control]({{< relref "../administration/roles-and-permissions/access-control/" >}}) in Grafana Enterprise to understand how you can control access with role-based permissions.

If you just want to explore your data and do not want to create a dashboard, then Explore makes this much easier. If your data source supports graph and table data, then Explore shows the results both as a graph and a table. This allows you to see trends in the data and more details at the same time. See also:

- [Query management in Explore]({{< relref "query-management/" >}})
- [Logs integration in Explore]({{< relref "logs-integration/" >}})
- [Trace integration in Explore]({{< relref "trace-integration/" >}})
- [Inspector in Explore]({{< relref "explore-inspector/" >}})

## Start exploring

> Refer to [Role-based access Control]({{< relref "../administration/roles-and-permissions/access-control/" >}}) in Grafana Enterprise to understand how you can manage Explore with role-based permissions.

In order to access Explore, you must have an editor or an administrator role, unless the [viewers_can_edit option]({{< relref "../setup-grafana/configure-grafana/#viewers_can_edit" >}}) is enabled. Refer to [About users and permissions]({{< relref "../administration/roles-and-permissions/" >}}) for more information on what each role has access to.

To access Explore:

1. Click on the Explore icon on the menu bar.

   An empty Explore tab opens.

   Alternately to start with an existing query in a panel, choose the Explore option from the Panel menu. This opens an Explore tab with the query from the panel and allows you to tweak or iterate in the query outside of your dashboard.

   {{< figure src="/static/img/docs/explore/panel_dropdown-7-4.png" class="docs-image--no-shadow" max-width= "650px" caption="Screenshot of the new Explore option in the panel menu" >}}

1. Choose your data source from the dropdown in the top left.
1. Write the query using a query editor provided by the selected data source. Please check [data sources documentation]({{< relref "../datasources" >}}) to see how to use various query editors.
1. For general documentation on querying data sources in Grafana, see [Query and transform data]({{< relref "../panels-visualizations/query-transform-data" >}}).
1. Run the query using the button in the top right corner.

## Split and compare

The split view provides an easy way to compare visualizations side-by-side or to look at related data together on one page.

To open the split view:

1. Click the split button to duplicate the current query and split the page into two side-by-side queries.

It is possible to select another data source for the new query which for example, allows you to compare the same query for two different servers or to compare the staging environment to the production environment.

{{< figure src="/static/img/docs/explore/explore_split-7-4.png" max-width= "950px" caption="Screenshot of Explore option in the panel menu" >}}

In split view, timepickers for both panels can be linked (if you change one, the other gets changed as well) by clicking on one of the time-sync buttons attached to the timepickers. Linking of timepickers helps with keeping the start and the end times of the split view queries in sync. It ensures that you’re looking at the same time interval in both split panels.

To close the newly created query, click on the Close Split button.

## Share shortened link

{{% admonition type="note" %}}
Available in Grafana 7.3 and later versions.
{{% /admonition %}}

The Share shortened link capability allows you to create smaller and simpler URLs of the format /goto/:uid instead of using longer URLs with query parameters. To create a shortened link to the executed query, click the **Share** option in the Explore toolbar. A shortened link that is never used will automatically get deleted after seven (7) days.

## Available feature toggles

### exploreMixedDatasource

Enabled by default, allows users in Explore to have different data sources for different queries. If compatible, results will be combined.

Learn more about how to use [Mixed data source]({{< relref "../datasources/#special-data-sources" >}}).
