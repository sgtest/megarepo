package ci

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	bk "github.com/sourcegraph/sourcegraph/internal/buildkite"
)

var allDockerImages = []string{
	"frontend",
	"github-proxy",
	"gitserver",
	"query-runner",
	"replacer",
	"repo-updater",
	"searcher",
	"server",
	"symbols",
	"precise-code-intel/api-server",
	"precise-code-intel/bundle-manager",
	"precise-code-intel/worker",

	// Images under docker-images/
	"grafana",
	"indexed-searcher",
	"postgres-11.4",
	"prometheus",
	"redis-cache",
	"redis-store",
	"search-indexer",
	"syntax-highlighter",
}

// Verifies the docs formatting and builds the `docsite` command.
func addDocs(pipeline *bk.Pipeline) {
	pipeline.AddStep(":memo:",
		bk.Cmd("./dev/ci/yarn-run.sh prettier-check"),
		bk.Cmd("./dev/check/docsite.sh"))
}

// Adds the static check test step.
func addCheck(pipeline *bk.Pipeline) {
	pipeline.AddStep(":white_check_mark:", bk.Cmd("./dev/check/all.sh"))
}

// Adds the lint test step.
func addLint(pipeline *bk.Pipeline) {
	// If we run all lints together it is our slow step (5m). So we split it
	// into two and try balance the runtime. yarn is a fixed cost so we always
	// pay it on a step. Aim for around 3m.
	//
	// Random sample of timings:
	//
	// - yarn 41s
	// - eslint 137s
	// - build-ts 60s
	// - prettier 29s
	// - stylelint 7s
	// - graphql-lint 1s
	pipeline.AddStep(":eslint:",
		bk.Cmd("dev/ci/yarn-run.sh build-ts all:eslint")) // eslint depends on build-ts
	pipeline.AddStep(":lipstick: :lint-roller: :stylelint: :graphql:",
		bk.Cmd("dev/ci/yarn-run.sh prettier-check all:stylelint graphql-lint"))
}

// Adds steps for the OSS and Enterprise web app builds. Runs the web app tests.
func addWebApp(pipeline *bk.Pipeline) {
	// Webapp build
	pipeline.AddStep(":webpack::globe_with_meridians:",
		bk.Cmd("dev/ci/yarn-build.sh web"),
		bk.Env("NODE_ENV", "production"),
		bk.Env("ENTERPRISE", "0"))

	// Webapp enterprise build
	pipeline.AddStep(":webpack::globe_with_meridians::moneybag:",
		bk.Cmd("dev/ci/yarn-build.sh web"),
		bk.Env("NODE_ENV", "production"),
		bk.Env("ENTERPRISE", "1"))

	// Webapp tests
	pipeline.AddStep(":jest::globe_with_meridians:",
		bk.Cmd("dev/ci/yarn-test.sh web"),
		bk.ArtifactPaths("web/coverage/coverage-final.json"))
}

// Builds and tests the browser extension.
func addBrowserExt(pipeline *bk.Pipeline) {
	// Browser extension build
	pipeline.AddStep(":webpack::chrome:",
		bk.Cmd("dev/ci/yarn-build.sh browser"))

	// Browser extension tests
	pipeline.AddStep(":jest::chrome:",
		bk.Cmd("dev/ci/yarn-test.sh browser"),
		bk.ArtifactPaths("browser/coverage/coverage-final.json"))
}

// Tests the precise code intel system.
func addPreciseCodeIntelSystem(pipeline *bk.Pipeline) {
	pipeline.AddStep(":jest:",
		bk.Cmd("dev/ci/yarn-test-separate.sh cmd/precise-code-intel"),
		bk.ArtifactPaths("cmd/precise-code-intel/coverage/coverage-final.json"))
}

// Adds the shared frontend tests (shared between the web app and browser extension).
func addSharedTests(pipeline *bk.Pipeline) {
	// Shared tests
	pipeline.AddStep(":jest:",
		bk.Cmd("dev/ci/yarn-test.sh shared"),
		bk.ArtifactPaths("shared/coverage/coverage-final.json"))

	// Storybook
	pipeline.AddStep(":storybook:", bk.Cmd("dev/ci/yarn-run.sh storybook:smoke-test"))
}

// Adds PostgreSQL backcompat tests.
func addPostgresBackcompat(pipeline *bk.Pipeline) {
	pipeline.AddStep(":postgres:",
		bk.Cmd("./dev/ci/ci-db-backcompat.sh"))
}

// Adds the Go test step.
func addGoTests(pipeline *bk.Pipeline) {
	pipeline.AddStep(":go:",
		bk.Cmd("./dev/ci/go-test.sh"),
		bk.ArtifactPaths("coverage.txt"))
}

// Builds the OSS and Enterprise Go commands.
func addGoBuild(pipeline *bk.Pipeline) {
	pipeline.AddStep(":go:",
		bk.Cmd("./dev/ci/go-build.sh"),
	)
}

