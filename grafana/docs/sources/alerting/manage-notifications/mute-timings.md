---
aliases:
  - ../notifications/mute-timings/
  - ../unified-alerting/notifications/mute-timings/
canonical: https://grafana.com/docs/grafana/latest/alerting/manage-notifications/mute-timings/
description: Create mute timings to prevent alerts from firing during a specific and reoccurring period of time
keywords:
  - grafana
  - alerting
  - guide
  - mute
  - mute timings
  - mute time interval
labels:
  products:
    - cloud
    - enterprise
    - oss
title: Create mute timings
weight: 420
---

# Create mute timings

A mute timing is a recurring interval of time when no new notifications for a policy are generated or sent. Use them to prevent alerts from firing a specific and reoccurring period, for example, a regular maintenance period.

Similar to silences, mute timings do not prevent alert rules from being evaluated, nor do they stop alert instances from being shown in the user interface. They only prevent notifications from being created.

You can configure Grafana managed mute timings as well as mute timings for an [external Alertmanager data source][datasources/alertmanager]. For more information, refer to [Alertmanager documentation][fundamentals/alertmanager].

## Mute timings vs silences

The following table highlights the key differences between mute timings and silences.

| Mute timing                                        | Silence                                                                      |
| -------------------------------------------------- | ---------------------------------------------------------------------------- |
| Uses time interval definitions that can reoccur    | Has a fixed start and end time                                               |
| Is created and then added to notification policies | Uses labels to match against an alert to determine whether to silence or not |

## Add mute timings

1. In the left-side menu, click **Alerts & IRM**, and then **Alerting**.
1. Click **Notification policies** and then the **Mute Timings** tab.
1. From the **Alertmanager** dropdown, select an external Alertmanager. By default, the **Grafana Alertmanager** is selected.
1. Click **+ Add mute timing**.
1. Fill out the form to create a [time interval](#time-intervals) to match against for your mute timing.
1. Save your mute timing.

## Add mute timing to a notification policy

1. In the left-side menu, click **Alerts & IRM**, and then **Alerting**.
1. Click **Notification policies** and make sure you are on the **Notification Policies** tab.
1. Find the notification policy you would like to add the mute timing to and click **...** -> **Edit**.
1. From the **Mute timings** dropdown, choose the mute timings you would like to add to the policy.
1. Save your changes.

## Time intervals

A time interval is a specific duration during which alerts are suppressed. The duration typically consists of a specific time range and the days of the week, month, or year.

Supported time interval options are:

- Time range: The time inclusive of the start and exclusive of the end time (in UTC if no location has been selected, otherwise local time).
- Location: Depending on the location you select, the time range is displayed in local time.
- Days of the week: The day or range of days of the week. Example: `monday:thursday`.
- Days of the month: The date 1-31 of a month. Negative values can also be used to represent days that begin at the end of the month. For example: `-1` for the last day of the month.
- Months: The months of the year in either numerical or the full calendar month. For example: `1, may:august`.
- Years: The year or years for the interval. For example: `2021:2024`.

All fields are lists; to match the field, at least one list element must be satisfied. Fields also support ranges using `:` (e.g., `monday:thursday`).

If a field is left blank, any moment of time will match the field. For an instant of time to match a complete time interval, all fields must match. A mute timing can contain multiple time intervals.

If you want to specify an exact duration, specify all the options. For example, if you wanted to create a time interval for the first Monday of the month, for March, June, September, and December, between the hours of 12:00 and 24:00 UTC your time interval specification would be:

- Time range:
  - Start time: `12:00`
  - End time: `24:00`
- Days of the week: `monday`
- Months: `3, 6, 9, 12`
- Days of the month: `1:7`

{{% docs/reference %}}
[datasources/alertmanager]: "/docs/grafana/ -> /docs/grafana/<GRAFANA_VERSION>/datasources/alertmanager"
[datasources/alertmanager]: "/docs/grafana-cloud/ -> /docs/grafana/<GRAFANA_VERSION>/datasources/alertmanager"

[fundamentals/alertmanager]: "/docs/grafana/ -> /docs/grafana/<GRAFANA_VERSION>/alerting/fundamentals/alertmanager"
[fundamentals/alertmanager]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/alerting-and-irm/alerting/fundamentals/alertmanager"
{{% /docs/reference %}}
