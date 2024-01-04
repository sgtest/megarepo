// Package managedservicesplatform manages infrastructure-as-code using CDKTF
// for Managed Services Platform (MSP) services.
package managedservicesplatform

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/cloudrun"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/iam"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/monitoring"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/terraformversion"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/options/tfcbackend"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/project"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/stack/tfcworkspaces"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/internal/terraform"
	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/terraformcloud"

	"github.com/sourcegraph/sourcegraph/dev/managedservicesplatform/spec"
)

const (
	// TODO: re-export for use, maybe we should lift stack packages out of
	// internal so that we can share consts, including output names.
	StackNameProject  = project.StackName
	StackNameIAM      = iam.StackName
	StackNameCloudRun = cloudrun.StackName
)

type TerraformCloudOptions struct {
	// Enabled will render all stacks to use a Terraform CLoud workspace as its
	// Terraform state backend with the following format as the workspace name
	// for each stack:
	//
	//  msp-${svc.id}-${env.id}-${stackName}
	//
	// If false, a local backend will be used.
	Enabled bool
}

type GCPOptions struct{}

// Renderer takes MSP service specifications
type Renderer struct {
	// OutputDir is the target directory for generated CDKTF assets.
	OutputDir string
	// TFC declares Terraform-Cloud-specific configuration for rendered CDKTF
	// components.
	TFC TerraformCloudOptions
	// GCPOptions declares GCP-specific configuration for rendered CDKTF components.
	GCP GCPOptions
	// StableGenerate, if true, is propagated to stacks to indicate that any values
	// populated at generation time should not be regenerated.
	StableGenerate bool
}

// RenderEnvironment sets up a CDKTF application comprised of stacks that define
// the infrastructure required to deploy an environment as specified.
func (r *Renderer) RenderEnvironment(
	svc spec.ServiceSpec,
	build spec.BuildSpec,
	env spec.EnvironmentSpec,
	monitoringSpec spec.MonitoringSpec,
) (*CDKTF, error) {
	terraformVersion := terraform.Version
	stackSetOptions := []stack.NewStackOption{
		// Enforce Terraform versions on all stacks
		terraformversion.With(terraformVersion),
	}
	if r.TFC.Enabled {
		// Use a Terraform Cloud backend on all stacks
		stackSetOptions = append(stackSetOptions,
			tfcbackend.With(tfcbackend.Config{
				Workspace: func(stackName string) string {
					return terraformcloud.WorkspaceName(svc, env, stackName)
				},
			}))
	}
	stacks := stack.NewSet(r.OutputDir, stackSetOptions...)

	// If destroys are not allowed, configure relevant resources to prevent
	// destroys.
	preventDestroys := !pointers.DerefZero(env.AllowDestroys)

	// Render all required CDKTF stacks for this environment
	projectOutput, err := project.NewStack(stacks, project.Variables{
		ProjectID: env.ProjectID,
		DisplayName: fmt.Sprintf("%s - %s",
			pointers.Deref(svc.Name, svc.ID), env.ID),

		Category: env.Category,
		Labels: map[string]string{
			"service":     svc.ID,
			"environment": env.ID,
			"msp":         "true",
		},
		Services: func() []string {
			if svc.IAM != nil && len(svc.IAM.Services) > 0 {
				return svc.IAM.Services
			}
			return nil
		}(),
		PreventDestroys: preventDestroys,
	})
	if err != nil {
		return nil, errors.Wrap(err, "failed to create project stack")
	}
	iamOutput, err := iam.NewStack(stacks, iam.Variables{
		ProjectID:       *projectOutput.Project.ProjectId(),
		Image:           build.Image,
		Service:         svc,
		SecretEnv:       env.SecretEnv,
		PreventDestroys: preventDestroys,
	})
	if err != nil {
		return nil, errors.Wrap(err, "failed to create IAM stack")
	}
	cloudrunOutput, err := cloudrun.NewStack(stacks, cloudrun.Variables{
		ProjectID: *projectOutput.Project.ProjectId(),
		IAM:       *iamOutput,

		Service:     svc,
		Image:       build.Image,
		Environment: env,

		StableGenerate: r.StableGenerate,

		PreventDestroys: preventDestroys,
	})
	if err != nil {
		return nil, errors.Wrap(err, "failed to create cloudrun stack")
	}
	if _, err := monitoring.NewStack(stacks, monitoring.Variables{
		ProjectID:  *projectOutput.Project.ProjectId(),
		Service:    svc,
		Monitoring: monitoringSpec,
		MaxInstanceCount: func() *int {
			if env.Instances.Scaling != nil {
				return env.Instances.Scaling.MaxCount
			}
			return nil
		}(),
		RedisInstanceID:     cloudrunOutput.RedisInstanceID,
		ServiceStartupProbe: pointers.DerefZero(env.EnvironmentServiceSpec).StatupProbe,

		// Notification configuration
		EnvironmentCategory: env.Category,
		EnvironmentID:       env.ID,
		Owners:              svc.Owners,
	}); err != nil {
		return nil, errors.Wrap(err, "failed to create monitoring stack")
	}

	// If TFC is enabled, render the TFC workspace runs stack to manage initial
	// applies/teardowns and other configuration.
	if r.TFC.Enabled {
		if _, err := tfcworkspaces.NewStack(stacks, tfcworkspaces.Variables{
			PreviousStacks: stack.ExtractStacks(stacks),
			// TODO: Maybe include spec option to disable notifications
			EnableNotifications: true,
		}); err != nil {
			return nil, errors.Wrap(err, "failed to create TFC workspace runs stack")
		}
	}

	// Return CDKTF representation for caller to synthesize
	return &CDKTF{
		app:              stack.ExtractApp(stacks),
		stacks:           stack.ExtractStacks(stacks),
		terraformVersion: terraformVersion,
	}, nil
}
