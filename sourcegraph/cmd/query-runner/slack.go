package main

import (
	"context"
	"fmt"

	log15 "gopkg.in/inconshreveable/log15.v2"

	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/slack"
)

func (n *notifier) slackNotify(ctx context.Context) {
	plural := ""
	if n.results.Data.Search.Results.ApproximateResultCount != "1" {
		plural = "s"
	}

	text := fmt.Sprintf(`*%s* new result%s found for saved search <%s|"%s">`,
		n.results.Data.Search.Results.ApproximateResultCount,
		plural,
		searchURL(n.newQuery, utmSourceSlack),
		n.query.Description,
	)
	for _, recipient := range n.recipients {
		if err := slackNotify(ctx, recipient, text); err != nil {
			log15.Error("Failed to post Slack notification message.", "recipient", recipient, "text", text, "error", err)
		}
	}
	logEvent("", "SavedSearchSlackNotificationSent", "results")
}

func slackNotifySubscribed(ctx context.Context, recipient *recipient, query api.SavedQuerySpecAndConfig) error {
	text := fmt.Sprintf(`Slack notifications enabled for the saved search <%s|"%s">. Notifications will be sent here when new results are available.`,
		searchURL(query.Config.Query, utmSourceSlack),
		query.Config.Description,
	)
	if err := slackNotify(ctx, recipient, text); err != nil {
		return err
	}
	logEvent("", "SavedSearchSlackNotificationSent", "enabled")
	return nil
}

func slackNotifyUnsubscribed(ctx context.Context, recipient *recipient, query api.SavedQuerySpecAndConfig) error {
	text := fmt.Sprintf(`Slack notifications for the saved search <%s|"%s"> disabled.`,
		searchURL(query.Config.Query, utmSourceSlack),
		query.Config.Description,
	)
	if err := slackNotify(ctx, recipient, text); err != nil {
		return err
	}
	logEvent("", "SavedSearchSlackNotificationSent", "disabled")
	return nil
}

func slackNotify(ctx context.Context, recipient *recipient, text string) error {
	if !recipient.slack {
		return nil
	}

	settings, _, err := api.InternalClient.SettingsGetForSubject(ctx, recipient.subject())
	if err != nil {
		return err
	}
	if settings.NotificationsSlack == nil || settings.NotificationsSlack.WebhookURL == "" {
		return fmt.Errorf("unable to send Slack notification because recipient (%s) has no Slack webhook URL configured", recipient.spec)
	}

	payload := &slack.Payload{
		Username:    "saved-search-bot",
		IconEmoji:   ":mag:",
		UnfurlLinks: false,
		UnfurlMedia: false,
		Text:        text,
	}
	client := slack.New(settings.NotificationsSlack.WebhookURL, true)
	return slack.Post(payload, client.WebhookURL)
}
