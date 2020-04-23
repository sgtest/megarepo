# Alerts

Alerts can be configured to notify site admins when there is something wrong or noteworthy on the Sourcegraph instance. 

Alerts are configured in Grafana. (Prometheus Alertmanager may also be used, but this documentation
prefers Grafana.)

## Set up alerts in Grafana

### Configure alert channels

Before configuring specific alerts in Grafana, you must set up alert channels. Each channel
corresponds to an external service to which Grafana will push alerts.

1. Access Grafana directly as a Grafana admin:
  1. Follow [these instructions](metrics.md#accessing-grafana-directly) to access Grafana directly (instead of going through the Sourcegraph Site Admin Monitoring page as usual).
  1. Navigate to the Grafana `/login` URL (e.g., `http://localhost:3070/-/debug/grafana/login` or append `/login` to your Grafana direct access URL if different). If you are doing this for the first time, user the username and password `admin` and `admin`.
1. In the left sidebar, click the bell icon 🔔 and select `Notification channels`.
1. Click `New channel` and then specify the settings of your channel. The `Type` field selects the type of external service (e.g., PagerDuty, Slack, Email). Some service types will require additional configuration in the service itself. Here are some examples:
  1. Slack
     1. Go to `https://api.slack.com/apps` to create a new Slack App.
     1. Click `Create an App`, give the app a name, and click `Create App`.
     1. Click `Incoming Webhooks` and toggle on `Activate Incoming Webhooks`.
     1. Click `Add New Webhook to Workspace`.
     1. Pick the channel to which this Slack App will post.
     1. Back on the Grafana New Notification Channel page, copy the webhook URL to the `Url` field.
  1. PagerDuty
     1. Go to `https://app.pagerduty.com/developer/apps`.
     1. Click `Create New App`. Give the app a name and decription, and set the category to
        "Application Performance Management". For "Would you like to publish the app for all
        PagerDuty users?", select "No". Click `Save`.
     1. On the Configure App page, under Functionality > Events Integration, click `Add`.
     1. On the Event Integration page, under Events Integration Test > Create a Test Service, enter a name and click `Create`.
     1. Copy the Integration Key. Click `Save` and then `Save` again on the Configure App page.
     1. Back in the Grafana New Notification Channel page, paste the Integration Key into the `Integration Key` field.
1. After you have specified the settings on the Grafana New Notification Channel page, click `Send Test` to send a test notification. You should receive a notification from Grafana via your specified channel. If this worked, click `Save`.

> NOTE: Alerts have a link back to the relevant Grafana panel. In order for these links to work properly Grafana needs
> to know under which external URL it is running (note: this is usually different from the direct access URL you used
> earlier). Set the environment variable `GF_SERVER_ROOT_URL` to your Sourcegraph instance external URL followed
> by the path `/-/debug/grafana`.

### Set up an individual alert

After adding the appropriate notification channels, configure individual alerts to notify those channels.

1. Navigate to the dashboard with the panel and metric for which you'd like to configure an
   alert.
   1. Make sure the dashboard is not read-only (the default Sourcegraph-provided dashboards are
      read-only, because they are provisioned from disk). If the dashboard is read-only, go to the
      dashboard settings (the gear icon in the upper right) and click `Save As..` to create a
      writeable copy.
1. The panel title has a small dropdown next to it. Click the dropdown icon and select `Edit`.
1. In the left sidebar, choose the bell icon 🔔 for Alert.
1. Fill out the fields for the alert rule and select a notification channel.
1. Verify your rule by clicking `Test Rule` or viewing `State History`.
1. Return to the dashboard page by clicking the left arrow in the upper left. Save the dashboard by
   clicking the save icon in the upper right.

### Understanding alerts

See [alert solutions](alert_solutions.md) for possible solutions when alerts are firing.
