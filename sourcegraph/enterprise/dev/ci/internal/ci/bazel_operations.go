package ci

import (
	"fmt"
	"strings"

	bk "github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/buildkite"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/ci/operations"
)

const bazelRemoteCacheURL = "https://storage.googleapis.com/sourcegraph_bazel_cache"

func BazelOperations() *operations.Set {
	ops := operations.NewSet()
	ops.Append(bazelBuild(
		"//...",
	))
	ops.Append(bazelTest("//monitoring/...", "//lib/..."))
	return ops
}

func bazelTest(targets ...string) func(*bk.Pipeline) {
	cmds := []bk.StepOpt{
		bk.Env("CI_BAZEL_REMOTE_CACHE", bazelRemoteCacheURL),
		bk.Agent("queue", "bazel"),
	}

	for _, target := range targets {
		bazelCmd := []string{
			"bazel",
			"--bazelrc=.bazelrc",
			"--bazelrc=.aspect/bazelrc/ci.bazelrc",
			fmt.Sprintf("test %s", target),
			"--remote_cache=$$CI_BAZEL_REMOTE_CACHE",
			"--google_credentials=/mnt/gcloud-service-account/gcloud-service-account.json",
		}
		cmds = append(cmds, bk.Cmd(strings.Join(bazelCmd, " ")))
	}

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: Tests",
			cmds...,
		)
	}
}
func bazelBuild(targets ...string) func(*bk.Pipeline) {
	cmds := []bk.StepOpt{
		bk.Env("CI_BAZEL_REMOTE_CACHE", bazelRemoteCacheURL),
		bk.Agent("queue", "bazel"),
	}

	for _, target := range targets {
		bazelCmd := []string{
			"bazel",
			"--bazelrc=.bazelrc",
			"--bazelrc=.aspect/bazelrc/ci.bazelrc",
			fmt.Sprintf("build %s", target),
			"--remote_cache=$$CI_BAZEL_REMOTE_CACHE",
			"--google_credentials=/mnt/gcloud-service-account/gcloud-service-account.json",
		}
		cmds = append(cmds, bk.Cmd(strings.Join(bazelCmd, " ")))
	}

	return func(pipeline *bk.Pipeline) {
		pipeline.AddStep(":bazel: Build ...",
			cmds...,
		)
	}
}
