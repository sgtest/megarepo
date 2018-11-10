// gen-pipeline.go generates a Buildkite YAML file that tests the entire
// Sourcegraph application and writes it to stdout.
//
// The main reason to generate this file on demand is to ensure all Go packages
// are tested. Go packages are tested in separate containers - instead of all
// at once - so that Buildkite can schedule them to run across more machines
// on Sourcegraph's build farm.
//
// This script also generates a different file for deploy/tag builds vs. PR
// builds.
//
// See dev/ci/init-pipeline.yml for an example of where this script is invoked.
package main

import (
	"flag"
	"fmt"
	"os"

	bk "github.com/sourcegraph/sourcegraph/pkg/buildkite"
)

func init() {
	bk.Plugins["gopath-checkout#v1.0.1"] = map[string]string{
		"import": "github.com/sourcegraph/sourcegraph",
	}
}

func main() {
	flag.Usage = func() {
		fmt.Fprintf(os.Stderr,
			`Generate a Buildkite YAML file that tests the entire Sourcegraph application and write it to stdout.
`)
		flag.PrintDefaults()
	}
	flag.Parse()
	pipeline := &bk.Pipeline{}

	bk.OnEveryStepOpts = append(bk.OnEveryStepOpts,
		bk.Env("CYPRESS_INSTALL_BINARY", "0"),
		bk.Env("GO111MODULE", "on"))

	pipeline.AddStep(":white_check_mark:",
		bk.Cmd("./dev/check/all.sh"))

	pipeline.AddStep(":lipstick:",
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn run prettier"))

	pipeline.AddStep(":typescript:", // for speed
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"), // for speed
		bk.Env("FORCE_COLOR", "1"),
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace webapp run tslint"))

	pipeline.AddStep(":stylelint:",
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Env("FORCE_COLOR", "1"),
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace webapp run stylelint --quiet"))

	pipeline.AddStep(":graphql:",
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn run graphql-lint"))

	pipeline.AddStep(":webpack:",
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Env("FORCE_COLOR", "1"),
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace webapp run browserslist"),
		bk.Cmd("yarn workspace sourcegraph run build"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run build"),
		bk.Cmd("NODE_ENV=production yarn workspace webapp run build --color"),
		bk.Cmd("GITHUB_TOKEN= yarn workspace webapp run bundlesize"))

	pipeline.AddStep(":mocha:",
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Env("FORCE_COLOR", "1"),
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace webapp run cover"),
		bk.Cmd("yarn workspace webapp run nyc report -r json --report-dir coverage"),
		bk.ArtifactPaths("packages/webapp/coverage/coverage-final.json"))

	pipeline.AddStep(":docker:",
		bk.Cmd("curl -sL -o hadolint \"https://github.com/hadolint/hadolint/releases/download/v1.6.5/hadolint-$(uname -s)-$(uname -m)\" && chmod 700 hadolint"),
		bk.Cmd("git ls-files | grep Dockerfile | xargs ./hadolint"))

	pipeline.AddStep(":postgres:",
		bk.Cmd("./dev/ci/ci-db-backcompat.sh"))

	pipeline.AddStep(":go:",
		bk.Cmd("go test -coverprofile=coverage.txt -covermode=atomic -race ./..."),
		bk.ArtifactPaths("coverage.txt"))

	pipeline.AddStep(":typescript:",
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace sourcegraph run tslint"),
		bk.Cmd("yarn workspace sourcegraph run build"),
		bk.Cmd("yarn workspace sourcegraph run typecheck"),
		bk.Cmd("yarn workspace sourcegraph run cover"),
		bk.Cmd("yarn workspace sourcegraph run nyc report -r json --report-dir coverage"),
		bk.ArtifactPaths("packages/sourcegraph-extension-api/coverage/coverage-final.json"))

	pipeline.AddStep(":typescript:",
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace sourcegraph run build"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run tslint"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run build"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run typecheck"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run cover"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run nyc report -r json --report-dir coverage"),
		bk.ArtifactPaths("packages/extensions-client-common/coverage/coverage-final.json"))

	pipeline.AddStep(":typescript:",
		bk.Cmd("yarn --frozen-lockfile"),
		bk.Cmd("yarn workspace sourcegraph run build"),
		bk.Cmd("yarn workspace @sourcegraph/extensions-client-common run build"),
		bk.Cmd("yarn workspace browser-extensions run tslint"),
		bk.Cmd("yarn workspace browser-extensions run browserslist"),
		bk.Cmd("yarn workspace browser-extensions run build"),
		bk.Cmd("yarn workspace browser-extensions run test:ci"))

	pipeline.AddWait()

	pipeline.AddStep(":codecov:",
		bk.Cmd("buildkite-agent artifact download 'packages/webapp/coverage/coverage-final.json' . || true"), // ignore error when no report exists
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -f coverage-final.json"),
		bk.Cmd("buildkite-agent artifact download 'packages/sourcegraph-extension-api/coverage/coverage-final.json' . || true"),
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -f coverage-final.json"),
		bk.Cmd("buildkite-agent artifact download 'packages/extensions-client-common/coverage/coverage-final.json' . || true"),
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -f coverage-final.json"),
		bk.Cmd("buildkite-agent artifact download 'coverage.txt' coverage.txt || true"),
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -f coverage.txt"))

	pipeline.AddWait()

	_, err := pipeline.WriteTo(os.Stdout)
	if err != nil {
		panic(err)
	}
}
