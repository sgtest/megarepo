package ci

import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/Masterminds/semver"

	"github.com/sourcegraph/sourcegraph/dev/ci/runtype"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/images"
	bk "github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/buildkite"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/ci/changed"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/ci/operations"
)

var goAthensProxyURL = "http://athens-athens-proxy"

// CoreTestOperationsOptions should be used ONLY to adjust the behaviour of specific steps,
// e.g. by adding flags, and not as a condition for adding steps or commands.
type CoreTestOperationsOptions struct {
	// for clientChromaticTests
	ChromaticShouldAutoAccept bool
	MinimumUpgradeableVersion string
}

// CoreTestOperations is a core set of tests that should be run in most CI cases. More
// notably, this is what is used to define operations that run on PRs. Please read the
// following notes:
//
// - opts should be used ONLY to adjust the behaviour of specific steps, e.g. by adding
//   flags and not as a condition for adding steps or commands.
// - be careful not to add duplicate steps.
//
// If the conditions for the addition of an operation cannot be expressed using the above
// arguments, please add it to the switch case within `GeneratePipeline` instead.
func CoreTestOperations(diff changed.Diff, opts CoreTestOperationsOptions) *operations.Set {
	// Base set
	ops := operations.NewSet()

	// Simple, fast-ish linter checks
	linterOps := operations.NewNamedSet("Linters and static analysis",
		// lightweight check that works over a lot of stuff - we are okay with running
		// these on all PRs
		addPrettier,
		addCheck)
	if diff.Has(changed.GraphQL) {
		linterOps.Append(addGraphQLLint)
	}
	if diff.Has(changed.SVG) {
		linterOps.Append(addSVGLint)
	}
	if diff.Has(changed.Client) {
		linterOps.Append(addYarnDeduplicateLint)
	}
	if diff.Has(changed.Dockerfiles) {
		linterOps.Append(addDockerfileLint)
	}
	if diff.Has(changed.Terraform) {
		linterOps.Append(addTerraformLint)
	}
	if diff.Has(changed.Docs) {
		linterOps.Append(addDocs)
	}
	ops.Merge(linterOps)

	if diff.Has(changed.Client | changed.GraphQL) {
		// If there are any Graphql changes, they are impacting the client as well.
		ops.Merge(operations.NewNamedSet("Client checks",
			clientIntegrationTests,
			clientChromaticTests(opts.ChromaticShouldAutoAccept),
			frontendTests,   // ~4.5m
			addWebApp,       // ~3m
			addBrowserExt,   // ~2m
			addBrandedTests, // ~1.5m
			addTsLint))
	}

	if diff.Has(changed.Go | changed.GraphQL) {
		// If there are any Graphql changes, they are impacting the backend as well.
		ops.Merge(operations.NewNamedSet("Go checks",
			addGoTests,
			addGoBuild))
	}

	if diff.Has(changed.DatabaseSchema) {
		// If there are schema changes, ensure the tests of the last minor release continue
		// to succeed when the new version of the schema is applied. This ensures that the
		// schema can be rolled forward pre-upgrade without negatively affecting the running
		// instance (which was working fine prior to the upgrade).
		ops.Merge(operations.NewNamedSet("DB backcompat tests",
			addGoTestsBackcompat(opts.MinimumUpgradeableVersion)))
	}

	// CI script testing
	if diff.Has(changed.CIScripts) {
		ops.Merge(operations.NewNamedSet("CI script tests", addCIScriptsTests))
	}

	return ops
}

// Run enterprise/dev/ci/scripts tests
func addCIScriptsTests(pipeline *bk.Pipeline) {
	testDir := "./enterprise/dev/ci/scripts/tests"
	files, err := os.ReadDir(testDir)
	if err != nil {
		log.Fatalf("Failed to list CI scripts tests scripts: %s", err)
	}

	for _, f := range files {
		if filepath.Ext(f.Name()) == ".sh" {
			pipeline.AddStep(fmt.Sprintf(":bash: %s", f.Name()),
				bk.RawCmd(fmt.Sprintf("%s/%s", testDir, f.Name())))
		}
	}
}

// Verifies the docs formatting and builds the `docsite` command.
func addDocs(pipeline *bk.Pipeline) {
	pipeline.AddStep(":memo: Check and build docsite",
		bk.AnnotatedCmd("./dev/check/docsite.sh", bk.AnnotatedCmdOpts{}))
}

