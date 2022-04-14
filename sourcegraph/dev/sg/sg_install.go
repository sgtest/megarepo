package main

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/stdout"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var installCommand = &cli.Command{
	Name:  "install",
	Usage: "Installs sg to a user-defined location by copying sg itself",
	Description: `Installs sg to a user-defined location by copying sg itself.

Can also be used to install a custom build of 'sg' globally, for example:

	go build -o ./sg ./dev/sg && ./sg install -f -p=false
`,
	Category: CategoryUtil,
	Hidden:   true, // usually an internal command used during installation script
	Flags: []cli.Flag{
		&cli.BoolFlag{
			Name:    "force",
			Aliases: []string{"f"},
			Usage:   "Overwrite existing sg installation",
		},
		&cli.BoolFlag{
			Name:    "profile",
			Aliases: []string{"p"},
			Usage:   "Update profile during installation",
			Value:   true,
		},
	},
	Action: installAction,
}

func installAction(cmd *cli.Context) error {
	ctx := cmd.Context

	probeCmdOut, err := exec.CommandContext(ctx, "sg", "help").CombinedOutput()
	if err == nil && outputLooksLikeSG(string(probeCmdOut)) {
		path, err := exec.LookPath("sg")
		if err != nil {
			return err
		}
		// Looks like sg is already installed.
		if cmd.Bool("force") {
			writeOrangeLinef("Removing existing 'sg' installation at %s.", path)
			if err := os.Remove(path); err != nil {
				return err
			}
		} else {
			// Instead of overwriting anything we let the user know and exit.
			writeFingerPointingLinef("Looks like 'sg' is already installed at %s.", path)
			writeOrangeLinef("Skipping installation.")
			return nil
		}
	}

	var location string
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return err
	}

	switch runtime.GOOS {
	case "linux":
		location = filepath.Join(homeDir, ".local", "bin", "sg")
	case "darwin":
		// We're using something in the home directory because on a fresh macOS
		// installation the user doesn't have permission to create/open/write
		// to /usr/local/bin. We're safe with ~/.sg/sg.
		location = filepath.Join(homeDir, ".sg", "sg")
	default:
		return errors.Newf("unsupported platform: %s", runtime.GOOS)
	}

	var logoOut bytes.Buffer
	printLogo(&logoOut)
	stdout.Out.Write(logoOut.String())

	stdout.Out.Write("")
	stdout.Out.WriteLine(output.Linef("", output.StyleLogo, "Welcome to the sg installation!"))

	// Do not prompt for installation if we are forcefully installing
	if !cmd.Bool("force") {
		stdout.Out.Write("")
		stdout.Out.Writef("We are going to install %ssg%s to %s%s%s. Okay?", output.StyleBold, output.StyleReset, output.StyleBold, location, output.StyleReset)

		locationOkay := getBool()
		if !locationOkay {
			return errors.New("user not happy with location :(")
		}
	}

	currentLocation, err := os.Executable()
	if err != nil {
		return err
	}

	pending := stdout.Out.Pending(output.Linef("", output.StylePending, "Copying from %s%s%s to %s%s%s...", output.StyleBold, currentLocation, output.StyleReset, output.StyleBold, location, output.StyleReset))

	original, err := os.Open(currentLocation)
	if err != nil {
		pending.Complete(output.Linef(output.EmojiFailure, output.StyleWarning, "Failed: %s", err))
		return err
	}
	defer original.Close()

	// Make sure directory for new file exists
	sgDir := filepath.Dir(location)
	if err := os.MkdirAll(sgDir, os.ModePerm); err != nil {
		pending.Complete(output.Linef(output.EmojiFailure, output.StyleWarning, "Failed: %s", err))
		return err
	}

	// Create new file
	newFile, err := os.OpenFile(location, os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0755)
	if err != nil {
		pending.Complete(output.Linef(output.EmojiFailure, output.StyleWarning, "Failed: %s", err))
		return err
	}
	defer newFile.Close()

	_, err = io.Copy(newFile, original)
	if err != nil {
		pending.Complete(output.Linef(output.EmojiFailure, output.StyleWarning, "Failed: %s", err))
		return err
	}
	pending.Complete(output.Linef(output.EmojiSuccess, output.StyleSuccess, "Done!"))

	// Update profile files
	if cmd.Bool("profile") {
		if err := updateProfiles(homeDir, sgDir); err != nil {
			return err
		}
	}

	stdout.Out.Write("")
	stdout.Out.Writef("Restart your shell and run 'sg logo' to make sure the installation worked!")

	return nil
}

func outputLooksLikeSG(out string) bool {
	// This is a weak check, but it's better than anything else we have
	return strings.Contains(out, "logo") &&
		strings.Contains(out, "setup") &&
		strings.Contains(out, "doctor")
}

func updateProfiles(homeDir, sgDir string) error {
	// We add this to all three files, creating them if necessary, because on
	// completely new machines it's hard to detect what gets sourced when.
	// (On a fresh macOS installation .zshenv doesn't exist, but zsh is the
	// default shell, but adding something to ~/.profile will only get read by
	// logging out and back in)
	paths := []string{
		filepath.Join(homeDir, ".zshenv"),
		filepath.Join(homeDir, ".bashrc"),
		filepath.Join(homeDir, ".profile"),
	}

	stdout.Out.Write("")
	stdout.Out.Writef("The path %s%s%s will be added to your %sPATH%s environment variable by", output.StyleBold, sgDir, output.StyleReset, output.StyleBold, output.StyleReset)
	stdout.Out.Writef("modifying the profile files located at:")
	stdout.Out.Write("")
	for _, p := range paths {
		stdout.Out.Writef("  %s%s", output.StyleBold, p)
	}

	addToShellOkay := getBool()
	if !addToShellOkay {
		stdout.Out.Writef("Alright! Make sure to add %s to your $PATH, restart your shell and run 'sg logo'. See you!", sgDir)
		return nil
	}

	pending := stdout.Out.Pending(output.Linef("", output.StylePending, "Writing to files..."))

	exportLine := fmt.Sprintf("\nexport PATH=%s:$PATH\n", sgDir)
	lineWrittenTo := []string{}
	for _, p := range paths {
		f, err := os.OpenFile(p, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
		if err != nil {
			return errors.Wrapf(err, "failed to open %s", p)
		}
		defer f.Close()

		if _, err := f.WriteString(exportLine); err != nil {
			return errors.Wrapf(err, "failed to write to %s", p)
		}

		lineWrittenTo = append(lineWrittenTo, p)
	}

	pending.Complete(output.Linef(output.EmojiSuccess, output.StyleSuccess, "Done!"))

	stdout.Out.Writef("Modified the following files:")
	stdout.Out.Write("")
	for _, p := range lineWrittenTo {
		stdout.Out.Writef("  %s%s", output.StyleBold, p)
	}
	return nil
}
