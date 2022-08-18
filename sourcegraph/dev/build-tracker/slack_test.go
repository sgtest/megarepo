package main

import (
	"flag"
	"testing"

	"github.com/buildkite/go-buildkite/v3/buildkite"
	"github.com/hexops/autogold"
	"github.com/sourcegraph/log/logtest"
)

var RunIntegrationTest *bool = flag.Bool("RunIntegrationTest", false, "Run integrations tests")

func newJob(name string, exit int) *Job {
	return &Job{buildkite.Job{
		Name:       &name,
		ExitStatus: &exit,
	}}
}

func TestSlack(t *testing.T) {
	flag.Parse()
	if !*RunIntegrationTest {
		t.Skip("Integration test not enabled")
	}
	logger := logtest.NoOp(t)

	config, err := configFromEnv()
	if err != nil {
		t.Fatal(err)
	}

	client := NewNotificationClient(logger, config.SlackToken, config.GithubToken, DefaultChannel)

	num := 160000
	url := "http://www.google.com"
	commit := "78926a5b3b836a8a104a5d5adf891e5626b1e405"
	pipelineID := "sourcegraph"
	exit := 999
	msg := "this is a test"
	err = client.sendFailedBuild(
		&Build{
			Build: buildkite.Build{
				Message: &msg,
				WebURL:  &url,
				Creator: &buildkite.Creator{
					AvatarURL: "https://www.gravatar.com/avatar/7d4f6781b10e48a94d1052c443d13149",
				},
				Pipeline: &buildkite.Pipeline{
					ID:   &pipelineID,
					Name: &pipelineID,
				},
				Author: &buildkite.Author{
					Name:  "William Bezuidenhout",
					Email: "william.bezuidenhout@sourcegraph.com",
				},
				Number: &num,
				URL:    &url,
				Commit: &commit,
			},
			Pipeline: &Pipeline{buildkite.Pipeline{
				Name: &pipelineID,
			}},
			Jobs: map[string]Job{
				":one: fake step":   *newJob(":one: fake step", exit),
				":two: fake step":   *newJob(":two: fake step", exit),
				":three: fake step": *newJob(":three: fake step", exit),
				":four: fake step":  *newJob(":four: fake step", exit),
			},
		},
	)

	if err != nil {
		t.Fatalf("failed to send slack notification: %v", err)
	}
}

func TestGenerateHeader(t *testing.T) {
	for _, tc := range []struct {
		build *Build
		want  autogold.Value // use 'go test -update' to update
	}{
		{
			build: &Build{
				ConsecutiveFailure: 0,
			},
			want: autogold.Want("first failure", ":red_circle: Build 0 failed"),
		},
		{
			build: &Build{
				ConsecutiveFailure: 1,
			},
			want: autogold.Want("second failure", ":red_circle: Build 0 failed"),
		},
		{
			build: &Build{
				ConsecutiveFailure: 4,
			},
			want: autogold.Want("fifth failure", ":red_circle: Build 0 failed (:bangbang: 4th failure)"),
		},
	} {
		t.Run(tc.want.Name(), func(t *testing.T) {
			got := generateSlackHeader(tc.build)
			tc.want.Equal(t, got)
		})
	}
}
