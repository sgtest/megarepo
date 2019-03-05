// gen-pipeline.go generates a Buildkite YAML file that tests the entire
// Sourcegraph application and writes it to stdout.
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"

	bk "github.com/sourcegraph/sourcegraph/pkg/buildkite"
)

func init() {
	bk.Plugins["gopath-checkout#v1.0.1"] = map[string]string{
		"import": "github.com/sourcegraph/sourcegraph",
	}
}

func main() {
	pipeline := &bk.Pipeline{}

	defer func() {
		_, err := pipeline.WriteTo(os.Stdout)
		if err != nil {
			panic(err)
		}
	}()

	branch := os.Getenv("BUILDKITE_BRANCH")
	version := os.Getenv("BUILDKITE_TAG")
	taggedRelease := true // true if this is a semver tagged release
	if !strings.HasPrefix(version, "v") {
		taggedRelease = false
		commit := os.Getenv("BUILDKITE_COMMIT")
		if commit == "" {
			commit = "1234567890123456789012345678901234567890" // for testing
		}
		buildNum, _ := strconv.Atoi(os.Getenv("BUILDKITE_BUILD_NUMBER"))
		version = fmt.Sprintf("%05d_%s_%.7s", buildNum, time.Now().Format("2006-01-02"), commit)
	} else {
		// The Git tag "v1.2.3" should map to the Docker image "1.2.3" (without v prefix).
		version = strings.TrimPrefix(version, "v")
	}
	releaseBranch := regexp.MustCompile(`^[0-9]+\.[0-9]+$`).MatchString(branch)

	isBextReleaseBranch := branch == "bext/release"

	bk.OnEveryStepOpts = append(bk.OnEveryStepOpts,
		bk.Env("GO111MODULE", "on"),
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Env("FORCE_COLOR", "1"),
		bk.Env("ENTERPRISE", "1"),
	)

	if !isBextReleaseBranch {
		pipeline.AddStep(":white_check_mark:",
			bk.Cmd("./dev/check/all.sh"))
	}

	pipeline.AddStep(":lipstick: :lint-roller: :stylelint: :typescript: :graphql:",
		bk.Cmd("dev/ci/yarn-run.sh prettier-check all:tslint all:stylelint all:typecheck graphql-lint"))

	pipeline.AddStep(":ie:",
		bk.Cmd("dev/ci/yarn-build.sh client/browser"))

	if !isBextReleaseBranch {
		pipeline.AddStep(":webpack:",
			bk.Cmd("dev/ci/yarn-build.sh web"),
			bk.Env("NODE_ENV", "production"),
			bk.Env("ENTERPRISE", "0"))

		pipeline.AddStep(":webpack: :moneybag:",
			bk.Cmd("dev/ci/yarn-build.sh web"),
			bk.Env("NODE_ENV", "production"),
			bk.Env("ENTERPRISE", "1"))

		pipeline.AddStep(":typescript:",
			bk.Cmd("dev/ci/yarn-test.sh web"),
			bk.ArtifactPaths("web/coverage/coverage-final.json"))
	}

	pipeline.AddStep(":typescript:",
		bk.Cmd("dev/ci/yarn-test.sh shared"),
		bk.ArtifactPaths("shared/coverage/coverage-final.json"))

	if !isBextReleaseBranch {
		pipeline.AddStep(":postgres:",
			bk.Cmd("./dev/ci/ci-db-backcompat.sh"))

		pipeline.AddStep(":go:",
			bk.Cmd("./cmd/symbols/build.sh buildLibsqlite3Pcre"), // for symbols tests
			bk.Cmd("go test -coverprofile=coverage.txt -covermode=atomic -race ./..."),
			bk.ArtifactPaths("coverage.txt"))

		pipeline.AddStep(":go:",
			bk.Cmd("go generate ./..."),
			bk.Cmd("go install -tags dist ./cmd/... ./enterprise/cmd/..."),
		)

		pipeline.AddStep(":docker:",
			bk.Cmd("curl -sL -o hadolint \"https://github.com/hadolint/hadolint/releases/download/v1.15.0/hadolint-$(uname -s)-$(uname -m)\" && chmod 700 hadolint"),
			bk.Cmd("git ls-files | grep Dockerfile | xargs ./hadolint"))

		pipeline.AddStep(":chromium:",
			bk.Cmd(fmt.Sprintf(`echo "Building server..."`)),
			bk.Cmd("pushd enterprise"),
			bk.Cmd("./cmd/server/pre-build.sh"),
			bk.Env("IMAGE", "sourcegraph/server:"+version+"_candidate"),
			bk.Env("VERSION", version),
			bk.Cmd("./cmd/server/build.sh"),
			bk.Cmd("popd"),
			bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", ""),
			bk.Cmd("./dev/ci/e2e.sh"),
			bk.ArtifactPaths("./puppeteer/*.png"))
	}

	pipeline.AddWait()

	pipeline.AddStep(":codecov:",
		bk.Cmd("buildkite-agent artifact download 'coverage.txt' . || true"), // ignore error when no report exists
		bk.Cmd("buildkite-agent artifact download '*/coverage-final.json' . || true"),
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -X gcov -X coveragepy -X xcode"))

	// addDockerImageStep adds a build step for a given app.
	// If the app is not in the cmd directory, it is assumed to be from the open source repo.
	addDockerImageStep := func(app string, insiders bool) {
		cmds := []bk.StepOpt{
			bk.Cmd(fmt.Sprintf(`echo "Building %s..."`, app)),
		}

		cmdDir := "cmd/" + app
		if _, err := os.Stat(filepath.Join("enterprise", cmdDir)); err != nil {
			fmt.Fprintf(os.Stderr, "github.com/sourcegraph/sourcegraph/enterprise/cmd/%s does not exist so building github.com/sourcegraph/sourcegraph/cmd/%s instead\n", app, app)
		} else {
			cmds = append(cmds, bk.Cmd("pushd enterprise"))
		}

		preBuildScript := cmdDir + "/pre-build.sh"
		if _, err := os.Stat(preBuildScript); err == nil {
			cmds = append(cmds, bk.Cmd(preBuildScript))
		}

		image := "sourcegraph/" + app

		getBuildScript := func() string {
			buildScriptByApp := map[string]string{
				"symbols": "env BUILD_TYPE=dist ./cmd/symbols/build.sh buildSymbolsDockerImage",
				// The server image was built prior to e2e tests in a previous step.
				"server": fmt.Sprintf("docker tag %s:%s_candidate %s:%s", image, version, image, version),
			}
			if buildScript, ok := buildScriptByApp[app]; ok {
				return buildScript
			}
			return cmdDir + "/build.sh"
		}

		cmds = append(cmds,
			bk.Env("IMAGE", image+":"+version),
			bk.Env("VERSION", version),
			bk.Cmd(getBuildScript()),
		)

		if app != "server" || taggedRelease {
			cmds = append(cmds,
				bk.Cmd(fmt.Sprintf("docker push %s:%s", image, version)),
			)
		}

		if app == "server" && releaseBranch {
			cmds = append(cmds,
				bk.Cmd(fmt.Sprintf("docker tag %s:%s %s:%s-insiders", image, version, image, branch)),
				bk.Cmd(fmt.Sprintf("docker push %s:%s-insiders", image, branch)),
			)
		}

		if insiders {
			cmds = append(cmds,
				bk.Cmd(fmt.Sprintf("docker tag %s:%s %s:insiders", image, version, image)),
				bk.Cmd(fmt.Sprintf("docker push %s:insiders", image)),
			)
		}
		pipeline.AddStep(":docker:", cmds...)
	}

	if strings.HasPrefix(branch, "docker-images-patch-notest/") {
		version = version + "_patch"
		addDockerImageStep(branch[27:], false)
		return
	}

	addBrowserExtensionReleaseSteps := func() {
		// Run e2e tests
		pipeline.AddStep(":chromium:",
			bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", ""),
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("pushd client/browser"),
			bk.Cmd("yarn -s run build"),
			bk.Cmd("yarn -s run test-e2e"),
			bk.Cmd("popd"),
			bk.ArtifactPaths("./puppeteer/*.png"))

		pipeline.AddWait()

		// Release to the Chrome Webstore
		pipeline.AddStep(":chrome:",
			bk.Env("FORCE_COLOR", "1"),
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("pushd client/browser"),
			bk.Cmd("yarn -s run build"),
			bk.Cmd("yarn release:chrome"),
			bk.Cmd("popd"))

		// Build and self sign the FF extension and upload it to ...
		pipeline.AddStep(":firefox:",
			bk.Env("FORCE_COLOR", "1"),
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("pushd client/browser"),
			bk.Cmd("yarn release:ff"),
			bk.Cmd("popd"))
	}

	if isBextReleaseBranch {
		addBrowserExtensionReleaseSteps()
		return
	}

	pipeline.AddWait()

	allDockerImages := []string{
		"frontend",
		"github-proxy",
		"gitserver",
		"management-console",
		"query-runner",
		"repo-updater",
		"searcher",
		"server",
		"symbols",
	}

	switch {
	case taggedRelease:
		for _, dockerImage := range allDockerImages {
			addDockerImageStep(dockerImage, false)
		}
		pipeline.AddWait()

	case releaseBranch:
		addDockerImageStep("server", false)
		pipeline.AddWait()

	case branch == "master":
		for _, dockerImage := range allDockerImages {
			addDockerImageStep(dockerImage, true)
		}
		pipeline.AddWait()

	case strings.HasPrefix(branch, "master-dry-run/"): // replicates `master` build but does not deploy
		for _, dockerImage := range allDockerImages {
			addDockerImageStep(dockerImage, true)
		}
		pipeline.AddWait()

	case strings.HasPrefix(branch, "docker-images-patch/"):
		version = version + "_patch"
		addDockerImageStep(branch[20:], false)
		pipeline.AddWait()
	}
}
