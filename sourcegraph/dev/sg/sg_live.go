package main

import (
	"context"
	"flag"
	"fmt"
	"net/url"
	"strings"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var liveCommand = &cli.Command{
	Name:      "live",
	ArgsUsage: "<environment-name-or-url>",
	Usage:     "Reports which version of Sourcegraph is currently live in the given environment",
	UsageText: `
# See which version is deployed on a preset environment
sg live cloud
sg live k8s

# See which version is deployed on a custom environment
sg live https://demo.sourcegraph.com

# List environments:
sg live -help
	`,
	Category:    CategoryCompany,
	Description: constructLiveCmdLongHelp(),
	Action:      execAdapter(liveExec),
	BashComplete: completeOptions(func() (options []string) {
		return append(environmentNames(), `https\://...`)
	}),
}

func constructLiveCmdLongHelp() string {
	var out strings.Builder

	fmt.Fprintf(&out, "Prints the Sourcegraph version deployed to the given environment.")
	fmt.Fprintf(&out, "\n\n")
	fmt.Fprintf(&out, "Available preset environments:\n")

	for _, name := range environmentNames() {
		fmt.Fprintf(&out, "\n* %s", name)
	}

	return out.String()
}

func liveExec(ctx context.Context, args []string) error {
	if len(args) == 0 {
		std.Out.WriteLine(output.Styled(output.StyleWarning, "ERROR: No environment specified"))
		return flag.ErrHelp
	}

	if len(args) != 1 {
		std.Out.WriteLine(output.Styled(output.StyleWarning, "ERROR: Too many arguments"))
		return flag.ErrHelp
	}

	e, ok := getEnvironment(args[0])
	if !ok {
		if customURL, err := url.Parse(args[0]); err == nil && customURL.Scheme != "" {
			e = environment{Name: customURL.Host, URL: customURL.String()}
		} else {
			std.Out.WriteLine(output.Styledf(output.StyleWarning, "ERROR: Environment %q not found, or is not a valid URL :(", args[0]))
			return flag.ErrHelp
		}
	}

	return printDeployedVersion(e)
}