// Adds the terraform scanner step.  This executes very quickly ~6s
func addTerraformLint(pipeline *bk.Pipeline) {
	pipeline.AddStep(":lock: security - checkov",
		bk.Cmd("dev/ci/ci-checkov.sh"),
		bk.SoftFail(222))
}

// Adds the static check test step.
func addCheck(pipeline *bk.Pipeline) {
	pipeline.AddStep(":clipboard: Misc Linters",
		withYarnCache(),
		bk.AnnotatedCmd("./dev/check/all.sh", bk.AnnotatedCmdOpts{
			IncludeNames: true,
		}))
}

// yarn ~41s + ~30s
func addPrettier(pipeline *bk.Pipeline) {
	pipeline.AddStep(":lipstick: Prettier",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-run.sh prettier-check"))
}

// yarn ~41s + ~1s
func addGraphQLLint(pipeline *bk.Pipeline) {
	pipeline.AddStep(":lipstick: :graphql: GraphQL lint",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-run.sh graphql-lint"))
}

func addSVGLint(pipeline *bk.Pipeline) {
	pipeline.AddStep(":lipstick: :compression: SVG lint",
		bk.Cmd("dev/check/svgo.sh"))
}

func addYarnDeduplicateLint(pipeline *bk.Pipeline) {
	pipeline.AddStep(":lipstick: :yarn: Yarn deduplicate lint",
		bk.Cmd("dev/check/yarn-deduplicate.sh"))
}

// Adds Typescript linting. (2x ~41s) + ~60s + ~137s + 7s
func addTsLint(pipeline *bk.Pipeline) {
	// - yarn 41s (required on all steps)
	// - build-ts 60s
	// - eslint 137s
	// - stylelint 7s
	pipeline.AddStep(":eslint: Typescript eslint",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-run.sh build-ts all:eslint")) // eslint depends on build-ts
	pipeline.AddStep(":stylelint: Stylelint",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-run.sh all:stylelint"))
}

// Adds steps for the OSS and Enterprise web app builds. Runs the web app tests.
func addWebApp(pipeline *bk.Pipeline) {
	// Webapp build
	pipeline.AddStep(":webpack::globe_with_meridians: Build",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-build.sh client/web"),
		bk.Env("NODE_ENV", "production"),
		bk.Env("ENTERPRISE", ""))

	// Webapp enterprise build
	pipeline.AddStep(":webpack::globe_with_meridians::moneybag: Enterprise build",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-build.sh client/web"),
		bk.Env("NODE_ENV", "production"),
		bk.Env("ENTERPRISE", "1"),
		bk.Env("CHECK_BUNDLESIZE", "1"),
		// To ensure the Bundlesize output can be diffed to the baseline on main
		bk.Env("WEBPACK_USE_NAMED_CHUNKS", "true"))

	// Webapp tests
	pipeline.AddStep(":jest::globe_with_meridians: Test",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-test.sh client/web"),
		bk.Cmd("dev/ci/codecov.sh -c -F typescript -F unit"))
}

// Builds and tests the browser extension.
func addBrowserExt(pipeline *bk.Pipeline) {
	// Browser extension integration tests
	for _, browser := range []string{"chrome"} {
		pipeline.AddStep(
			fmt.Sprintf(":%s: Puppeteer tests for %s extension", browser, browser),
			withYarnCache(),
			bk.Env("EXTENSION_PERMISSIONS_ALL_URLS", "true"),
			bk.Env("BROWSER", browser),
			bk.Env("LOG_BROWSER_CONSOLE", "true"),
			bk.Env("SOURCEGRAPH_BASE_URL", "https://sourcegraph.com"),
			bk.Env("POLLYJS_MODE", "replay"), // ensure that we use existing recordings
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("yarn --cwd client/browser -s run build"),
			bk.Cmd("yarn run cover-browser-integration"),
			bk.Cmd("yarn nyc report -r json"),
			bk.Cmd("dev/ci/codecov.sh -c -F typescript -F integration"),
			bk.ArtifactPaths("./puppeteer/*.png"),
		)
	}

	// Browser extension unit tests
	pipeline.AddStep(":jest::chrome: Test browser extension",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-test.sh client/browser"),
		bk.Cmd("dev/ci/codecov.sh -c -F typescript -F unit"))
}

func clientIntegrationTests(pipeline *bk.Pipeline) {
	chunkSize := 2
	prepStepKey := "puppeteer:prep"
	// TODO check with Valery about this. Because we're running stateless agents,
	// this runs on a fresh instance and the hooks are not present at all, which
	// breaks the step.
	// skipGitCloneStep := bk.Plugin("uber-workflow/run-without-clone", "")

	// Build web application used for integration tests to share it between multiple parallel steps.
	pipeline.AddStep(":puppeteer::electric_plug: Puppeteer tests prep",
		withYarnCache(),
		bk.Key(prepStepKey),
		bk.Env("ENTERPRISE", "1"),
		bk.Env("COVERAGE_INSTRUMENT", "true"),
		bk.Cmd("dev/ci/yarn-build.sh client/web"),
		bk.Cmd("dev/ci/create-client-artifact.sh"))

	// Chunk web integration tests to save time via parallel execution.
	chunkedTestFiles := getChunkedWebIntegrationFileNames(chunkSize)
	// Percy finalize step should be executed after all integration tests.
	puppeteerFinalizeDependencies := make([]bk.StepOpt, len(chunkedTestFiles))

	// Add pipeline step for each chunk of web integrations files.
	for i, chunkTestFiles := range chunkedTestFiles {
		stepLabel := fmt.Sprintf(":puppeteer::electric_plug: Puppeteer tests chunk #%s", fmt.Sprint(i+1))

		stepKey := fmt.Sprintf("puppeteer:chunk:%s", fmt.Sprint(i+1))
		puppeteerFinalizeDependencies[i] = bk.DependsOn(stepKey)

		pipeline.AddStep(stepLabel,
			withYarnCache(),
			bk.Key(stepKey),
			bk.DependsOn(prepStepKey),
			bk.DisableManualRetry("The Percy build is finalized even if one of the concurrent agents fails. To retry correctly, restart the entire pipeline."),
			bk.Env("PERCY_ON", "true"),
			bk.Cmd(fmt.Sprintf(`dev/ci/yarn-web-integration.sh "%s"`, chunkTestFiles)),
			bk.ArtifactPaths("./puppeteer/*.png"))
	}

	finalizeSteps := []bk.StepOpt{
		// Allow to teardown the Percy build even if there was a failure in the earlier Percy steps.
		bk.AllowDependencyFailure(),
		// Percy service often fails for obscure reasons. The step is pretty fast, so we
		// just retry a few times.
		bk.AutomaticRetry(3),
		// Finalize just uses a remote package.
		// skipGitCloneStep,
		bk.Cmd("npx @percy/cli build:finalize"),
	}

	pipeline.AddStep(":puppeteer::electric_plug: Puppeteer tests finalize",
		append(finalizeSteps, puppeteerFinalizeDependencies...)...)
}

func clientChromaticTests(autoAcceptChanges bool) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		stepOpts := []bk.StepOpt{
			withYarnCache(),
			bk.AutomaticRetry(3),
			bk.Cmd("yarn --mutex network --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("yarn gulp generate"),
			bk.Env("MINIFY", "1"),
		}

		// Upload storybook to Chromatic
		chromaticCommand := "yarn chromatic --exit-zero-on-changes --exit-once-uploaded"
		if autoAcceptChanges {
			chromaticCommand += " --auto-accept-changes"
		} else {
			// Unless we plan on automatically accepting these changes, we only run this
			// step on ready-for-review pull requests.
			stepOpts = append(stepOpts, bk.IfReadyForReview())
		}

		pipeline.AddStep(":chromatic: Upload Storybook to Chromatic",
			append(stepOpts, bk.Cmd(chromaticCommand))...)
	}
}

// Adds the shared frontend tests (shared between the web app and browser extension).
func frontendTests(pipeline *bk.Pipeline) {
	// Shared tests
	pipeline.AddStep(":jest: Test shared client code",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-test.sh client/shared"),
		bk.Cmd("dev/ci/codecov.sh -c -F typescript -F unit"))

	// Wildcard tests
	pipeline.AddStep(":jest: Test wildcard client code",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-test.sh client/wildcard"),
		bk.Cmd("dev/ci/codecov.sh -c -F typescript -F unit"))
}

func addBrandedTests(pipeline *bk.Pipeline) {
	pipeline.AddStep(":jest: Test branded client code",
		withYarnCache(),
		bk.Cmd("dev/ci/yarn-test.sh client/branded"),
		bk.Cmd("dev/ci/codecov.sh -c -F typescript -F unit"))
}

// Adds the Go test step.
func addGoTests(pipeline *bk.Pipeline) {
	buildGoTests(func(description, testSuffix string) {
		pipeline.AddStep(
			fmt.Sprintf(":go: Test (%s)", description),
			bk.Env("GOPROXY", goAthensProxyURL),
			bk.Cmd("./dev/ci/go-test.sh "+testSuffix),
			bk.Cmd("./dev/ci/codecov.sh -c -F go"),
		)
	})
}

// Adds the Go backcompat test step.
func addGoTestsBackcompat(minimumUpgradeableVersion string) func(pipeline *bk.Pipeline) {
	return func(pipeline *bk.Pipeline) {
		buildGoTests(func(description, testSuffix string) {
			pipeline.AddStep(
				// TODO - set minimum upgradeable version
				fmt.Sprintf(":go::postgres: Backcompat test (%s)", description),
				bk.Env("MINIMUM_UPGRADEABLE_VERSION", minimumUpgradeableVersion),
				bk.Env("GOPROXY", goAthensProxyURL),
				bk.Cmd("./dev/ci/go-backcompat/test.sh "+testSuffix),
			)
		})
	}
}

// buildGoTests invokes the given function once for each subset of tests that should
// be run as part of complete coverage. The description will be the specific test path
// broken out to be run independently (or "all"), and the testSuffix will be the string
// to pass to go test to filter test packaes (e.g., "only <pkg>" or "exclude <pkgs...>").
func buildGoTests(f func(description, testSuffix string)) {
	// This is a bandage solution to speed up the go tests by running the slowest ones
	// concurrently. As a results, the PR time affecting only Go code is divided by two.
	slowGoTestPackages := []string{
		"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore",       // 224s
		"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore",     // 122s
		"github.com/sourcegraph/sourcegraph/enterprise/internal/insights",                       // 82+162s
		"github.com/sourcegraph/sourcegraph/internal/database",                                  // 253s
		"github.com/sourcegraph/sourcegraph/internal/repos",                                     // 106s
		"github.com/sourcegraph/sourcegraph/enterprise/internal/batches",                        // 52 + 60
		"github.com/sourcegraph/sourcegraph/cmd/frontend",                                       // 100s
		"github.com/sourcegraph/sourcegraph/enterprise/internal/database",                       // 94s
		"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/resolvers", // 152s
	}

	f("all", "exclude "+strings.Join(slowGoTestPackages, " "))

	for _, slowPkg := range slowGoTestPackages {
		f(strings.ReplaceAll(slowPkg, "github.com/sourcegraph/sourcegraph/", ""), "only "+slowPkg)
	}
}

// Builds the OSS and Enterprise Go commands.
func addGoBuild(pipeline *bk.Pipeline) {
	pipeline.AddStep(":go: Build",
		bk.Env("GOPROXY", goAthensProxyURL),
		bk.Cmd("./dev/ci/go-build.sh"),
	)
}

// Lints the Dockerfiles.
func addDockerfileLint(pipeline *bk.Pipeline) {
	pipeline.AddStep(":docker: Lint",
		bk.Cmd("./dev/ci/docker-lint.sh"))
}

// Adds backend integration tests step.
//
// Runtime: ~11m
func backendIntegrationTests(candidateImageTag string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":chains: Backend integration tests",
			// Run tests against the candidate server image
			bk.DependsOn(candidateImageStepKey("server")),
			bk.Env("IMAGE",
				images.DevRegistryImage("server", candidateImageTag)),
			bk.Cmd("dev/ci/integration/backend/run.sh"),
			bk.ArtifactPaths("./*.log"))
	}
}

func addBrowserExtensionE2ESteps(pipeline *bk.Pipeline) {
	for _, browser := range []string{"chrome"} {
		// Run e2e tests
		pipeline.AddStep(fmt.Sprintf(":%s: E2E for %s extension", browser, browser),
			withYarnCache(),
			bk.Env("EXTENSION_PERMISSIONS_ALL_URLS", "true"),
			bk.Env("BROWSER", browser),
			bk.Env("LOG_BROWSER_CONSOLE", "true"),
			bk.Env("SOURCEGRAPH_BASE_URL", "https://sourcegraph.com"),
			bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
			bk.Cmd("yarn --cwd client/browser -s run build"),
			bk.Cmd("yarn -s mocha ./client/browser/src/end-to-end/github.test.ts ./client/browser/src/end-to-end/gitlab.test.ts"),
			bk.ArtifactPaths("./puppeteer/*.png"))
	}
}

// Release the browser extension.
func addBrowserExtensionReleaseSteps(pipeline *bk.Pipeline) {
	addBrowserExtensionE2ESteps(pipeline)

	pipeline.AddWait()

	// Release to the Chrome Webstore
	pipeline.AddStep(":rocket::chrome: Extension release",
		withYarnCache(),
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn --cwd client/browser -s run build"),
		bk.Cmd("yarn --cwd client/browser release:chrome"))

	// Build and self sign the FF add-on and upload it to a storage bucket
	pipeline.AddStep(":rocket::firefox: Extension release",
		withYarnCache(),
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn --cwd client/browser release:firefox"))

	// Release to npm
	pipeline.AddStep(":rocket::npm: NPM Release",
		withYarnCache(),
		bk.Cmd("yarn --frozen-lockfile --network-timeout 60000"),
		bk.Cmd("yarn --cwd client/browser -s run build"),
		bk.Cmd("yarn --cwd client/browser release:npm"))
}

// Adds a Buildkite pipeline "Wait".
func wait(pipeline *bk.Pipeline) {
	pipeline.AddWait()
}

// Trigger the async pipeline to run. See pipeline.async.yaml.
func triggerAsync(buildOptions bk.BuildOptions) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		pipeline.AddTrigger(":snail: Trigger async",
			bk.Key("trigger:async"),
			bk.Trigger("sourcegraph-async"),
			bk.Async(true),
			bk.Build(buildOptions),
		)
	}
}

