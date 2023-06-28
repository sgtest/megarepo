// Package linters defines all available linters.
package linters

import (
	"bytes"
	"context"
	"os"

	"github.com/sourcegraph/run"
	"go.bobheadxi.dev/streamline/pipeline"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/check"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/repo"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/dev/sg/root"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Target = check.Category[*repo.State]

type linter = check.Check[*repo.State]

// Targets lists all available linter targets. Each target consists of multiple linters.
//
// These should align with the names in 'enterprise/dev/ci/internal/ci/changed'
var Targets = []Target{
	{
		Name:        "urls",
		Description: "Check for broken urls in the codebase",
		Checks: []*linter{
			runScript("Broken urls", "dev/check/broken-urls.bash"),
		},
	},
	{
		Name:        "go",
		Description: "Check go code for linting errors, forbidden imports, generated files, etc",
		Checks: []*linter{
			goFmt,
			goGenerateLinter,
			goDBConnImport,
			goEnterpriseImport,
			noLocalHost,
			lintGoDirectives(),
			lintLoggingLibraries(),
			lintTracingLibraries(),
			goModGuards(),
			lintSGExit(),
		},
	},
	{
		Name:        "graphql",
		Description: "Checks the graphql code for linting errors [bazel]",
		Checks: []*linter{
			onlyLocal(bazelTest("graphql schema lint (bazel)", "//cmd/frontend/graphqlbackend:graphql_schema_lint_test")),
		},
	},
	{
		Name:        "docs",
		Description: "Documentation checks",
		Checks: []*linter{
			onlyLocal(bazelTest("Docsite lint (bazel)", "//doc:test")),
		},
	},
	{
		Name:        "dockerfiles",
		Description: "Check Dockerfiles for Sourcegraph best practices",
		Checks: []*linter{
			hadolint(),
			customDockerfileLinters(),
		},
	},
	{
		Name:        "client",
		Description: "Check client code for linting errors, forbidden imports, etc",
		Checks: []*linter{
			tsEnterpriseImport,
			inlineTemplates,
			runScript("pnpm dedupe", "dev/check/pnpm-deduplicate.sh"),
			// we only run this linter locally, since on CI it has it's own job
			onlyLocal(runScript("pnpm list:js:web", "dev/ci/pnpm-run.sh lint:js:web")),
			checkUnversionedDocsLinks(),
		},
	},
	{
		Name:        "svg",
		Description: "Check svg assets",
		Enabled:     disabled("reported as unreliable"),
		Checks: []*linter{
			checkSVGCompression(),
		},
	},
	{
		Name:        "shell",
		Description: "Check shell code for linting errors, formatting, etc",
		Checks: []*linter{
			shFmt,
			shellCheck,
			bashSyntax,
		},
	},
	{
		Name:        "protobuf",
		Description: "Check protobuf code for linting errors, formatting, etc",
		Checks: []*linter{
			bufFormat,
			bufGenerate,
			bufLint,
		},
	},
	Formatting,
}

var Formatting = Target{
	Name:        "format",
	Description: "Check client code and docs for formatting errors",
	Checks: []*linter{
		prettier,
	},
}

func onlyLocal(l *linter) *linter {
	if os.Getenv("CI") == "true" {
		l.Enabled = func(ctx context.Context, args *repo.State) error {
			return errors.New("check is disabled in CI")
		}
	}
	return l
}

// runScript creates check that runs the given script from the root of sourcegraph/sourcegraph.
func runScript(name string, script string) *linter {
	return &linter{
		Name: name,
		Check: func(ctx context.Context, out *std.Output, args *repo.State) error {
			return root.Run(run.Bash(ctx, script)).StreamLines(out.Write)
		},
	}
}

// runCheck creates a check that runs the given check func.
func runCheck(name string, check check.CheckAction[*repo.State]) *linter {
	return &linter{
		Name:  name,
		Check: check,
	}
}

func bazelTest(name, target string) *linter {
	return &linter{
		Name: name,
		Check: func(ctx context.Context, out *std.Output, args *repo.State) error {
			return root.Run(run.Cmd(ctx, "bazel", "test", target)).StreamLines(out.Write)
		},
	}
}

// pnpmInstallFilter is a pipeline that filters out all the warning junk that pnpm install
// emits that seem inconsequential, for example:
//
//	warning "@storybook/addon-storyshots > react-test-renderer@16.14.0" has incorrect peer dependency "react@^16.14.0".
//	warning "@storybook/addon-storyshots > @storybook/core > @storybook/core-server > @storybook/builder-webpack4 > webpack-filter-warnings-plugin@1.2.1" has incorrect peer dependency "webpack@^2.0.0 || ^3.0.0 || ^4.0.0".
//	warning " > @storybook/react@6.5.9" has unmet peer dependency "require-from-string@^2.0.2".
//	warning "@storybook/react > react-element-to-jsx-string@14.3.4" has incorrect peer dependency "react@^0.14.8 || ^15.0.1 || ^16.0.0 || ^17.0.1".
//	warning " > @testing-library/react-hooks@8.0.0" has incorrect peer dependency "react@^16.9.0 || ^17.0.0".
//	warning "storybook-addon-designs > @figspec/react@1.0.0" has incorrect peer dependency "react@^16.14.0 || ^17.0.0".
//	warning Workspaces can only be enabled in private projects.
//	warning Workspaces can only be enabled in private projects.
func pnpmInstallFilter() pipeline.Pipeline {
	return pipeline.Filter(func(line []byte) bool { return !bytes.Contains(line, []byte("warning")) })
}

// disabled can be used to mark a category or check as disabled.
func disabled(reason string) check.EnableFunc[*repo.State] {
	return func(context.Context, *repo.State) error {
		return errors.Newf("disabled: %s", reason)
	}
}
