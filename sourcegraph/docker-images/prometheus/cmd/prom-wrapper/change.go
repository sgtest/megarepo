package main

import (
	"context"
	"fmt"
	"strconv"
	"time"

	"github.com/inconshreveable/log15"
	amconfig "github.com/prometheus/alertmanager/config"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/schema"
)

type ChangeContext struct {
	AMConfig *amconfig.Config // refer to https://prometheus.io/docs/alerting/latest/configuration/
}

// ChangeResult indicates output from a Change
type ChangeResult struct {
	Problems conf.Problems
}

// Change implements a change to configuration
type Change func(ctx context.Context, log log15.Logger, change ChangeContext, newConfig *subscribedSiteConfig) (result ChangeResult)

// changeReceivers applies `observability.alerts` as Alertmanager receivers.
func changeReceivers(ctx context.Context, log log15.Logger, change ChangeContext, newConfig *subscribedSiteConfig) (result ChangeResult) {
	// convenience function for creating a prefixed problem - this reflects the relevant site configuration fields
	newProblem := func(err error) {
		result.Problems = append(result.Problems, conf.NewSiteProblem(fmt.Sprintf("`observability.alerts`: %v", err)))
	}

	// generate new notifiers configuration
	change.AMConfig.Receivers = append(newReceivers(newConfig.Alerts, newProblem), &amconfig.Receiver{
		// stub receiver
		Name: alertmanagerNoopReceiver,
	})

	// make sure alerts are routed appropriately
	change.AMConfig.Route = &amconfig.Route{
		Receiver: alertmanagerNoopReceiver,
		// include `alertname` for now to accommodate non-generator alerts - in the long run, we want to remove grouping on `alertname`
		// because all alerts should have some predefined labels
		// https://github.com/sourcegraph/sourcegraph/issues/5370
		GroupByStr: []string{"alertname", "level", "service_name", "name"},

		// How long to initially wait to send a notification for a group - each group matches exactly one alert, so fire immediately
		GroupWait: duration(1 * time.Second),

		// How long to wait before sending a notification about new alerts that are added to a group of alerts - in this case,
		// equivalent to how long to wait until notifying about an alert re-firing
		GroupInterval:  duration(1 * time.Minute),
		RepeatInterval: duration(48 * time.Hour),

		// Route alerts to notifications
		Routes: []*amconfig.Route{
			{
				Receiver: alertmanagerWarningReceiver,
				Match: map[string]string{
					"level": "warning",
				},
			},
			{
				Receiver: alertmanagerCriticalReceiver,
				Match: map[string]string{
					"level": "critical",
				},
			},
		},
	}

	return result
}

// changeSMTP applies SMTP server configuration.
func changeSMTP(ctx context.Context, log log15.Logger, change ChangeContext, newConfig *subscribedSiteConfig) (result ChangeResult) {
	if change.AMConfig.Global == nil {
		change.AMConfig.Global = &amconfig.GlobalConfig{}
	}

	email := newConfig.Email
	change.AMConfig.Global.SMTPFrom = email.Address

	// assign zero-values to AMConfig SMTP fields if email.SMTP is nil
	if email.SMTP == nil {
		email.SMTP = &schema.SMTPServerConfig{}
	}
	change.AMConfig.Global.SMTPHello = email.SMTP.Domain
	change.AMConfig.Global.SMTPSmarthost = amconfig.HostPort{
		Host: email.SMTP.Host,
		Port: strconv.Itoa(email.SMTP.Port),
	}
	change.AMConfig.Global.SMTPAuthUsername = email.SMTP.Username
	switch email.SMTP.Authentication {
	case "PLAIN":
		change.AMConfig.Global.SMTPAuthPassword = amconfig.Secret(email.SMTP.Password)
	case "CRAM-MD5":
		change.AMConfig.Global.SMTPAuthSecret = amconfig.Secret(email.SMTP.Password)
	}
	change.AMConfig.Global.SMTPRequireTLS = !email.SMTP.DisableTLS

	return
}