func triggerReleaseBranchHealthchecks(minimumUpgradeableVersion string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		version := semver.MustParse(minimumUpgradeableVersion)
		for _, branch := range []string{
			// Most recent major.minor
			fmt.Sprintf("%d.%d", version.Major(), version.Minor()),
			// The previous major.minor-1
			fmt.Sprintf("%d.%d", version.Major(), version.Minor()-1),
		} {
			pipeline.AddTrigger(fmt.Sprintf(":stethoscope: Trigger %s release branch healthcheck build", branch),
				bk.Trigger("sourcegraph"),
				bk.Async(false),
				bk.Build(bk.BuildOptions{
					Branch:  branch,
					Message: time.Now().Format(time.RFC1123) + " healthcheck build",
				}),
			)
		}
	}
}

func codeIntelQA(candidateTag string) operations.Operation {
	return func(p *bk.Pipeline) {
		p.AddStep(":docker::brain: Code Intel QA",
			// Run tests against the candidate server image
			bk.DependsOn(candidateImageStepKey("server")),
			bk.Env("CANDIDATE_VERSION", candidateTag),
			bk.Env("SOURCEGRAPH_BASE_URL", "http://127.0.0.1:7080"),
			bk.Env("SOURCEGRAPH_SUDO_USER", "admin"),
			bk.Env("TEST_USER_EMAIL", "test@sourcegraph.com"),
			bk.Env("TEST_USER_PASSWORD", "supersecurepassword"),
			bk.Cmd("dev/ci/integration/code-intel/run.sh"),
			bk.ArtifactPaths("./*.log"))
	}
}