// Lints the Dockerfiles.
func addDockerfileLint(pipeline *bk.Pipeline) {
	pipeline.AddStep(":docker:",
		bk.Cmd("./dev/ci/docker-lint.sh"))
}

// Code coverage.
func addCodeCov(pipeline *bk.Pipeline) {
	pipeline.AddStep(":codecov:",
		bk.Cmd("buildkite-agent artifact download 'coverage.txt' . || true"), // ignore error when no report exists
		bk.Cmd("buildkite-agent artifact download '*/coverage-final.json' . || true"),
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -X gcov -X coveragepy -X xcode -F unit"))
}

// Release the browser extension.
func addBrowserExtensionReleaseSteps(pipeline *bk.Pipeline) {
	for _, browser := range []string{"chrome", "firefox"} {
		// Run e2e tests
		pipeline.AddStep(fmt.Sprintf(":%s:", browser),
			bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", ""),
			bk.Env("EXTENSION_PERMISSIONS_ALL_URLS", "true"),
			bk.Env("BROWSER", browser),
			bk.Env("SOURCEGRAPH_BASE_URL", "https://sourcegraph.com"),
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("pushd browser"),
			bk.Cmd("yarn -s run build"),
			bk.Cmd("yarn -s mocha ./src/e2e/github.test.ts"),
			bk.Cmd("popd"),
			bk.ArtifactPaths("./puppeteer/*.png"))
	}

	pipeline.AddWait()

	// Release to the Chrome Webstore
	pipeline.AddStep(":rocket::chrome:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd browser"),
		bk.Cmd("yarn -s run build"),
		bk.Cmd("yarn release:chrome"),
		bk.Cmd("popd"))

	// Build and self sign the FF extension and upload it to ...
	pipeline.AddStep(":rocket::firefox:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd browser"),
		bk.Cmd("yarn release:ff"),
		bk.Cmd("popd"))

	// Release to npm
	pipeline.AddStep(":rocket::npm:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd browser"),
		bk.Cmd("yarn -s run build"),
		bk.Cmd("yarn release:npm"),
		bk.Cmd("popd"))
}

// Adds a Buildkite pipeline "Wait".
func wait(pipeline *bk.Pipeline) {
	pipeline.AddWait()
}

func triggerE2E(c Config, commonEnv map[string]string) func(*bk.Pipeline) {
	// Run e2e tests for renovate and release branches
	// We do not run e2e tests on other branches until we can make them reliable.
	// See RFC 137: https://docs.google.com/document/d/14f7lwfToeT6t_vxnGsCuXqf3QcB5GRZ2Zoy6kYqBAIQ/edit
	runE2E := c.isRenovateBranch || c.releaseBranch || c.taggedRelease || c.isBextReleaseBranch || c.patch

	env := copyEnv(
		"BUILDKITE_PULL_REQUEST",
		"BUILDKITE_PULL_REQUEST_BASE_BRANCH",
		"BUILDKITE_PULL_REQUEST_REPO",
	)
	env["COMMIT_SHA"] = commonEnv["COMMIT_SHA"]
	env["DATE"] = commonEnv["DATE"]
	env["VERSION"] = commonEnv["VERSION"]
	env["CI_DEBUG_PROFILE"] = commonEnv["CI_DEBUG_PROFILE"]

	return func(pipeline *bk.Pipeline) {
		if !runE2E {
			return
		}
		pipeline.AddTrigger(":chromium:",
			bk.Trigger("sourcegraph-e2e"),
			bk.Build(bk.BuildOptions{
				Message: os.Getenv("BUILDKITE_MESSAGE"),
				Commit:  c.commit,
				Branch:  c.branch,
				Env:     env,
			}))
	}
}

func copyEnv(keys ...string) map[string]string {
	m := map[string]string{}
	for _, k := range keys {
		if v, ok := os.LookupEnv(k); ok {
			m[k] = v
		}
	}
	return m
}

// Build all relevant Docker images for Sourcegraph, given the current CI case (e.g., "tagged
// release", "release branch", "master branch", etc.)
func addDockerImages(c Config, final bool) func(*bk.Pipeline) {
	addDockerImage := func(c Config, app string, insiders bool) func(*bk.Pipeline) {
		if !final {
			return addCandidateDockerImage(c, app)
		}
		return addFinalDockerImage(c, app, insiders)
	}

	return func(pipeline *bk.Pipeline) {
		switch {
		case c.taggedRelease:
			for _, dockerImage := range allDockerImages {
				addDockerImage(c, dockerImage, false)(pipeline)
			}
			pipeline.AddWait()
		case c.releaseBranch:
			addDockerImage(c, "server", false)(pipeline)
			pipeline.AddWait()
		case c.isMasterDryRun: // replicates `master` build but does not deploy
			for _, dockerImage := range allDockerImages {
				addDockerImage(c, dockerImage, false)(pipeline)
			}
			pipeline.AddWait()
		case c.branch == "master":
			for _, dockerImage := range allDockerImages {
				addDockerImage(c, dockerImage, true)(pipeline)
			}
			pipeline.AddWait()

		case strings.HasPrefix(c.branch, "docker-images-patch/"):
			addDockerImage(c, c.branch[20:], false)(pipeline)
			pipeline.AddWait()
		}
	}
}

