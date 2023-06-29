package ci

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/sourcegraph/sourcegraph/dev/ci/runtype"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/images"
	bk "github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/buildkite"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/ci/operations"
)

func BazelOperations() []operations.Operation {
	ops := []operations.Operation{}
	ops = append(ops, bazelConfigure())
	ops = append(ops, bazelTest("//...", "//client/web:test"))
	ops = append(ops, bazelBackCompatTest(
		"@sourcegraph_back_compat//cmd/...",
		"@sourcegraph_back_compat//lib/...",
		"@sourcegraph_back_compat//internal/...",
		"@sourcegraph_back_compat//enterprise/cmd/...",
		"@sourcegraph_back_compat//enterprise/internal/...",
		"-@sourcegraph_back_compat//cmd/migrator/...",
		"-@sourcegraph_back_compat//enterprise/cmd/migrator/...",
	))
	return ops
}

func bazelCmd(args ...string) string {
	pre := []string{
		"bazel",
		"--bazelrc=.bazelrc",
		"--bazelrc=.aspect/bazelrc/ci.bazelrc",
		"--bazelrc=.aspect/bazelrc/ci.sourcegraph.bazelrc",
	}
	Cmd := append(pre, args...)
	return strings.Join(Cmd, " ")
}

func bazelPushImagesCandidates(version string) func(*bk.Pipeline) {
	return bazelPushImagesCmd(version, true)
}

func bazelPushImagesFinal(version string) func(*bk.Pipeline) {
	return bazelPushImagesCmd(version, false)
}

func bazelPushImagesCmd(version string, isCandidate bool) func(*bk.Pipeline) {
	stepName := ":bazel::docker: Push final images"
	stepKey := "bazel-push-images"
	candidate := ""

	if isCandidate {
		stepName = ":bazel::docker: Push candidate Images"
		stepKey = stepKey + "-candidate"
		candidate = "true"
	}

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(stepName,
			bk.Agent("queue", "bazel"),
			bk.DependsOn("bazel-tests"),
			bk.Key(stepKey),
			bk.Env("PUSH_VERSION", version),
			bk.Env("CANDIDATE_ONLY", candidate),
			bk.Cmd(bazelStampedCmd(`build $$(bazel query 'kind("oci_push rule", //...)')`)),
			bk.Cmd("./enterprise/dev/ci/push_all.sh"),
		)
	}
}

func bazelStampedCmd(args ...string) string {
	pre := []string{
		"bazel",
		"--bazelrc=.bazelrc",
		"--bazelrc=.aspect/bazelrc/ci.bazelrc",
		"--bazelrc=.aspect/bazelrc/ci.sourcegraph.bazelrc",
	}
	post := []string{
		"--stamp",
		"--workspace_status_command=./dev/bazel_stamp_vars.sh",
	}

	cmd := append(pre, args...)
	cmd = append(cmd, post...)
	return strings.Join(cmd, " ")
}

// bazelAnalysisPhase only runs the analasys phase, ensure that the buildfiles
// are correct, but do not actually build anything.
func bazelAnalysisPhase() func(*bk.Pipeline) {
	// We run :gazelle since 'configure' causes issues on CI, where it doesn't have the go path available
	cmd := bazelCmd(
		"build",
		"--nobuild", // this is the key flag to enable this.
		"//...",
	)

	cmds := []bk.StepOpt{
		bk.Key("bazel-analysis"),
		bk.Agent("queue", "bazel"),
		bk.Cmd(cmd),
	}

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: Analysis phase",
			cmds...,
		)
	}
}
func bazelConfigure() func(*bk.Pipeline) {
	// We run :gazelle since 'configure' causes issues on CI, where it doesn't have the go path available
	cmds := []bk.StepOpt{
		bk.Key("bazel-configure"),
		bk.Agent("queue", "bazel"),
		bk.AnnotatedCmd("dev/ci/bazel-configure.sh", bk.AnnotatedCmdOpts{
			Annotations: &bk.AnnotationOpts{
				Type:         bk.AnnotationTypeWarning,
				IncludeNames: false,
			},
		}),
	}

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: Ensure buildfiles are up to date",
			cmds...,
		)
	}
}

func bazelAnnouncef(format string, args ...any) bk.StepOpt {
	msg := fmt.Sprintf(format, args...)
	return bk.Cmd(fmt.Sprintf(`echo "--- :bazel: %s"`, msg))
}