func serverE2E(candidateTag string) operations.Operation {
	return func(p *bk.Pipeline) {
		p.AddStep(":chromium: Sourcegraph E2E",
			bk.Agent("queue", "baremetal"),
			// Run tests against the candidate server image
			bk.DependsOn(candidateImageStepKey("server")),
			bk.Env("CANDIDATE_VERSION", candidateTag),
			bk.Env("DISPLAY", ":99"),
			// TODO need doc
			bk.Env("JEST_CIRCUS", "0"),
			bk.Env("SOURCEGRAPH_BASE_URL", "http://127.0.0.1:7080"),
			bk.Env("SOURCEGRAPH_SUDO_USER", "admin"),
			bk.Env("TEST_USER_EMAIL", "test@sourcegraph.com"),
			bk.Env("TEST_USER_PASSWORD", "supersecurepassword"),
			bk.Env("INCLUDE_ADMIN_ONBOARDING", "false"),
			bk.Cmd("dev/ci/integration/e2e/run.sh"),
			bk.ArtifactPaths("./*.png", "./*.mp4", "./*.log"))
	}
}

func serverQA(candidateTag string) operations.Operation {
	return func(p *bk.Pipeline) {
		p.AddStep(":docker::chromium: Sourcegraph QA",
			bk.Agent("queue", "baremetal"),
			// Run tests against the candidate server image
			bk.DependsOn(candidateImageStepKey("server")),
			bk.Env("CANDIDATE_VERSION", candidateTag),
			bk.Env("DISPLAY", ":99"),
			// TODO need doc
			bk.Env("JEST_CIRCUS", "0"),
			bk.Env("LOG_STATUS_MESSAGES", "true"),
			bk.Env("NO_CLEANUP", "false"),
			bk.Env("SOURCEGRAPH_BASE_URL", "http://127.0.0.1:7080"),
			bk.Env("SOURCEGRAPH_SUDO_USER", "admin"),
			bk.Env("TEST_USER_EMAIL", "test@sourcegraph.com"),
			bk.Env("TEST_USER_PASSWORD", "supersecurepassword"),
			bk.Env("INCLUDE_ADMIN_ONBOARDING", "false"),
			bk.Cmd("dev/ci/integration/qa/run.sh"),
			bk.ArtifactPaths("./*.png", "./*.mp4", "./*.log"))
	}
}

