---
aliases:
  - ../linking/data-link-variables/
  - ../linking/data-links/
  - ../panels/configure-data-links/
  - ../reference/datalinks/
  - ../variables/url-variables/
  - ../variables/variable-types/url-variables/
keywords:
  - grafana
  - url variables
  - variables
  - data link
  - documentation
  - playlist
labels:
  products:
    - cloud
    - enterprise
    - oss
menuTitle: Configure data links
title: Configure data links
weight: 400
---

# Configure data links

You can use data link variables or data links to create links between panels.

## Data link variables

You can use variables in data links to refer to series fields, labels, and values. For more information about data links, refer to [Data links](#data-links).

To see a list of available variables, type `$` in the data link **URL** field to see a list of variables that you can use.

{{% admonition type="note" %}}
These variables changed in 6.4 so if you have an older version of Grafana, then use the version picker to select docs for an older version of Grafana.
{{% /admonition %}}

Azure Monitor, [CloudWatch][], and [Google Cloud Monitoring][] have pre-configured data links called _deep links_.

You can also use template variables in your data links URLs, refer to [Templates and variables][] for more information on template variables.

## Time range panel variables

These variables allow you to include the current time range in the data link URL.

- `__url_time_range` - current dashboard's time range (i.e. `?from=now-6h&to=now`)
- `$__from and $__to` - For more information, refer to [Global variables][].

## Series variables

Series specific variables are available under `__series` namespace:

- `__series.name` - series name to the URL

## Field variables

Field-specific variables are available under `__field` namespace:

- `__field.name` - the name of the field
- `__field.labels.<LABEL>` - label's value to the URL. If your label contains dots, then use `__field.labels["<LABEL>"]` syntax.

## Value variables

Value-specific variables are available under `__value` namespace:

- `__value.time` - value's timestamp (Unix ms epoch) to the URL (i.e. `?time=1560268814105`)
- `__value.raw` - raw value
- `__value.numeric` - numeric representation of a value
- `__value.text` - text representation of a value
- `__value.calc` - calculation name if the value is result of calculation

## Template variables

When linking to another dashboard that uses template variables, select variable values for whoever clicks the link.

`${var-myvar:queryparam}` - where `var-myvar` is the name of the template variable that matches one in the current dashboard that you want to use.

| Variable state           | Result in the created URL           |
| ------------------------ | ----------------------------------- |
| selected one value       | `var-myvar=value1`                  |
| selected multiple values | `var-myvar=value1&var-myvar=value2` |
| selected `All`           | `var-myvar=All`                     |

If you want to add all of the current dashboard's variables to the URL, then use `${__all_variables}`.

## Data links

Data links allow you to provide more granular context to your links. You can create links that include the series name or even the value under the cursor. For example, if your visualization showed four servers, you could add a data link to one or two of them.

The link itself is accessible in different ways depending on the visualization. For the Graph you need to click on a data point or line, for a panel like
Stat, Gauge, or Bar Gauge you can click anywhere on the visualization to open the context menu.

You can use variables in data links to send people to a detailed dashboard with preserved data filters. For example, you could use variables to specify a time range, series, and variable selection. For more information, refer to [Data link variables](#data-link-variables).

### Typeahead suggestions

When creating or updating a data link, press Cmd+Space or Ctrl+Space on your keyboard to open the typeahead suggestions to more easily add variables to your URL.

{{< figure src="/static/img/docs/data_link_typeahead.png"  max-width= "800px" >}}

### Add a data link

1. Hover over any part of the panel you want to which you want to add the data link to display the actions menu on the top right corner.
1. Click the menu and select **Edit**.

   To use a keyboard shortcut to open the panel, hover over the panel and press `e`.

1. Scroll down to the Data links section and expand it.
1. Click **Add link**.
1. Enter a **Title**. **Title** is a human-readable label for the link that will be displayed in the UI.
1. Enter the **URL** you want to link to.

   You can even add one of the template variables defined in the dashboard. Click in the **URL** field and then type `$` or press Ctrl+Space or Cmd+Space to see a list of available variables. By adding template variables to your panel link, the link sends the user to the right context, with the relevant variables already set. For more information, refer to [Data link variables](#data-link-variables).

1. If you want the link to open in a new tab, then select **Open in a new tab**.
1. Click **Save** to save changes and close the window.
1. Click **Save** in the upper right to save your changes to the dashboard.

### Update a data link

1. Scroll down to the Data links section, expand it, and find the link that you want to make changes to.
1. Click the Edit (pencil) icon to open the Edit link window.
1. Make any necessary changes.
1. Click **Save** to save changes and close the window.
1. Click **Save** in the upper right to save your changes to the dashboard.

### Delete a data link

1. Scroll down to the Data links section, expand it, and find the link that you want to delete.
1. Click the **X** icon next to the link you want to delete.
1. Click **Save** in the upper right to save your changes to the dashboard.

{{% docs/reference %}}
[Cloudwatch]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/datasources/aws-cloudwatch/query-editor#deep-link-grafana-panels-to-the-cloudwatch-console-1"
[Cloudwatch]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/connect-externally-hosted/data-sources/aws-cloudwatch/query-editor#deep-link-grafana-panels-to-the-cloudwatch-console-1"

[Google Cloud Monitoring]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/datasources/google-cloud-monitoring/query-editor#deep-link-from-grafana-panels-to-the-google-cloud-console-metrics-explorer"
[Google Cloud Monitoring]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/connect-externally-hosted/data-sources/google-cloud-monitoring/query-editor#deep-link-from-grafana-panels-to-the-google-cloud-console-metrics-explorer"

[Templates and variables]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/dashboards/variables"
[Templates and variables]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/dashboards/variables"

[Global variables]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/dashboards/variables/add-template-variables#**from-and-**to"
[Global variables]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA VERSION>/dashboards/variables/add-template-variables#**from-and-**to"
{{% /docs/reference %}}
