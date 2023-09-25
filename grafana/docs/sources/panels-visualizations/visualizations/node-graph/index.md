---
aliases:
  - ../../panels/visualizations/node-graph/
  - ../../visualizations/node-graph/
keywords:
  - grafana
  - dashboard
  - documentation
  - panels
  - node graph
  - directed graph
labels:
  products:
    - cloud
    - enterprise
    - oss
title: Node graph
weight: 100
---

# Node graph

Node graphs can visualize directed graphs or networks. They use a directed force layout to effectively position the nodes, so they can display complex infrastructure maps, hierarchies, or execution diagrams.

![Node graph visualization](/static/img/docs/node-graph/node-graph-8-0.png 'Node graph')

## Data requirements

A node graph requires a specific shape of the data to be able to display its nodes and edges. This means not every data source or query can be visualized with this graph. If you want to use this as a data source developer see the section about data API.

A node graph consists of _nodes_ and _edges_.

- A _node_ is displayed as a circle. A node might represent an application, a service, or anything else that is relevant from an application perspective.
- An _edge_ is displayed as a line that connects two nodes. The connection might be a request, an execution, or some other relationship between the two nodes.

Both nodes and edges can have associated metadata or statistics. The data source defines what information and values is shown, so different data sources can show different type of values or not show some values.

### Nodes

{{% admonition type="note" %}}
Node graphs can show only 1,500 nodes. If this limit is crossed a warning will be visible in upper right corner, and some nodes will be hidden. You can expand hidden parts of the graph by clicking on the "Hidden nodes" markers in the graph.
{{% /admonition %}}

Usually, nodes show two statistical values inside the node and two identifiers just below the node, usually name and type. Nodes can also show another set of values as a color circle around the node, with sections of different color represents different values that should add up to 1.

For example, you can have the percentage of errors represented by a red portion of the circle. Additional details can be displayed in a context menu which is displayed when you click on the node. There also can be additional links in the context menu that can target either other parts of Grafana or any external link.

![Node graph navigation](/static/img/docs/node-graph/node-graph-navigation-7-4.gif 'Node graph navigation')

### Edges

Edges can also show statistics when you hover over the edge. Similar to nodes, you can open a context menu with additional details and links by clicking on the edge.

The first data source supporting this visualization is X-Ray data source for its Service map feature. For more information, refer to the [X-Ray plugin documentation](https://grafana.com/grafana/plugins/grafana-x-ray-datasource).

## Navigating the node graph

You can pan and zoom in or out a node graph.

### Pan

You can pan the view by clicking outside any node or edge and dragging your mouse.

### Zoom in or out

Use the buttons in the upper left corner or use the mouse wheel, touchpad scroll, together with either Ctrl or Cmd key to zoom in or out.

### Explore hidden nodes

The number of nodes shown at a given time is limited to maintain a reasonable visualization performance. Nodes that are not currently visible are hidden behind clickable markers that show an approximate number of hidden nodes that are connected by a particular edge. You can click on the marker to expand the graph around that node.

![Node graph exploration](/static/img/docs/node-graph/node-graph-exploration-8-0.png 'Node graph exploration')

### Grid view

You can switch to the grid view to have a better overview of the most interesting nodes in the graph. Grid view shows nodes in a grid without edges and can be sorted by stats shown inside the node or by stats represented by the a colored border of the nodes.

![Node graph grid](/static/img/docs/node-graph/node-graph-grid-8-0.png 'Node graph grid')

To sort the nodes, click on the stats inside the legend. The marker next to the stat name shows which stat is currently used for sorting and sorting direction.

![Node graph legend](/static/img/docs/node-graph/node-graph-legend-8-0.png 'Node graph legend')

Click on the node and select "Show in Graph layout" option to switch back to graph layout and focus on the selected node, to show it in context of the full graph.

![Node graph grid to default](/static/img/docs/node-graph/node-graph-grid-to-default-8-0.png 'Node graph grid to default')

## Data API

This visualization needs a specific shape of the data to be returned from the data source in order to correctly display it.

Node graphs, at minimum, require a data frame describing the edges of the graph. By default, node graphs will compute the nodes and any stats based on this data frame. Optionally a second data frame describing the nodes can be sent in case there is need to show more node specific metadata. You have to set `frame.meta.preferredVisualisationType = 'nodeGraph'` on both data frames or name them `nodes` and `edges` respectively for the node graph to render.

### Edges data frame structure

Required fields:

| Field name | Type   | Description                    |
| ---------- | ------ | ------------------------------ |
| id         | string | Unique identifier of the edge. |
| source     | string | Id of the source node.         |
| target     | string | Id of the target.              |

Optional fields:

| Field name    | Type          | Description                                                                                                                                                                                         |
| ------------- | ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| mainstat      | string/number | First stat shown in the overlay when hovering over the edge. It can be a string showing the value as is or it can be a number. If it is a number, any unit associated with that field is also shown |
| secondarystat | string/number | Same as mainStat, but shown right under it.                                                                                                                                                         |
| detail\_\_\*  | string/number | Any field prefixed with `detail__` will be shown in the header of context menu when clicked on the edge. Use `config.displayName` for more human readable label.                                    |

### Nodes data frame structure

Required fields:

| Field name | Type   | Description                                                                                  |
| ---------- | ------ | -------------------------------------------------------------------------------------------- |
| id         | string | Unique identifier of the node. This ID is referenced by edge in its source and target field. |

Optional fields:

| Field name    | Type          | Description                                                                                                                                                                                                                                                                                                                                                               |
| ------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| title         | string        | Name of the node visible in just under the node.                                                                                                                                                                                                                                                                                                                          |
| subtitle      | string        | Additional, name, type or other identifier shown under the title.                                                                                                                                                                                                                                                                                                         |
| mainstat      | string/number | First stat shown inside the node itself. It can either be a string showing the value as is or a number. If it is a number, any unit associated with that field is also shown.                                                                                                                                                                                             |
| secondarystat | string/number | Same as mainStat, but shown under it inside the node.                                                                                                                                                                                                                                                                                                                     |
| arc\_\_\*     | number        | Any field prefixed with `arc__` will be used to create the color circle around the node. All values in these fields should add up to 1. You can specify color using `config.color.fixedColor`.                                                                                                                                                                            |
| detail\_\_\*  | string/number | Any field prefixed with `detail__` will be shown in the header of context menu when clicked on the node. Use `config.displayName` for more human readable label.                                                                                                                                                                                                          |
| color         | string/number | Can be used to specify a single color instead of using the `arc__` fields to specify color sections. It can be either a string which should then be an acceptable HTML color string or it can be a number in which case the behaviour depends on `field.config.color.mode` setting. This can be for example used to create gradient colors controlled by the field value. |
| icon          | string        | Name of the icon to show inside the node instead of the default stats. Only Grafana built in icons are allowed (see the available icons [here](https://developers.grafana.com/ui/latest/index.html?path=/story/docs-overview-icon--icons-overview)).                                                                                                                      |
| nodeRadius    | number        | Radius value in pixels. Used to manage node size.                                                                                                                                                                                                                                                                                                                         |
