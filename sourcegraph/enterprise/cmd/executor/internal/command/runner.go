package command

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Runner is the interface between an executor and the host on which commands
// are invoked. Having this interface at this level allows us to use the same
// code paths for local development (via shell + docker) as well as production
// usage (via Firecracker).
type Runner interface {
	// Setup prepares the runner to invoke a series of commands.
	Setup(ctx context.Context) error

	// Teardown disposes of any resources created in Setup.
	Teardown(ctx context.Context) error

	// Run invokes a command in the runner context.
	Run(ctx context.Context, command CommandSpec) error
}

// CommandSpec represents a command that can be run on a machine, whether that
// is the host, in a virtual machine, or in a docker container. If an image is
// supplied, then the command will be run in a one-shot docker container.
type CommandSpec struct {
	Key        string
	Image      string
	ScriptPath string
	Command    []string
	Dir        string
	Env        []string
	Operation  *observation.Operation
}

type Options struct {
	// ExecutorName is a unique identifier for the requesting executor.
	ExecutorName string

	// DockerOptions configures the behavior of docker container creation.
	DockerOptions DockerOptions

	// FirecrackerOptions configures the behavior of Firecracker virtual machine creation.
	FirecrackerOptions FirecrackerOptions

	// ResourceOptions configures the resource limits of docker container and Firecracker
	// virtual machines running on the executor.
	ResourceOptions ResourceOptions
}

type DockerOptions struct {
	// DockerAuthConfig, if set, will be used to configure the docker CLI to authenticate to
	// registries.
	DockerAuthConfig executor.DockerAuthConfig
	// AddHostGateway, if set, will add a host entry and route to the daemon host to the
	// container. This can be useful to add host.docker.internal as an endpoint inside
	// the container.
	AddHostGateway bool
}

type FirecrackerOptions struct {
	// Enabled determines if commands will be run in Firecracker virtual machines.
	Enabled bool

	// Image is the base image used for all Firecracker virtual machines.
	Image string

	// KernelImage is the base image containing the kernel binary for all Firecracker
	// virtual machines.
	KernelImage string

	// SandboxImage is the docker image used by ignite for isolation of the Firecracker
	// process.
	SandboxImage string

	// VMStartupScriptPath is a path to a file on the host that is loaded into a fresh
	// virtual machine and executed on startup.
	VMStartupScriptPath string

	// DockerRegistryMirrorURLs is an optional parameter to configure docker
	// registry mirrors for the VMs docker daemon on startup. When set, /etc/docker/daemon.json
	// will be mounted into the VM.
	DockerRegistryMirrorURLs []string
}

type ResourceOptions struct {
	// NumCPUs is the number of virtual CPUs a container or VM can use.
	NumCPUs int

	// Memory is the maximum amount of memory a container or VM can use.
	Memory string

	// DiskSpace is the maximum amount of disk a container or VM can use.
	// Only available in firecracker.
	DiskSpace string

	// MaxIngressBandwidth configures the maximum permissible ingress bytes per second
	// per job. Only available in Firecracker.
	MaxIngressBandwidth int

	// MaxEgressBandwidth configures the maximum permissible egress bytes per second
	// per job. Only available in Firecracker.
	MaxEgressBandwidth int

	// DockerHostMountPath, if supplied, replaces the workspace parent directory in the
	// volume mounts of Docker containers. This option is used when running privileged
	// executors in k8s or docker-compose without requiring the host and node paths to
	// be identical.
	DockerHostMountPath string
}

// NewRunner creates a new runner with the given options.
func NewRunner(dir string, logger Logger, options Options, operations *Operations) Runner {
	if !options.FirecrackerOptions.Enabled {
		return &dockerRunner{
			dir:       dir,
			logger:    log.Scoped("docker-runner", ""),
			cmdLogger: logger,
			options:   options,
		}
	}

	return &firecrackerRunner{
		name:            options.ExecutorName,
		workspaceDevice: dir,
		logger:          logger,
		options:         options,
		operations:      operations,
	}
}

type dockerRunner struct {
	dir       string
	logger    log.Logger
	cmdLogger Logger
	options   Options
	// tmpDir is used to store temporary files used for docker execution.
	tmpDir           string
	dockerConfigPath string
}

var _ Runner = &dockerRunner{}

func (r *dockerRunner) Setup(ctx context.Context) error {
	dir, err := os.MkdirTemp("", "executor-docker-runner")
	if err != nil {
		return errors.Wrap(err, "failed to create tmp dir for docker runner")
	}
	r.tmpDir = dir

	// If docker auth config is present, write it.
	if len(r.options.DockerOptions.DockerAuthConfig.Auths) > 0 {
		d, err := json.Marshal(r.options.DockerOptions.DockerAuthConfig)
		if err != nil {
			return err
		}
		r.dockerConfigPath, err = os.MkdirTemp(r.tmpDir, "docker_auth")
		if err != nil {
			return err
		}
		if err := os.WriteFile(filepath.Join(r.dockerConfigPath, "config.json"), d, os.ModePerm); err != nil {
			return err
		}
	}

	return nil
}

func (r *dockerRunner) Teardown(ctx context.Context) error {
	if err := os.RemoveAll(r.tmpDir); err != nil {
		r.logger.Error("Failed to remove docker state tmp dir", log.String("tmpDir", r.tmpDir), log.Error(err))
	}

	return nil
}

func (r *dockerRunner) Run(ctx context.Context, command CommandSpec) error {
	return runCommand(ctx, formatRawOrDockerCommand(command, r.dir, r.options, r.dockerConfigPath), r.cmdLogger)
}

type firecrackerRunner struct {
	name            string
	workspaceDevice string
	logger          Logger
	options         Options
	// tmpDir is used to store temporary files used for firecracker execution.
	tmpDir           string
	operations       *Operations
	dockerConfigPath string
}

var _ Runner = &firecrackerRunner{}

func (r *firecrackerRunner) Setup(ctx context.Context) error {
	dir, err := os.MkdirTemp("", "executor-firecracker-runner")
	if err != nil {
		return errors.Wrap(err, "failed to create tmp dir for firecracker runner")
	}
	r.tmpDir = dir

	dockerConfigPath, err := setupFirecracker(ctx, defaultRunner, r.logger, r.name, r.workspaceDevice, r.tmpDir, r.options, r.operations)
	r.dockerConfigPath = dockerConfigPath
	return err
}

func (r *firecrackerRunner) Teardown(ctx context.Context) error {
	return teardownFirecracker(ctx, defaultRunner, r.logger, r.name, r.tmpDir, r.operations)
}

func (r *firecrackerRunner) Run(ctx context.Context, command CommandSpec) error {
	return runCommand(ctx, formatFirecrackerCommand(command, r.name, r.options, r.dockerConfigPath), r.logger)
}

type runnerWrapper struct{}

var defaultRunner = &runnerWrapper{}

func (runnerWrapper) RunCommand(ctx context.Context, command command, logger Logger) error {
	return runCommand(ctx, command, logger)
}