func testUpgrade(candidateTag, minimumUpgradeableVersion string) operations.Operation {
	return func(p *bk.Pipeline) {
		p.AddStep(":docker::arrow_double_up: Sourcegraph Upgrade",
			bk.Agent("queue", "baremetal"),
			// Run tests against the candidate server image
			bk.DependsOn(candidateImageStepKey("server")),
			bk.Env("CANDIDATE_VERSION", candidateTag),
			bk.Env("MINIMUM_UPGRADEABLE_VERSION", minimumUpgradeableVersion),
			bk.Env("DISPLAY", ":99"),
			bk.Env("LOG_STATUS_MESSAGES", "true"),
			bk.Env("NO_CLEANUP", "false"),
			bk.Env("SOURCEGRAPH_BASE_URL", "http://127.0.0.1:7080"),
			bk.Env("SOURCEGRAPH_SUDO_USER", "admin"),
			bk.Env("TEST_USER_EMAIL", "test@sourcegraph.com"),
			bk.Env("TEST_USER_PASSWORD", "supersecurepassword"),
			bk.Env("INCLUDE_ADMIN_ONBOARDING", "false"),
			bk.Cmd("dev/ci/integration/upgrade/run.sh"),
			bk.ArtifactPaths("./*.png", "./*.mp4", "./*.log"))
	}
}

