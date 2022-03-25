package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"strings"
	"text/template"
	"time"

	"github.com/sourcegraph/sourcegraph/dev/team"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var slackTemplate = `:truck: *{{.Environment}}* deployment (<{{.BuildURL}}|build>)

- Applications:
{{- range .Services }}
    - ` + "`" + `{{ . }}` + "`" + `
{{- end }} 

- Pull Requests:
{{- range .PullRequests }}
    - <{{ .WebURL }}|{{ .Name }}> {{ .AuthorSlackID }}
{{- end }}`

type slackSummaryPresenter struct {
	Environment  string
	BuildURL     string
	Services     []string
	PullRequests []pullRequestPresenter
}

type pullRequestPresenter struct {
	Name          string
	AuthorSlackID string
	WebURL        string
}

func slackSummary(ctx context.Context, teammates team.TeammateResolver, report *report) (string, error) {
	presenter := &slackSummaryPresenter{
		Environment: report.Environment,
		BuildURL:    report.BuildkiteBuildURL,
		Services:    report.Services,
	}

	for _, pr := range report.PullRequests {
		user := pr.GetUser()
		if user == nil {
			return "", errors.Newf("pull request %d has no user", pr.GetNumber())
		}
		teammate, err := teammates.ResolveByGitHubHandle(ctx, user.GetLogin())
		if err != nil {
			return "", err
		}
		presenter.PullRequests = append(presenter.PullRequests, pullRequestPresenter{
			Name:          pr.GetTitle(),
			WebURL:        pr.GetHTMLURL(),
			AuthorSlackID: fmt.Sprintf("<@%s>", teammate.SlackID),
		})
	}

	tmpl, err := template.New("deployment-status-slack-summary").Parse(slackTemplate)
	if err != nil {
		return "", err
	}
	var sb strings.Builder
	err = tmpl.Execute(&sb, presenter)
	if err != nil {
		return "", err
	}

	return sb.String(), nil
}

// postSlackUpdate attempts to send the given summary to at each of the provided webhooks.
func postSlackUpdate(webhook string, summary string) error {
	if webhook == "" {
		return nil
	}

	type slackText struct {
		Type string `json:"type"`
		Text string `json:"text"`
	}

	type slackBlock struct {
		Type string     `json:"type"`
		Text *slackText `json:"text,omitempty"`
	}

	// Generate request
	body, err := json.MarshalIndent(struct {
		Blocks []slackBlock `json:"blocks"`
	}{
		Blocks: []slackBlock{{
			Type: "section",
			Text: &slackText{
				Type: "mrkdwn",
				Text: summary,
			},
		}},
	}, "", "  ")
	if err != nil {
		return errors.Newf("MarshalIndent: %w", err)
	}
	log.Println("slackBody: ", string(body))

	req, err := http.NewRequest(http.MethodPost, webhook, bytes.NewBuffer(body))
	if err != nil {
		return err
	}
	req.Header.Add("Content-Type", "application/json")

	// Perform the HTTP Post on the webhook
	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}

	// Parse the response, to check if it succeeded
	buf := new(bytes.Buffer)
	_, err = buf.ReadFrom(resp.Body)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if buf.String() != "ok" {
		return err
	}
	return err
}