func bazelTest(targets ...string) func(*bk.Pipeline) {
	cmds := []bk.StepOpt{
		bk.DependsOn("bazel-configure"),
		bk.Agent("queue", "bazel"),
		bk.Key("bazel-tests"),
		bk.ArtifactPaths("./bazel-testlogs/enterprise/cmd/embeddings/shared/shared_test/*.log", "./command.profile.gz"),
		bk.AutomaticRetry(1), // TODO @jhchabran flaky stuff are breaking builds
	}

	// Test commands
	bazelTestCmds := []bk.StepOpt{}
	for _, target := range targets {
		cmd := bazelCmd(fmt.Sprintf("test %s", target))
		bazelTestCmds = append(bazelTestCmds,
			bazelAnnouncef("bazel test %s", target),
			bk.Cmd(cmd))
	}
	cmds = append(cmds, bazelTestCmds...)

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: Tests",
			cmds...,
		)
	}
}

func bazelBackCompatTest(targets ...string) func(*bk.Pipeline) {
	cmds := []bk.StepOpt{
		bk.DependsOn("bazel-configure"),
		bk.Agent("queue", "bazel"),

		// Generate a patch that backports the migration from the new code into the old one.
		// Ignore space is because of https://github.com/bazelbuild/bazel/issues/17376
		bk.Cmd("git diff --ignore-space-at-eol v5.1.0..HEAD -- migrations/ > dev/backcompat/patches/back_compat_migrations.patch"),
	}

	bazelCmd := bazelCmd(fmt.Sprintf("test --test_tag_filters=go -- %s ", strings.Join(targets, " ")))
	cmds = append(
		cmds,
		bk.Cmd(bazelCmd),
	)

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: BackCompat Tests",
			cmds...,
		)
	}
}

func bazelTestWithDepends(optional bool, dependsOn string, targets ...string) func(*bk.Pipeline) {
	cmds := []bk.StepOpt{
		bk.Agent("queue", "bazel"),
	}

	bazelCmd := bazelCmd(fmt.Sprintf("test %s", strings.Join(targets, " ")))
	cmds = append(cmds, bk.Cmd(bazelCmd))
	cmds = append(cmds, bk.DependsOn(dependsOn))

	return func(pipeline *bk.Pipeline) {
		if optional {
			cmds = append(cmds, bk.SoftFail())
		}
		pipeline.AddStep(":bazel: Tests",
			cmds...,
		)
	}
}

func bazelBuild(targets ...string) func(*bk.Pipeline) {
	cmds := []bk.StepOpt{
		bk.Key("bazel_build"),
		bk.Agent("queue", "bazel"),
	}
	cmd := bazelStampedCmd(fmt.Sprintf("build %s", strings.Join(targets, " ")))
	cmds = append(
		cmds,
		bk.Cmd(cmd),
		bk.Cmd(bazelStampedCmd("run //enterprise/cmd/server:candidate_push")),
	)

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: Build ...",
			cmds...,
		)
	}
}

