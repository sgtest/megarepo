// Copyright 2023 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

package gover

import (
	"cmd/go/internal/base"
	"errors"
	"fmt"
	"strings"
)

// ToolchainVersion returns the Go version for the named toolchain,
// derived from the name itself (not by running the toolchain).
// A toolchain is named "goVERSION" or "anything-goVERSION".
// A suffix after the VERSION introduced by a +, -, space, or tab is removed.
// Examples:
//
//	ToolchainVersion("go1.2.3") == "1.2.3"
//	ToolchainVersion("go1.2.3+bigcorp") == "1.2.3"
//	ToolchainVersion("go1.2.3-bigcorp") == "1.2.3"
//	ToolchainVersion("gccgo-go1.23rc4") == "1.23rc4"
//	ToolchainVersion("invalid") == ""
func ToolchainVersion(name string) string {
	var v string
	if strings.HasPrefix(name, "go") {
		v = name[2:]
	} else if i := strings.Index(name, "-go"); i >= 0 {
		v = name[i+3:]
	}
	// Some builds use custom suffixes; strip them.
	if i := strings.IndexAny(v, " \t+-"); i >= 0 {
		v = v[:i]
	}
	if !IsValid(v) {
		return ""
	}
	return v
}

func maybeToolchainVersion(name string) string {
	if IsValid(name) {
		return name
	}
	return ToolchainVersion(name)
}

// Startup records the information that went into the startup-time version switch.
// It is initialized by switchGoToolchain.
var Startup struct {
	GOTOOLCHAIN   string // $GOTOOLCHAIN setting
	AutoFile      string // go.mod or go.work file consulted
	AutoGoVersion string // go line found in file
	AutoToolchain string // toolchain line found in file
}

// A TooNewError explains that a module is too new for this version of Go.
type TooNewError struct {
	What      string
	GoVersion string
}

func (e *TooNewError) Error() string {
	var explain string
	if Startup.GOTOOLCHAIN != "" && Startup.GOTOOLCHAIN != "auto" {
		explain = "; GOTOOLCHAIN=" + Startup.GOTOOLCHAIN
	}
	if Startup.AutoFile != "" && (Startup.AutoGoVersion != "" || Startup.AutoToolchain != "") {
		explain += fmt.Sprintf("; %s sets ", base.ShortPath(Startup.AutoFile))
		if Startup.AutoGoVersion != "" {
			explain += "go " + Startup.AutoGoVersion
			if Startup.AutoToolchain != "" {
				explain += ", "
			}
		}
		if Startup.AutoToolchain != "" {
			explain += "toolchain " + Startup.AutoToolchain
		}
	}
	return fmt.Sprintf("%v requires go %v (running go %v%v)", e.What, e.GoVersion, Local(), explain)
}

var ErrTooNew = errors.New("module too new")

func (e *TooNewError) Is(err error) bool {
	return err == ErrTooNew
}
