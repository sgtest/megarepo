package command

import (
	"path/filepath"
)

// NewShellSpec creates a new spec for a shell command.
func NewShellSpec(workingDir string, image string, scriptPath string, spec Spec, options DockerOptions) Spec {
	// TODO - remove this once src-cli is not required anymore for SSBC.
	if image == "" {
		env := spec.Env
		return Spec{
			Key:       spec.Key,
			Command:   spec.Command,
			Dir:       filepath.Join(workingDir, spec.Dir),
			Env:       env,
			Operation: spec.Operation,
		}
	}

	hostDir := workingDir
	if options.Resources.DockerHostMountPath != "" {
		hostDir = filepath.Join(options.Resources.DockerHostMountPath, filepath.Base(workingDir))
	}

	return Spec{
		Key: spec.Key,
		Dir: filepath.Join(hostDir, spec.Dir),
		Env: spec.Env,
		Command: Flatten(
			"/bin/sh",
			filepath.Join(hostDir, ScriptsPath, scriptPath),
		),
		Operation: spec.Operation,
	}
}