// Keep: allows building an array of images on one agent. Useful for streamlining and rules_oci in the future.
func bazelBuildCandidateDockerImages(apps []string, version string, tag string, rt runtype.RunType) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		cmds := []bk.StepOpt{}

		cmds = append(cmds,
			bk.Key(candidateImageStepKey(apps[0])),
			bk.Env("DOCKER_BAZEL", "true"),
			bk.Env("DOCKER_BUILDKIT", "1"),
			bk.Env("VERSION", version),
			bk.Agent("queue", "bazel"),
		)

		// Allow all build scripts to emit info annotations
		// TODO(JH) probably remove
		buildAnnotationOptions := bk.AnnotatedCmdOpts{
			Annotations: &bk.AnnotationOpts{
				Type:         bk.AnnotationTypeInfo,
				IncludeNames: true,
			},
		}

		for _, app := range apps {
			image := strings.ReplaceAll(app, "/", "-")
			localImage := "sourcegraph/" + image + ":" + version

			// Add Sentry environment variables if we are building off main branch
			// to enable building the webapp with source maps enabled
			if rt.Is(runtype.MainDryRun) && app == "frontend" {
				cmds = append(cmds,
					bk.Env("SENTRY_UPLOAD_SOURCE_MAPS", "1"),
					bk.Env("SENTRY_ORGANIZATION", "sourcegraph"),
					bk.Env("SENTRY_PROJECT", "sourcegraph-dot-com"),
				)
			}

			cmds = append(cmds,
				bk.Cmd(fmt.Sprintf(`echo "--- Building candidate %s image..."`, app)),
				bk.Cmd("export IMAGE='"+localImage+"'"),
			)

			if _, err := os.Stat(filepath.Join("docker-images", app)); err == nil {
				// Building Docker image located under $REPO_ROOT/docker-images/
				buildScriptPath := filepath.Join("docker-images", app, "build.sh")
				_, err := os.Stat(filepath.Join("docker-images", app, "build-bazel.sh"))
				if err == nil {
					// If the file exists.
					buildScriptPath = filepath.Join("docker-images", app, "build-bazel.sh")
				}

				cmds = append(cmds,
					bk.Cmd("ls -lah "+buildScriptPath),
					bk.Cmd(buildScriptPath),
				)
			} else if _, err := os.Stat(filepath.Join("client", app)); err == nil {
				// Building Docker image located under $REPO_ROOT/client/
				cmds = append(cmds, bk.AnnotatedCmd("client/"+app+"/build.sh", buildAnnotationOptions))
			} else {
				// Building Docker images located under $REPO_ROOT/cmd/
				cmdDir := func() string {
					folder := app
					if app == "blobstore2" {
						// experiment: cmd/blobstore is a Go rewrite of docker-images/blobstore. While
						// it is incomplete, we do not want cmd/blobstore/Dockerfile to get published
						// under the same name.
						// https://github.com/sourcegraph/sourcegraph/issues/45594
						// TODO(blobstore): remove this when making Go blobstore the default
						folder = "blobstore"
					}
					// If /enterprise/cmd/... does not exist, build just /cmd/... instead.
					if _, err := os.Stat(filepath.Join("enterprise/cmd", folder)); err != nil {
						return "cmd/" + folder
					}
					return "enterprise/cmd/" + folder
				}()
				buildScriptPath := filepath.Join(cmdDir, "build.sh")
				_, err := os.Stat(filepath.Join(cmdDir, "build-bazel.sh"))
				if err == nil {
					// If the file exists.
					buildScriptPath = filepath.Join(cmdDir, "build-bazel.sh")
				}
				cmds = append(cmds, bk.AnnotatedCmd(buildScriptPath, buildAnnotationOptions))
			}

			devImage := images.DevRegistryImage(app, tag)
			cmds = append(cmds,
				bk.Cmd(fmt.Sprintf(`echo "--- Tagging and Pushing candidate %s image..."`, app)),
				// Retag the local image for dev registry
				bk.Cmd(fmt.Sprintf("docker tag %s %s", localImage, devImage)),
				// Publish tagged image
				bk.Cmd(fmt.Sprintf("docker push %s || exit 10", devImage)),
				// Retry in case of flakes when pushing
				// bk.AutomaticRetryStatus(3, 10),
				// Retry in case of flakes when pushing
				// bk.AutomaticRetryStatus(3, 222),
			)
		}
		pipeline.AddStep(":bazel::docker: :construction: Build Docker images", cmds...)
	}
}

