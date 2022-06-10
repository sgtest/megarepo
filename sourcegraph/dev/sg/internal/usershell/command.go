package usershell

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strings"

	"github.com/sourcegraph/run"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type wrapped struct {
	ShellPath  string
	ShellFlags []string
	Command    string
	Environ    []string
}

// wrap builds a wrapping for executing the given command in a new shell process.
func wrap(ctx context.Context, cmd string) wrapped {
	// Defaults
	w := wrapped{
		ShellPath:  ShellPath(ctx),
		ShellFlags: []string{"-c"},
		Command:    cmd,
		Environ:    os.Environ(),
	}

	switch {
	case os.Getenv("SG_DEV_NO_RELOAD_ENV") != "":
		// If the user does not want the auto env reloading mechanism, just
		// perform a standard command.

	case ShellType(ctx) == FishShell:
		w.Command = fmt.Sprintf("fish || true; %s", cmd)

	case runtime.GOOS == "linux":
		// The default Ubuntu bashrc comes with a caveat that prevents the bashrc to be
		// reloaded unless the shell is interactive. Therefore, we need to request for an
		// interactive one.
		//
		// But because we are running an interactive shell, we also need to exit explictly.
		// To avoid messing up with the output checking that depends on this function,
		// we silence the exit commands, which otherwise, prints "exit".
		w.Command = fmt.Sprintf("%s; \nexit $? 2>/dev/null", strings.TrimSpace(cmd))
		w.ShellFlags = append(w.ShellFlags, "-i")

	default:
		// The above interactive shell approach fails on OSX because the default shell configuration
		// prints sessions restoration informations that will mess with the output. So we fall back
		// to manually reloading the shell configuration.
		w.Command = fmt.Sprintf("source %s || true; %s", ShellConfigPath(ctx), cmd)
	}

	if ShellType(ctx) == ZshShell {
		// Set this env var for oh-my-zsh users so that oh-my-zsh does not try to
		// auto-update itself when we're restarting the shell.
		w.Environ = append(w.Environ, "DISABLE_AUTO_UPDATE=true")
	}

	return w
}

// Cmd returns a command wrapped in a new shell process, enabling
// changes added by various checks to be run. This negates the new to ask the
// user to restart sg for many checks.
func Cmd(ctx context.Context, cmd string) *exec.Cmd {
	w := wrap(ctx, cmd)

	wrappedCmd := exec.CommandContext(ctx, w.ShellPath, append(w.ShellFlags, w.Command)...)
	wrappedCmd.Env = w.Environ
	return wrappedCmd
}

// CombinedExec runs a command in a fresh shell environment, and returns
// stderr and stdout combined, along with an error.
func CombinedExec(ctx context.Context, cmd string) ([]byte, error) {
	if cmd == "" {
		return nil, errors.Errorf("can't execute empty command")
	}
	return Cmd(ctx, cmd).CombinedOutput()
}

// Command runs a command in a fresh shell environment, and returns run.Command.
func Command(ctx context.Context, parts ...string) *run.Command {
	w := wrap(ctx, strings.Join(parts, " "))
	return run.Cmd(ctx, w.ShellPath, strings.Join(w.ShellFlags, " "), run.Arg(w.Command)).
		Environ(w.Environ)
}

// Command runs a command in a fresh shell environment, runs it, and returns run.Output.
func Run(ctx context.Context, parts ...string) run.Output {
	return Command(ctx, parts...).Run()
}
