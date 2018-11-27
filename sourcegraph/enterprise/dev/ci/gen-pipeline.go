// gen-pipeline.go generates a Buildkite YAML file that tests the entire
// Sourcegraph application and writes it to stdout.
package main

import (
	"fmt"
	"os"
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

	bk.OnEveryStepOpts = append(bk.OnEveryStepOpts,
		bk.Env("GO111MODULE", "on"),
		bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", "true"),
		bk.Env("FORCE_COLOR", "1"),
		bk.Env("ENTERPRISE", "1"),
	)

	pipeline.AddStep(":white_check_mark:",
		bk.Cmd("./dev/check/all.sh"))

	pipeline.AddStep(":lipstick:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn -s run prettier"))

	pipeline.AddStep(":typescript:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn -s run all:tslint"))

	pipeline.AddStep(":stylelint:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn -s run all:stylelint"),
		bk.Cmd("yarn run all:typecheck"))

	pipeline.AddStep(":graphql:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn run graphql-lint"))

	pipeline.AddStep(":typescript:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd client/browser"),
		bk.Cmd("yarn -s run browserslist"),
		bk.Cmd("yarn -s run build"),
		bk.Cmd("popd"))

	pipeline.AddStep(":webpack:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd web"),
		bk.Cmd("yarn -s run browserslist"),
		bk.Cmd("NODE_ENV=production yarn -s run build --color"),
		bk.Cmd("GITHUB_TOKEN= yarn -s run bundlesize"),
		bk.Cmd("popd"))

	pipeline.AddStep(":webpack: :moneybag:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd web"),
		bk.Cmd("yarn -s run browserslist"),
		bk.Cmd("ENTERPRISE=1 NODE_ENV=production yarn -s run build --color"),
		bk.Cmd("GITHUB_TOKEN= yarn -s run bundlesize"),
		bk.Cmd("popd"))

	pipeline.AddStep(":typescript:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd web"),
		bk.Cmd("yarn -s run cover"),
		bk.Cmd("yarn -s run nyc report -r json --report-dir coverage"),
		bk.Cmd("popd"),
		bk.ArtifactPaths("web/coverage/coverage-final.json"))

	pipeline.AddStep(":typescript:",
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("pushd shared"),
		bk.Cmd("yarn -s run cover"),
		bk.Cmd("yarn -s run nyc report -r json --report-dir coverage"),
		bk.Cmd("popd"),
		bk.ArtifactPaths("shared/coverage/coverage-final.json"))

	// TODO(sqs): reenable the DB backcompat test
	//
	// pipeline.AddStep(":postgres:",
	// 	bk.Cmd("./dev/ci/ci-db-backcompat.sh"))

	pipeline.AddStep(":go:",
		bk.Cmd("go generate ./..."),
		bk.Cmd("go test -coverprofile=coverage.txt -covermode=atomic -race ./..."),
		bk.ArtifactPaths("coverage.txt"))

	pipeline.AddStep(":go:",
		bk.Cmd("go generate ./..."),
		bk.Cmd("go install -tags dist ./cmd/... ./enterprise/cmd/..."),
	)

	pipeline.AddStep(":docker:",
		bk.Cmd("curl -sL -o hadolint \"https://github.com/hadolint/hadolint/releases/download/v1.6.5/hadolint-$(uname -s)-$(uname -m)\" && chmod 700 hadolint"),
		bk.Cmd("git ls-files | grep Dockerfile | xargs ./hadolint"))

	pipeline.AddWait()

	pipeline.AddStep(":codecov:",
		bk.Cmd("buildkite-agent artifact download 'coverage.txt' . || true"), // ignore error when no report exists
		bk.Cmd("buildkite-agent artifact download '*/coverage-final.json' . || true"),
		bk.Cmd("bash <(curl -s https://codecov.io/bash) -X gcov -X coveragepy -X xcode"))

	// addDockerImageStep adds a build step for a given app.
	// If the app is not in the cmd directory, it is assumed to be from the open source repo.
	addDockerImageStep := func(app string, insiders bool) {
		cmdDir := "cmd/" + app
		var pkgPath string
		if _, err := os.Stat(cmdDir); err != nil {
			fmt.Fprintf(os.Stderr, "github.com/sourcegraph/sourcegraph/enterprise/cmd/%s does not exist so building github.com/sourcegraph/sourcegraph/cmd/%s instead\n", app, app)
			pkgPath = "github.com/sourcegraph/sourcegraph/cmd/" + app
		} else {
			pkgPath = "github.com/sourcegraph/sourcegraph/enterprise/cmd/" + app
		}

		cmds := []bk.StepOpt{
			bk.Cmd("pushd enterprise"),
			bk.Cmd(fmt.Sprintf(`echo "Building %s..."`, app)),
		}

		preBuildScript := cmdDir + "/pre-build.sh"
		if _, err := os.Stat(preBuildScript); err == nil {
			cmds = append(cmds, bk.Cmd(preBuildScript))
		}

		image := "sourcegraph/" + app
		buildScript := cmdDir + "/build.sh"
		if _, err := os.Stat(buildScript); err == nil {
			cmds = append(cmds,
				bk.Env("IMAGE", image+":"+version),
				bk.Env("VERSION", version),
				bk.Cmd(buildScript),
			)
		} else {
			cmds = append(cmds,
				bk.Cmd("go build github.com/sourcegraph/godockerize"),
				bk.Cmd(fmt.Sprintf("./godockerize build -t %s:%s --go-build-flags='-ldflags' --go-build-flags='-X github.com/sourcegraph/sourcegraph/pkg/version.version=%s' --env VERSION=%s %s", image, version, version, version, pkgPath)),
			)
		}

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
		// // Run e2e tests
		// pipeline.AddStep(":chromium:",
		// 	bk.Env("FORCE_COLOR", "1"),
		// 	bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", ""),
		// 	bk.Env("DISPLAY", ":99"),
		// 	bk.Cmd("Xvfb :99 &"),
		// 	bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		// 	bk.Cmd("pushd client/browser"),
		// 	bk.Cmd("yarn -s run build"),
		// 	bk.Cmd("yarn -s run test:ci"),
		// 	bk.Cmd("yarn -s run test:e2e-ci --retries 5"),
		// 	bk.Cmd("popd"),
		// 	bk.ArtifactPaths("./puppeteer/*.png"),
		// )

		// pipeline.AddWait()

		// // Run e2e tests with extensions enabled
		// //
		// // TODO: Remove this step when extensions are enabled by default
		// pipeline.AddStep(":chromium:",
		// 	bk.Env("FORCE_COLOR", "1"),
		// 	bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", ""),
		// 	bk.Env("DISPLAY", ":99"),
		// 	bk.Cmd("Xvfb :99 &"),
		// 	bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		// 	bk.Cmd("pushd client/browser"),
		// 	bk.Cmd("USE_EXTENSIONS=true yarn -s run build"),
		// 	bk.Cmd("yarn -s run test:ci"),
		// 	bk.Cmd("yarn -s run test:e2e-ci --retries 5"),
		// 	bk.Cmd("popd"),
		// 	bk.ArtifactPaths("./puppeteer/*.png"),
		// )

		// pipeline.AddWait()

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

	if branch == "bext/release" {
		addBrowserExtensionReleaseSteps()
		return
	}

	pipeline.AddWait()

	fetchClusterCredentials := func(name, zone, project string) bk.StepOpt {
		return bk.Cmd(fmt.Sprintf("gcloud container clusters get-credentials %s --zone %s --project %s", name, zone, project))
	}

	addDeploySteps := func() {
		// Deploy to dogfood
		pipeline.AddStep(":dog:",
			// Protect against concurrent/out-of-order deploys
			bk.ConcurrencyGroup("deploy"),
			bk.Concurrency(1),
			bk.Env("VERSION", version),
			bk.Env("CONTEXT", "gke_sourcegraph-dev_us-central1-a_dogfood-cluster-7"),
			bk.Env("NAMESPACE", "default"),
			fetchClusterCredentials("dogfood-cluster-7", "us-central1-a", "sourcegraph-dev"),
			bk.Cmd("./dev/ci/deploy-dogfood.sh"))
		pipeline.AddWait()

		// Run e2e tests against dogfood
		pipeline.AddStep(":chromium:",
			// Protect against deploys while tests are running
			bk.ConcurrencyGroup("deploy"),
			bk.Concurrency(1),
			bk.Env("PUPPETEER_SKIP_CHROMIUM_DOWNLOAD", ""),
			bk.Cmd("yarn cache clean puppeteer"), // ensure it's downloaded even if the package was cached w/o downloading
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("pushd web"),
			bk.Cmd("yarn -s run test-e2e-sgdev --retries 5"),
			bk.Cmd("popd"),
			bk.ArtifactPaths("./puppeteer/*.png"))
		pipeline.AddWait()

		// Deploy to prod
		pipeline.AddStep(":rocket:",
			bk.Env("VERSION", version),
			bk.Cmd("./dev/ci/deploy-prod.sh"))
	}

	switch {
	case taggedRelease:
		allDockerImages := []string{
			"frontend",
			"github-proxy",
			"gitserver",
			"indexer",
			"lsp-proxy",
			"management-console",
			"query-runner",
			"repo-updater",
			"searcher",
			"server",
			"symbols",
			"xlang-go",
		}

		for _, dockerImage := range allDockerImages {
			addDockerImageStep(dockerImage, false)
		}
		pipeline.AddWait()

	case releaseBranch:
		addDockerImageStep("server", false)
		pipeline.AddWait()

	case branch == "master":
		addDockerImageStep("frontend", true)
		addDockerImageStep("server", true)
		pipeline.AddWait()
		addDeploySteps()

	case strings.HasPrefix(branch, "master-dry-run/"): // replicates `master` build but does not deploy
		addDockerImageStep("frontend", true)
		addDockerImageStep("server", true)
		pipeline.AddWait()

	case strings.HasPrefix(branch, "docker-images-patch/"):
		version = version + "_patch"
		addDockerImageStep(branch[20:], false)

	case strings.HasPrefix(branch, "docker-images/"):
		addDockerImageStep(branch[14:], true)
		pipeline.AddWait()
		// Only deploy images that aren't auto deployed from master.
		if branch != "docker-images/server" && branch != "docker-images/frontend" {
			addDeploySteps()
		}
	}
}