// Flaky deployment. See https://github.com/sourcegraph/sourcegraph/issues/25977
// func clusterQA(candidateTag string) operations.Operation {
// 	return func(p *bk.Pipeline) {
// 		p.AddStep(":k8s: Sourcegraph Cluster (deploy-sourcegraph) QA",
// 			bk.DependsOn(candidateImageStepKey("frontend")),
// 			bk.Env("CANDIDATE_VERSION", candidateTag),
// 			bk.Env("DOCKER_CLUSTER_IMAGES_TXT", strings.Join(images.DeploySourcegraphDockerImages, "\n")),
// 			bk.Env("NO_CLEANUP", "false"),
// 			bk.Env("SOURCEGRAPH_BASE_URL", "http://127.0.0.1:7080"),
// 			bk.Env("SOURCEGRAPH_SUDO_USER", "admin"),
// 			bk.Env("TEST_USER_EMAIL", "test@sourcegraph.com"),
// 			bk.Env("TEST_USER_PASSWORD", "supersecurepassword"),
// 			bk.Env("INCLUDE_ADMIN_ONBOARDING", "false"),
// 			bk.Cmd("./dev/ci/integration/cluster/run.sh"),
// 			bk.ArtifactPaths("./*.png", "./*.mp4", "./*.log"))
// 	}
// }

