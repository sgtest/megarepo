package command

import (
	"context"
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestFormatFirecrackerCommandRaw(t *testing.T) {
	actual := formatFirecrackerCommand(
		CommandSpec{
			Command: []string{"ls", "-a"},
			Dir:     "sub dir",
			Env: []string{
				`TEST=true`,
				`CONTAINS_WHITESPACE=yes it does`,
			},
			Operation: makeTestOperation(),
		},
		"deadbeef",
		Options{},
		"/tmp/docker-config",
	)

	expected := command{
		Command: []string{
			"ignite", "exec", "deadbeef", "--",
			`cd '/work/sub dir' && TEST=true CONTAINS_WHITESPACE='yes it does' DOCKER_CONFIG=/tmp/docker-config ls -a`,
		},
	}
	if diff := cmp.Diff(expected, actual, commandComparer); diff != "" {
		t.Errorf("unexpected command (-want +got):\n%s", diff)
	}
}

func TestFormatFirecrackerCommandDockerScript(t *testing.T) {
	actual := formatFirecrackerCommand(
		CommandSpec{
			Image:      "alpine:latest",
			ScriptPath: "myscript.sh",
			Dir:        "sub dir",
			Env: []string{
				`TEST=true`,
				`CONTAINS_WHITESPACE=yes it does`,
			},
			Operation: makeTestOperation(),
		},
		"deadbeef",
		Options{
			ResourceOptions: ResourceOptions{
				NumCPUs: 4,
				Memory:  "20G",
			},
		},
		"/tmp/docker-config",
	)

	expected := command{
		Command: []string{
			"ignite", "exec", "deadbeef", "--",
			strings.Join([]string{
				"docker",
				"--config", "/tmp/docker-config",
				"run", "--rm",
				"--cpus", "4",
				"--memory", "20G",
				"-v", "/work:/data",
				"-w", "'/data/sub dir'",
				"-e", "TEST=true",
				"-e", `'CONTAINS_WHITESPACE=yes it does'`,
				"--entrypoint /bin/sh",
				"alpine:latest",
				"/data/.sourcegraph-executor/myscript.sh",
			}, " "),
		},
	}

	if diff := cmp.Diff(expected, actual, commandComparer); diff != "" {
		t.Errorf("unexpected command (-want +got):\n%s", diff)
	}
}

func TestFormatFirecrackerCommandDockerScript_NoInjection(t *testing.T) {
	actual := formatFirecrackerCommand(
		CommandSpec{
			Image:      "--privileged alpine:latest",
			ScriptPath: "myscript.sh",
			Operation:  makeTestOperation(),
		},
		"deadbeef",
		Options{},
		"/tmp/docker-config",
	)

	expected := command{
		Command: []string{
			"ignite", "exec", "deadbeef", "--",
			strings.Join([]string{
				"docker",
				"--config", "/tmp/docker-config",
				"run", "--rm",
				"-v", "/work:/data",
				"-w", "/data",
				"--entrypoint /bin/sh",
				// This has to be quoted, otherwise it allows to pass arbitrary params.
				"'--privileged alpine:latest'",
				"/data/.sourcegraph-executor/myscript.sh",
			}, " "),
		},
	}
	if diff := cmp.Diff(expected, actual, commandComparer); diff != "" {
		t.Errorf("unexpected command (-want +got):\n%s", diff)
	}
}

func TestSetupFirecracker(t *testing.T) {
	runner := NewMockCommandRunner()
	options := Options{
		FirecrackerOptions: FirecrackerOptions{
			Enabled:             true,
			Image:               "sourcegraph/executor-vm:3.43.1",
			KernelImage:         "ignite-kernel:5.10.135",
			SandboxImage:        "sourcegraph/ignite:v0.10.5",
			VMStartupScriptPath: "/vm-startup.sh",
		},
		ResourceOptions: ResourceOptions{
			NumCPUs:   4,
			Memory:    "20G",
			DiskSpace: "1T",
		},
	}
	operations := NewOperations(&observation.TestContext)

	tmpDir, err := os.MkdirTemp("", "test-setup-firecracker")
	if err != nil {
		t.Fatal(err)
	}
	logger := NewMockLogger()
	if _, err := setupFirecracker(context.Background(), runner, logger, "deadbeef", "/dev/loopX", tmpDir, options, operations); err != nil {
		t.Fatalf("unexpected error setting up virtual machine: %s", err)
	}

	var actual []string
	for _, call := range runner.RunCommandFunc.History() {
		actual = append(actual, strings.Join(call.Arg1.Command, " "))
	}

	expected := []string{
		strings.Join([]string{
			"ignite run",
			"--runtime docker --network-plugin cni",
			"--cpus 4 --memory 20G --size 1T",
			"--copy-files /vm-startup.sh:/vm-startup.sh",
			"--volumes /dev/loopX:/work",
			"--ssh --name deadbeef",
			"--kernel-image", "ignite-kernel:5.10.135",
			"--kernel-args", "console=ttyS0 reboot=k panic=1 pci=off ip=dhcp random.trust_cpu=on i8042.noaux i8042.nomux i8042.nopnp i8042.dumbkbd",
			"--sandbox-image", "sourcegraph/ignite:v0.10.5",
			"sourcegraph/executor-vm:3.43.1",
		}, " "),
		"ignite exec deadbeef -- /vm-startup.sh",
	}
	if diff := cmp.Diff(expected, actual); diff != "" {
		t.Errorf("unexpected commands (-want +got):\n%s", diff)
	}
}

func TestTeardownFirecracker(t *testing.T) {
	runner := NewMockCommandRunner()
	operations := NewOperations(&observation.TestContext)

	if err := teardownFirecracker(context.Background(), runner, nil, "deadbeef", "/tmp/firecracker123", operations); err != nil {
		t.Fatalf("unexpected error tearing down virtual machine: %s", err)
	}

	var actual []string
	for _, call := range runner.RunCommandFunc.History() {
		actual = append(actual, strings.Join(call.Arg1.Command, " "))
	}

	expected := []string{
		"ignite rm -f deadbeef",
	}
	if diff := cmp.Diff(expected, actual); diff != "" {
		t.Errorf("unexpected commands (-want +got):\n%s", diff)
	}
}

func TestSanitizeImage(t *testing.T) {
	image := "sourcegraph/executor-vm"
	tag := ":3.43.1"
	digest := "@sha256:e54a802a8bec44492deee944acc560e4e0a98f6ffa9a5038f0abac1af677e134"

	testCases := map[string]string{
		"":                   "",          // no regex match (no crash)
		image:                image,       // no tag or hash
		image + digest:       image,       // remove hash without tag
		image + tag:          image + tag, // tag only
		image + tag + digest: image + tag, // tag and hash - keep only tag
	}

	for input, expected := range testCases {
		name := fmt.Sprintf("input=%s", input)

		t.Run(name, func(t *testing.T) {
			if image := sanitizeImage(input); image != expected {
				t.Errorf("unexpected image. want=%q have=%q", expected, image)
			}
		})
	}
}
