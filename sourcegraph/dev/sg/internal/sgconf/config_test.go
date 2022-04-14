package sgconf

import (
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/run"
)

func TestParseConfig(t *testing.T) {
	input := `
env:
  SRC_REPOS_DIR: $HOME/.sourcegraph/repos

commands:
  frontend:
    cmd: ulimit -n 10000 && .bin/frontend
    install: go build -o .bin/frontend github.com/sourcegraph/sourcegraph/cmd/frontend
    checkBinary: .bin/frontend
    env:
      CONFIGURATION_MODE: server
    watch:
      - lib

checks:
  docker:
    cmd: docker version
    failMessage: "Failed to run 'docker version'. Please make sure Docker is running."

commandsets:
  oss:
    - frontend
    - gitserver
  enterprise:
    checks:
      - docker
    commands:
      - frontend
      - gitserver
`

	have, err := parseConfig([]byte(input))
	if err != nil {
		t.Errorf("unexpected error: %s", err)
	}

	want := &Config{
		Env: map[string]string{"SRC_REPOS_DIR": "$HOME/.sourcegraph/repos"},
		Commands: map[string]run.Command{
			"frontend": {
				Name:        "frontend",
				Cmd:         "ulimit -n 10000 && .bin/frontend",
				Install:     "go build -o .bin/frontend github.com/sourcegraph/sourcegraph/cmd/frontend",
				CheckBinary: ".bin/frontend",
				Env:         map[string]string{"CONFIGURATION_MODE": "server"},
				Watch:       []string{"lib"},
			},
		},
		Commandsets: map[string]*Commandset{
			"oss": {
				Name:     "oss",
				Commands: []string{"frontend", "gitserver"},
			},
			"enterprise": {
				Name:     "enterprise",
				Commands: []string{"frontend", "gitserver"},
				Checks:   []string{"docker"},
			},
		},
	}

	if diff := cmp.Diff(want, have); diff != "" {
		t.Fatalf("wrong config. (-want +got):\n%s", diff)
	}
}

func TestParseAndMerge(t *testing.T) {
	a := `
commands:
  enterprise-frontend:
    cmd: .bin/enterprise-frontend
    install: go build .bin/enterprise-frontend github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend
    checkBinary: .bin/enterprise-frontend
    env:
      ENTERPRISE: 1
      EXTSVC_CONFIG_FILE: '../dev-private/enterprise/dev/external-services-config.json'
    watch:
      - lib
      - internal
      - cmd/frontend
      - enterprise/internal
      - enterprise/cmd/frontend
`
	config, err := parseConfig([]byte(a))
	if err != nil {
		t.Errorf("unexpected error: %s", err)
	}

	b := `
commands:
  enterprise-frontend:
    env:
      EXTSVC_CONFIG_FILE: ''
`

	overwrite, err := parseConfig([]byte(b))
	if err != nil {
		t.Errorf("unexpected error: %s", err)
	}

	config.Merge(overwrite)

	cmd, ok := config.Commands["enterprise-frontend"]
	if !ok {
		t.Fatalf("command not found")
	}

	want := run.Command{
		Name:        "enterprise-frontend",
		Cmd:         ".bin/enterprise-frontend",
		Install:     "go build .bin/enterprise-frontend github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend",
		CheckBinary: ".bin/enterprise-frontend",
		Env:         map[string]string{"ENTERPRISE": "1", "EXTSVC_CONFIG_FILE": ""},
		Watch: []string{
			"lib",
			"internal",
			"cmd/frontend",
			"enterprise/internal",
			"enterprise/cmd/frontend",
		},
	}

	if diff := cmp.Diff(cmd, want); diff != "" {
		t.Fatalf("wrong cmd. (-want +got):\n%s", diff)
	}
}