// candidateImageStepKey is the key for the given app (see the `images` package). Useful for
// adding dependencies on a step.
func candidateImageStepKey(app string) string {
	return strings.ReplaceAll(app, ".", "-") + ":candidate"
}

// Build a candidate docker image that will re-tagged with the final
// tags once the e2e tests pass.
//
// Version is the actual version of the code, and
func buildCandidateDockerImage(app, version, tag string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		image := strings.ReplaceAll(app, "/", "-")
		localImage := "sourcegraph/" + image + ":" + version

		cmds := []bk.StepOpt{
			bk.Key(candidateImageStepKey(app)),
			bk.Cmd(fmt.Sprintf(`echo "Building candidate %s image..."`, app)),
			bk.Env("DOCKER_BUILDKIT", "1"),
			bk.Env("IMAGE", localImage),
			bk.Env("VERSION", version),
		}

		if _, err := os.Stat(filepath.Join("docker-images", app)); err == nil {
			// Building Docker image located under $REPO_ROOT/docker-images/
			cmds = append(cmds, bk.Cmd(filepath.Join("docker-images", app, "build.sh")))
		} else {
			// Building Docker images located under $REPO_ROOT/cmd/
			cmdDir := func() string {
				// If /enterprise/cmd/... does not exist, build just /cmd/... instead.
				if _, err := os.Stat(filepath.Join("enterprise/cmd", app)); err != nil {
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

		devImage := images.DevRegistryImage(app, tag)
		cmds = append(cmds,
			// Retag the local image for dev registry
			bk.Cmd(fmt.Sprintf("docker tag %s %s", localImage, devImage)),
			// Publish tagged image
			bk.Cmd(fmt.Sprintf("docker push %s", devImage)),
		)

		pipeline.AddStep(fmt.Sprintf(":docker: :construction: %s", app), cmds...)
	}
}

// Ask trivy, a security scanning tool, to scan the candidate image
// specified by "app" and "tag".
func trivyScanCandidateImage(app, tag string) operations.Operation {
	image := images.DevRegistryImage(app, tag)

	// This is the special exit code that we tell trivy to use
	// if it finds a vulnerability. This is also used to soft-fail
	// this step.
	vulnerabilityExitCode := 27

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(fmt.Sprintf(":trivy: :docker: :mag: %s", app),
			bk.DependsOn(candidateImageStepKey(app)),

			bk.Cmd(fmt.Sprintf("docker pull %s", image)),

			// have trivy use a shorter name in its output
			bk.Cmd(fmt.Sprintf("docker tag %s %s", image, app)),

			bk.Env("IMAGE", app),
			bk.Env("VULNERABILITY_EXIT_CODE", fmt.Sprintf("%d", vulnerabilityExitCode)),
			bk.ArtifactPaths("./*-security-report.html"),
			bk.SoftFail(vulnerabilityExitCode),

			bk.AnnotatedCmd("./dev/ci/trivy/trivy-scan-high-critical.sh", bk.AnnotatedCmdOpts{
				Type:            bk.AnnotationTypeWarning,
				MultiJobContext: "docker-security-scans",
			}))
	}
}

// Tag and push final Docker image for the service defined by `app`
// after the e2e tests pass.
//
// It requires Config as an argument because published images require a lot of metadata.
func publishFinalDockerImage(c Config, app string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		devImage := images.DevRegistryImage(app, "")
		publishImage := images.PublishedRegistryImage(app, "")

		var images []string
		for _, image := range []string{publishImage, devImage} {
			if app != "server" || c.RunType.Is(runtype.TaggedRelease, runtype.ImagePatch, runtype.ImagePatchNoTest) {
				images = append(images, fmt.Sprintf("%s:%s", image, c.Version))
			}

			if app == "server" && c.RunType.Is(runtype.ReleaseBranch) {
				images = append(images, fmt.Sprintf("%s:%s-insiders", image, c.Branch))
			}

			if c.RunType.Is(runtype.MainBranch) {
				images = append(images, fmt.Sprintf("%s:insiders", image))
			}
		}

		// these tags are pushed to our dev registry, and are only
		// used internally
		for _, tag := range []string{
			c.Version,
			c.Commit,
			c.shortCommit(),
			fmt.Sprintf("%s_%s_%d", c.shortCommit(), c.Time.Format("2006-01-02"), c.BuildNumber),
			fmt.Sprintf("%s_%d", c.shortCommit(), c.BuildNumber),
			fmt.Sprintf("%s_%d", c.Commit, c.BuildNumber),
			strconv.Itoa(c.BuildNumber),
		} {
			internalImage := fmt.Sprintf("%s:%s", devImage, tag)
			images = append(images, internalImage)
		}

		candidateImage := fmt.Sprintf("%s:%s", devImage, c.candidateImageTag())
		cmd := fmt.Sprintf("./dev/ci/docker-publish.sh %s %s", candidateImage, strings.Join(images, " "))

		pipeline.AddStep(fmt.Sprintf(":docker: :truck: %s", app),
			// This step just pulls a prebuild image and pushes it to some registries. The
			// only possible failure here is a registry flake, so we retry a few times.
			bk.AutomaticRetry(3),
			bk.Cmd(cmd))
	}
}

