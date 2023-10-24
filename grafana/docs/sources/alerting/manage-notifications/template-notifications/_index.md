---
canonical: https://grafana.com/docs/grafana/latest/alerting/manage-notifications/template-notifications/
description: How to customize your notifications using templating
keywords:
  - grafana
  - alerting
  - notifications
  - templates
labels:
  products:
    - cloud
    - enterprise
    - oss
title: Customize notifications
weight: 400
---

# Customize notifications

Customize your notifications with notifications templates.

You can use notification templates to change the title, message, and format of the message in your notifications.

Notification templates are not tied to specific contact point integrations, such as email or Slack. However, you can choose to create separate notification templates for different contact point integrations.

You can use notification templates to:

- Customize the subject of an email or the title of a message.
- Add, change or remove text in notifications. For example, to select or omit certain labels, annotations and links.
- Format text in bold and italic, and add or remove line breaks.

You cannot use notification templates to:

- Add HTML and CSS to email notifications to change their visual appearance.
- Change the design of notifications in instant messaging services such as Slack and Microsoft Teams. For example, to add or remove custom blocks with Slack Block Kit or adaptive cards with Microsoft Teams.
- Choose the number and size of images, or where in the notification images are shown.
- Customize the data in webhooks, including the fields or structure of the JSON data or send the data in other formats such as XML.
- Add or remove HTTP headers in webhooks other than those in the contact point configuration.

[Using Go's templating language][using-go-templating-language]

Learn how to write the content of your notification templates in Go’s templating language.

Create reusable notification templates for your contact points.

[Use notification templates][use-notification-templates]

Use notification templates to send notifications to your contact points.

[Reference][reference]

Data that is available when writing templates.

{{% docs/reference %}}
[reference]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/alerting/manage-notifications/template-notifications/reference"
[reference]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/alerting-and-irm/alerting/manage-notifications/template-notifications/reference"

[use-notification-templates]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/alerting/manage-notifications/template-notifications/use-notification-templates"
[use-notification-templates]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/alerting-and-irm/alerting/manage-notifications/template-notifications/use-notification-templates"

[using-go-templating-language]: "/docs/grafana/ -> /docs/grafana/<GRAFANA VERSION>/alerting/manage-notifications/template-notifications/using-go-templating-language"
[using-go-templating-language]: "/docs/grafana-cloud/ -> /docs/grafana-cloud/alerting-and-irm/alerting/manage-notifications/template-notifications/using-go-templating-language"
{{% /docs/reference %}}
