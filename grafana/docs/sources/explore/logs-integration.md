---
description: Logs in Explore
keywords:
  - explore
  - logs
title: Logs in Explore
weight: 15
---

# Logs in Explore

Along with metrics, Explore allows you to investigate your logs across several data sources including:

- [Elasticsearch]({{< relref "../datasources/elasticsearch/" >}})
- [InfluxDB]({{< relref "../datasources/influxdb/" >}})
- [Loki]({{< relref "../datasources/loki/" >}})

During monitoring, troubleshooting, and incident response, you can dig deeper into the metrics and logs to find the cause of the issue. Explore also allows you to correlate metrics and logs by viewing them side-by-side.

## Logs visualization

Results of log queries are shown as histograms in the graph and individual logs are explained in the following sections.

If the data source supports a full range log volume histogram, the graph with log distribution for all entered log queries is shown automatically. This feature is currently supported by Elasticsearch and Loki data sources.

> **Note:** In Loki, this full range log volume histogram is rendered by metric query which can be expensive depending on time range queried. This query may be particularly challenging for smaller Loki installations to process. To mitigate this, we recommend using a proxy like [nginx](https://www.nginx.com/) in front of Loki to set a custom timeout (for example, 10 seconds) for these queries. Log volume histogram queries can be identified by looking for queries with the HTTP header `X-Query-Tags` with value `Source=logvolhist`; these headers are added by Grafana to all log volume histogram queries.

If the data source does not support loading full range log volume histogram, the logs model computes a time series based on the log row counts bucketed by an automatically calculated time interval, and the first log row's timestamp then anchors the start of the histogram from the result. The end of the time series is anchored to the time picker's **To** range.

### Log level

For logs where a level label is specified, we use the value of the label to determine the log level and update color accordingly. If the log doesn't have a level label specified, we try to find out if its content matches any of the supported expressions (see below for more information). The log level is always determined by the first match. In case Grafana is not able to determine a log level, it will be visualized with an unknown log level.

> **Tip:** If you use Loki data source and the "level" is in your log-line, use parsers (JSON, logfmt, regex,..) to extract the level information into a level label that is used to determine log level. This will allow the histogram to show the various log levels in separate bars.

**Supported log levels and mapping of log level abbreviation and expressions:**

| Supported expressions | Log level |      Color |
| --------------------- | :-------: | ---------: |
| emerg                 | critical  |     purple |
| fatal                 | critical  |     purple |
| alert                 | critical  |     purple |
| crit                  | critical  |     purple |
| critical              | critical  |     purple |
| err                   |   error   |        red |
| eror                  |   error   |        red |
| error                 |   error   |        red |
| warn                  |  warning  |     yellow |
| warning               |  warning  |     yellow |
| info                  |   info    |      green |
| information           |   info    |      green |
| informational         |   info    |      green |
| notice                |   info    |      green |
| dbug                  |   debug   |       blue |
| debug                 |   debug   |       blue |
| trace                 |   trace   | light blue |
| \*                    |  unknown  |       grey |

## Logs navigation

Logs navigation, at the right side of the log lines, can be used to request more logs. You can do this by clicking the **Older logs** button at the bottom of the navigation. This is especially useful when you hit the line limit and you want to see more logs. Each request that's run from the navigation is then displayed in the navigation as separate page. Every page shows `from` and `to` timestamps of the incoming log lines. You can see previous results by clicking on the page. Explore caches the last five requests run from the logs navigation, so you're not re-running the same queries when clicking on the pages.

![Navigate logs in Explore](/static/img/docs/explore/navigate-logs-8-0.png)

## Visualization options

You can customize how logs are displayed and select which columns are shown.

### Time

Shows or hides the time column. This is the timestamp associated with the log line as reported from the data source.

### Unique labels

Shows or hides the unique labels column that includes only non-common labels. All common labels are displayed above.

### Wrap lines

Set this to True if you want the display to use line wrapping. If set to False, it will result in horizontal scrolling.

### Prettify JSON

Set this to `true` to pretty print all JSON logs. This setting does not affect logs in any format other than JSON.

### Deduplication

Log data can be very repetitive and Explore can help by hiding duplicate log lines. There are a few different deduplication algorithms that you can use:

- **Exact -** Exact matches are done on the whole line except for date fields.
- **Numbers -** Matches on the line after stripping out numbers such as durations, IP addresses, and so on.
- **Signature -** The most aggressive deduplication, this strips all letters and numbers and matches on the remaining whitespace and punctuation.

### Display results order

You can change the order of received logs from the default descending order (newest first) to ascending order (oldest first).

## Labels and detected fields

Each log line has an extendable area, with its labels and detected fields, for more robust interaction. You can filter for (positive filter) and filter out (negative filter) selected labels. Additionally, you can select a unique field to visualize instead of the whole log line by clicking on the eye icon.
Finally, each field or label also has a stats icon to display ad-hoc statistics in relation to all displayed logs.

## Escaping newlines

Explore automatically detects some incorrectly escaped sequences in log lines, such as newlines (`\n`, `\r`) or tabs (`\t`). When it detects such sequences, Explore provides an "Escape newlines" option.

To automatically fix incorrectly escaped sequences that Explore has detected:

1. Click "Escape newlines" to replace the sequences.
2. Manually review the replacements to confirm their correctness.

Explore replaces these sequences. When it does so, the option will change from "Escape newlines" to "Remove escaping". Evaluate the changes as the parsing may not be accurate based on the input received. You can revert the replacements by clicking "Remove escaping".

## Data links

By using data links, you can turn any part of a log message into an internal or external link. The created link is visible as a button in the **Links** section inside the **Log details** view.
{{< figure src="/static/img/docs/explore/data-link-9-4.png" max-width="800px" caption="Data link in Explore" >}}

## Log context

Log context is a feature that allows you to display additional lines of context surrounding a log entry that matches a particular search query. This can be helpful in understanding the log entry's context, and is similar to the `-C` parameter in the `grep` command.

When using Log context in Grafana, you can configure the number of lines of context to display before and after the matching log entry. By default, the Log context feature will show the log entry itself along with the 50 lines before and after it. However, this can be adjusted as needed depending on the specific use case.

You may encounter long lines of text that make it difficult to read and analyze the context around each log entry. This is where the **Wrap lines** toggle can come in handy. By enabling this toggle, Grafana will automatically wrap long lines of text so that they fit within the visible width of the viewer. This can make it easier to read and understand the log entries.

The **Open in split view** button allows you to execute the context query for a log entry in a split screen in the Explore view. Clicking this button will open a new Explore pane with the context query displayed alongside the log entry, making it easier to analyze and understand the surrounding context.

## Copy link to log line

Linking of log lines in Grafana allows you to quickly navigate to specific log entries for precise analysis. By clicking the **Copy shortcut** 🔗 button for a log line, you can generate and copy a [short URL]({{< relref "../developers/http_api/short_url/" >}}) that provides direct access to the exact log entry within an absolute time range. When you open the link, Grafana will automatically scroll to the corresponding log line and highlight it with a blue background, making it easy to identify and focus on the relevant information.

## Loki-specific features

As mentioned, one of the log integrations is for the new open source log aggregation system from Grafana Labs - [Loki](https://github.com/grafana/loki). Loki is designed to be very cost effective, as it does not index the contents of the logs, but rather a set of labels for each log stream. The logs from Loki are queried in a similar way to querying with label selectors in Prometheus. It uses labels to group log streams which can be made to match up with your Prometheus labels. For more information about Grafana Loki, refer to [Grafana Loki](https://github.com/grafana/loki) or the Grafana Labs hosted variant: [Grafana Cloud Logs](https://grafana.com/loki).

For more information, refer to [Loki's data source documentation]({{< relref "../datasources/loki/" >}}) on how to query for log data.

### Switch from metrics to logs

If you switch from a Prometheus query to a logs query (you can do a split first to have your metrics and logs side by side) then it will keep the labels from your query that exist in the logs and use those to query the log streams. For example, the following Prometheus query:

`grafana_alerting_active_alerts{job="grafana"}`

after switching to the Logs data source, the query changes to:

`{job="grafana"}`

This will return a chunk of logs in the selected time range that can be grepped/text searched.

### Logs sample

If the selected data source implements logs sample, and supports both log and metric queries, then for metric queries you will be able to automatically see samples of log lines that contributed to visualized metrics. This feature is currently supported by Loki data sources.

### Live tailing

Use the Live tailing feature to see real-time logs on supported data sources.

Click the **Live** button in the Explore toolbar to switch to Live tail view.

While in Live tail view new logs will come from the bottom of the screen and will have fading contrasting background so you can keep track of what is new. Click the **Pause** button or scroll the logs view to pause the Live tailing and explore previous logs without interruption. Click **Resume** button to resume the Live tailing or click **Stop** button to exit Live tailing and go back to standard Explore view.

{{< video-embed src="/static/img/docs/v95/explore_live_tailing.mp4" >}}