// ~15m (building executor base VM)
func buildExecutor(version string, skipHashCompare bool) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		stepOpts := []bk.StepOpt{
			bk.Key(candidateImageStepKey("executor")),
			bk.Env("VERSION", version),
		}
		if !skipHashCompare {
			compareHashScript := "./enterprise/dev/ci/scripts/compare-hash.sh"
			stepOpts = append(stepOpts,
				// Soft-fail with code 222 if nothing has changed
				bk.SoftFail(222),
				bk.Cmd(fmt.Sprintf("%s ./enterprise/cmd/executor/hash.sh", compareHashScript)))
		}
		stepOpts = append(stepOpts,
			bk.Cmd("./enterprise/cmd/executor/build.sh"))

		pipeline.AddStep(":packer: :construction: executor image", stepOpts...)
	}
}

func publishExecutor(version string, skipHashCompare bool) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		candidateBuildStep := candidateImageStepKey("executor")
		stepOpts := []bk.StepOpt{
			bk.DependsOn(candidateBuildStep),
			bk.Env("VERSION", version),
		}
		if !skipHashCompare {
			// Publish iff not soft-failed on previous step
			checkDependencySoftFailScript := "./enterprise/dev/ci/scripts/check-dependency-soft-fail.sh"
			stepOpts = append(stepOpts,
				// Soft-fail with code 222 if nothing has changed
				bk.SoftFail(222),
				bk.Cmd(fmt.Sprintf("%s %s", checkDependencySoftFailScript, candidateBuildStep)))
		}
		stepOpts = append(stepOpts,
			bk.Cmd("./enterprise/cmd/executor/release.sh"))

		pipeline.AddStep(":packer: :white_check_mark: executor image", stepOpts...)
	}
}

// ~15m (building executor docker mirror base VM)
func buildExecutorDockerMirror(version string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		stepOpts := []bk.StepOpt{
			bk.Key(candidateImageStepKey("executor-docker-mirror")),
			bk.Env("VERSION", version),
		}
		stepOpts = append(stepOpts,
			bk.Cmd("./enterprise/cmd/executor/docker-mirror/build.sh"))

		pipeline.AddStep(":packer: :construction: docker registry mirror image", stepOpts...)
	}
}

func publishExecutorDockerMirror(version string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		candidateBuildStep := candidateImageStepKey("executor-docker-mirror")
		stepOpts := []bk.StepOpt{
			bk.DependsOn(candidateBuildStep),
			bk.Env("VERSION", version),
		}
		stepOpts = append(stepOpts,
			bk.Cmd("./enterprise/cmd/executor/docker-mirror/release.sh"))

		pipeline.AddStep(":packer: :white_check_mark: docker registry mirror image", stepOpts...)
	}
}

func uploadBuildeventTrace() operations.Operation {
	return func(p *bk.Pipeline) {
		p.AddStep(":arrow_heading_up: Upload build trace",
			bk.Cmd("./enterprise/dev/ci/scripts/upload-buildevent-report.sh"),
		)
	}
}