func bazelBuildCandidateDockerImage(app string, version string, tag string, rt runtype.RunType) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		cmds := []bk.StepOpt{}
		cmds = append(cmds,
			bk.Key(candidateImageStepKey(app)),
			bk.Env("DOCKER_BAZEL", "true"),
			bk.Env("VERSION", version),
			bk.Agent("queue", "bazel"),
		)

		// Allow all build scripts to emit info annotations
		// TODO(JH) probably remove
		buildAnnotationOptions := bk.AnnotatedCmdOpts{
			Annotations: &bk.AnnotationOpts{
				Type:         bk.AnnotationTypeInfo,
				IncludeNames: true,
			},
		}

		image := strings.ReplaceAll(app, "/", "-")
		localImage := "sourcegraph/" + image + ":" + version

		// Add Sentry environment variables if we are building off main branch
		// to enable building the webapp with source maps enabled
		if rt.Is(runtype.MainDryRun) && app == "frontend" {
			cmds = append(cmds,
				bk.Env("SENTRY_UPLOAD_SOURCE_MAPS", "1"),
				bk.Env("SENTRY_ORGANIZATION", "sourcegraph"),
				bk.Env("SENTRY_PROJECT", "sourcegraph-dot-com"),
			)
		}

		cmds = append(cmds,
			bk.Cmd(fmt.Sprintf(`echo "--- Building candidate %s image..."`, app)),
			bk.Cmd("export IMAGE='"+localImage+"'"),
		)

		if _, err := os.Stat(filepath.Join("docker-images", app)); err == nil {
			// Building Docker image located under $REPO_ROOT/docker-images/
			buildScriptPath := filepath.Join("docker-images", app, "build.sh")
			_, err := os.Stat(filepath.Join("docker-images", app, "build-bazel.sh"))
			if err == nil {
				// If the file exists.
				buildScriptPath = filepath.Join("docker-images", app, "build-bazel.sh")
			}

			cmds = append(cmds,
				bk.Cmd("ls -lah "+buildScriptPath),
				bk.Cmd(buildScriptPath),
			)
		} else if _, err := os.Stat(filepath.Join("client", app)); err == nil {
			// Building Docker image located under $REPO_ROOT/client/
			cmds = append(cmds, bk.AnnotatedCmd("client/"+app+"/build.sh", buildAnnotationOptions))
		} else {
			// Building Docker images located under $REPO_ROOT/cmd/
			cmdDir := func() string {
				folder := app
				if app == "blobstore2" {
					// experiment: cmd/blobstore is a Go rewrite of docker-images/blobstore. While
					// it is incomplete, we do not want cmd/blobstore/Dockerfile to get published
					// under the same name.
					// https://github.com/sourcegraph/sourcegraph/issues/45594
					// TODO(blobstore): remove this when making Go blobstore the default
					folder = "blobstore"
				}
				// If /enterprise/cmd/... does not exist, build just /cmd/... instead.
				if _, err := os.Stat(filepath.Join("enterprise/cmd", folder)); err != nil {
					return "cmd/" + folder
				}
				return "enterprise/cmd/" + folder
			}()
			buildScriptPath := filepath.Join(cmdDir, "build.sh")
			_, err := os.Stat(filepath.Join(cmdDir, "build-bazel.sh"))
			if err == nil {
				// If the file exists.
				buildScriptPath = filepath.Join(cmdDir, "build-bazel.sh")
			}
			cmds = append(cmds, bk.AnnotatedCmd(buildScriptPath, buildAnnotationOptions))
		}

		devImage := images.DevRegistryImage(app, tag)
		cmds = append(cmds,
			bk.Cmd(fmt.Sprintf(`echo "--- Tagging and Pushing candidate %s image..."`, app)),
			// Retag the local image for dev registry
			bk.Cmd(fmt.Sprintf("docker tag %s %s", localImage, devImage)),
			// Publish tagged image
			bk.Cmd(fmt.Sprintf("docker push %s || exit 10", devImage)),
			// Retry in case of flakes when pushing
			// bk.AutomaticRetryStatus(3, 10),
			// Retry in case of flakes when pushing
			// bk.AutomaticRetryStatus(3, 222),
		)
		pipeline.AddStep(fmt.Sprintf(":bazel::docker: :construction: Build %s", app), cmds...)
	}
}

// Tag and push final Docker image for the service defined by `app`
// after the e2e tests pass.
//
// It requires Config as an argument because published images require a lot of metadata.
func bazelPublishFinalDockerImage(c Config, apps []string) operations.Operation {
	return func(pipeline *bk.Pipeline) {
		cmds := []bk.StepOpt{}
		cmds = append(cmds, bk.Agent("queue", "bazel"))

		for _, app := range apps {

			devImage := images.DevRegistryImage(app, "")
			publishImage := images.PublishedRegistryImage(app, "")

			var imgs []string
			for _, image := range []string{publishImage, devImage} {
				if app != "server" || c.RunType.Is(runtype.TaggedRelease, runtype.ImagePatch, runtype.ImagePatchNoTest) {
					imgs = append(imgs, fmt.Sprintf("%s:%s", image, c.Version))
				}

				if app == "server" && c.RunType.Is(runtype.ReleaseBranch) {
					imgs = append(imgs, fmt.Sprintf("%s:%s-insiders", image, c.Branch))
				}

				if c.RunType.Is(runtype.MainBranch) {
					imgs = append(imgs, fmt.Sprintf("%s:insiders", image))
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
				imgs = append(imgs, internalImage)
			}

			candidateImage := fmt.Sprintf("%s:%s", devImage, c.candidateImageTag())
			cmds = append(cmds, bk.Cmd(fmt.Sprintf("./dev/ci/docker-publish.sh %s %s", candidateImage, strings.Join(imgs, " "))))
		}
		pipeline.AddStep(":docker: :truck: Publish images", cmds...)
		// This step just pulls a prebuild image and pushes it to some registries. The
		// only possible failure here is a registry flake, so we retry a few times.
		bk.AutomaticRetry(3)
	}
}
