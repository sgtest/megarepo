package managedservicesplatform

import (
	"fmt"
	"os"
	"path/filepath"

	jsiiruntime "github.com/aws/jsii-runtime-go"
	"github.com/hashicorp/terraform-cdk-go/cdktf"
	"github.com/sourcegraph/conc/panics"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type CDKTF struct {
	app    cdktf.App
	stacks []stack.Stack

	terraformVersion string
}

// OutputDir is the directory that Synthesize will place output in.
func (c CDKTF) OutputDir() string {
	if s := c.app.Outdir(); s != nil {
		return *s
	}
	return ""
}

// Synthesize all resources to the output directory that was originally
// configured.
func (c CDKTF) Synthesize() error {
	// Forcibly shut down the JSII runtime post-Synth to make sure that we don't
	// get bizarre side-effects from multiple apps being rendered.
	defer jsiiruntime.Close()

	// CDKTF is prone to panics for no good reason, so make a best-effort
	// attempt to capture them.
	var catcher panics.Catcher
	catcher.Try(c.app.Synth)
	if recovered := catcher.Recovered(); recovered != nil {
		return errors.Wrap(recovered, "failed to synthesize Terraform CDK app")
	}

	// Generate tfvar files, if any are required
	for _, stack := range c.stacks {
		// If nil or empty, do nothing
		if len(stack.DynamicVariables) == 0 {
			continue
		}

		tfvarPath := filepath.Join(c.OutputDir(), "stacks", stack.Name, "terraform.tfvars")
		_ = os.Remove(tfvarPath)
		if err := os.WriteFile(tfvarPath,
			stack.DynamicVariables.RenderTFVarsFile(),
			0644); err != nil {
			return errors.Wrap(err, "generate dynamic.tfvars")
		}
	}

	// Generate an asdf tool-version file for convenience to align Terraform
	// with the configured Terraform versions of the generated stacks.
	toolVersionsPath := filepath.Join(c.OutputDir(), ".tool-versions")
	_ = os.Remove(toolVersionsPath)
	if err := os.WriteFile(toolVersionsPath,
		[]byte(fmt.Sprintf("terraform %s", c.terraformVersion)),
		0644); err != nil {
		return errors.Wrap(err, "generate .tool-versions")
	}

	return nil
}

func (c CDKTF) Stacks() (names []string) {
	for _, s := range c.stacks {
		names = append(names, s.Name)
	}
	return names
}