// Build a candidate docker image that will re-tagged with the final
// tags once the e2e tests pass.
func addCandidateDockerImage(c Config, app string) func(*bk.Pipeline) {
	return func(pipeline *bk.Pipeline) {

		baseImage := "sourcegraph/" + strings.ReplaceAll(app, "/", "-")

		cmds := []bk.StepOpt{
			bk.Cmd(fmt.Sprintf(`echo "Building candidate %s image..."`, app)),
			bk.Env("DOCKER_BUILDKIT", "1"),
			bk.Env("IMAGE", baseImage+":"+c.version),
			bk.Env("VERSION", c.version),
			bk.Cmd("yes | gcloud auth configure-docker"),
		}

		if _, err := os.Stat(filepath.Join("docker-images", app)); err == nil {
			// Building Docker image located under $REPO_ROOT/docker-images/
			cmds = append(cmds, bk.Cmd(filepath.Join("docker-images", app, "build.sh")))
		} else {
			// Building Docker images located under 4REPO_ROOT/cmd/
			cmdDir := func() string {
				if _, err := os.Stat(filepath.Join("enterprise/cmd", app)); err != nil {
					fmt.Fprintf(os.Stderr, "github.com/sourcegraph/sourcegraph/enterprise/cmd/%s does not exist so building github.com/sourcegraph/sourcegraph/cmd/%s instead\n", app, app)
					return "cmd/" + app
				}
				return "enterprise/cmd/" + app
			}()
			preBuildScript := cmdDir + "/pre-build.sh"
			if _, err := os.Stat(preBuildScript); err == nil {
				cmds = append(cmds, bk.Cmd(preBuildScript))
			}
			cmds = append(cmds, bk.Cmd(cmdDir+"/build.sh"))
		}

		gcrImage := fmt.Sprintf("us.gcr.io/sourcegraph-dev/%s", strings.TrimPrefix(baseImage, "sourcegraph/"))
		tag := candidateImageTag(c)
		cmds = append(cmds,
			bk.Cmd(fmt.Sprintf("docker tag %s:%s %s:%s", baseImage, c.version, gcrImage, tag)),
			bk.Cmd(fmt.Sprintf("docker push %s:%s", gcrImage, tag)),
		)

		pipeline.AddStep(":docker: :construction:", cmds...)
	}
}

// Tag and push final Docker image for the service defined by `app`
// after the e2e tests pass.
func addFinalDockerImage(c Config, app string, insiders bool) func(*bk.Pipeline) {
	return func(pipeline *bk.Pipeline) {
		baseImage := "sourcegraph/" + strings.ReplaceAll(app, "/", "-")

		cmds := []bk.StepOpt{
			bk.Cmd(fmt.Sprintf(`echo "Tagging final %s image..."`, app)),
			bk.Cmd("yes | gcloud auth configure-docker"),
		}

		gcrImage := fmt.Sprintf("us.gcr.io/sourcegraph-dev/%s", strings.TrimPrefix(baseImage, "sourcegraph/"))

		candidateImage := fmt.Sprintf("%s:%s", gcrImage, candidateImageTag(c))
		cmds = append(cmds,
			bk.Cmd(fmt.Sprintf("docker pull %s", candidateImage)),
			bk.Cmd(fmt.Sprintf("docker tag %s %s:%s", candidateImage, baseImage, c.version)),
		)

		dockerHubImage := fmt.Sprintf("index.docker.io/%s", baseImage)
		for _, image := range []string{dockerHubImage, gcrImage} {
			if app != "server" || c.taggedRelease || c.patch || c.patchNoTest {
				cmds = append(cmds,
					bk.Cmd(fmt.Sprintf("docker tag %s:%s %s:%s", baseImage, c.version, image, c.version)),
					bk.Cmd(fmt.Sprintf("docker push %s:%s", image, c.version)),
				)
			}

			if app == "server" && c.releaseBranch {
				cmds = append(cmds,
					bk.Cmd(fmt.Sprintf("docker tag %s:%s %s:%s-insiders", baseImage, c.version, image, c.branch)),
					bk.Cmd(fmt.Sprintf("docker push %s:%s-insiders", image, c.branch)),
				)
			}

			if insiders {
				cmds = append(cmds,
					bk.Cmd(fmt.Sprintf("docker tag %s:%s %s:insiders", baseImage, c.version, image)),
					bk.Cmd(fmt.Sprintf("docker push %s:insiders", image)),
				)
			}
		}

		pipeline.AddStep(":docker: :white_check_mark:", cmds...)
	}
}

func candidateImageTag(c Config) string {
	buildNumber := os.Getenv("BUILDKITE_BUILD_NUMBER")
	return fmt.Sprintf("%s_%s_candidate", c.commit, buildNumber)
}
