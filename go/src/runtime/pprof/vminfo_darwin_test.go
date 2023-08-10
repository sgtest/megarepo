// Copyright 2023 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//go:build !ios

package pprof

import (
	"bufio"
	"bytes"
	"internal/abi"
	"internal/testenv"
	"os"
	"strconv"
	"strings"
	"testing"
)

func TestVMInfo(t *testing.T) {
	var begin, end, offset uint64
	var filename string
	first := true
	machVMInfo(func(lo, hi, off uint64, file, buildID string) {
		if first {
			begin = lo
			end = hi
			offset = off
			filename = file
		}
		// May see multiple text segments if rosetta is used for running
		// the go toolchain itself.
		first = false
	})
	lo, hi := useVMMap(t)
	if got, want := begin, lo; got != want {
		t.Errorf("got %x, want %x", got, want)
	}
	if got, want := end, hi; got != want {
		t.Errorf("got %x, want %x", got, want)
	}
	if got, want := offset, uint64(0); got != want {
		t.Errorf("got %x, want %x", got, want)
	}
	if !strings.HasSuffix(filename, "pprof.test") {
		t.Errorf("got %s, want pprof.test", filename)
	}
	addr := uint64(abi.FuncPCABIInternal(TestVMInfo))
	if addr < lo || addr > hi {
		t.Errorf("%x..%x does not contain function %p (%x)", lo, hi, TestVMInfo, addr)
	}
}

func useVMMap(t *testing.T) (hi, lo uint64) {
	pid := strconv.Itoa(os.Getpid())
	testenv.MustHaveExecPath(t, "vmmap")
	out, err := testenv.Command(t, "vmmap", pid).Output()
	if err != nil {
		t.Fatal(err)
	}
	return parseVmmap(t, out)
}

// parseVmmap parses the output of vmmap and calls addMapping for the first r-x TEXT segment in the output.
func parseVmmap(t *testing.T, data []byte) (hi, lo uint64) {
	// vmmap 53799
	// Process:         gopls [53799]
	// Path:            /Users/USER/*/gopls
	// Load Address:    0x1029a0000
	// Identifier:      gopls
	// Version:         ???
	// Code Type:       ARM64
	// Platform:        macOS
	// Parent Process:  Code Helper (Plugin) [53753]
	//
	// Date/Time:       2023-05-25 09:45:49.331 -0700
	// Launch Time:     2023-05-23 09:35:37.514 -0700
	// OS Version:      macOS 13.3.1 (22E261)
	// Report Version:  7
	// Analysis Tool:   /Applications/Xcode.app/Contents/Developer/usr/bin/vmmap
	// Analysis Tool Version:  Xcode 14.3 (14E222b)
	//
	// Physical footprint:         1.2G
	// Physical footprint (peak):  1.2G
	// Idle exit:                  untracked
	// ----
	//
	// Virtual Memory Map of process 53799 (gopls)
	// Output report format:  2.4  -64-bit process
	// VM page size:  16384 bytes
	//
	// ==== Non-writable regions for process 53799
	// REGION TYPE                    START END         [ VSIZE  RSDNT  DIRTY   SWAP] PRT/MAX SHRMOD PURGE    REGION DETAIL
	// __TEXT                      1029a0000-1033bc000    [ 10.1M  7360K     0K     0K] r-x/rwx SM=COW          /Users/USER/*/gopls
	// __DATA_CONST                1033bc000-1035bc000    [ 2048K  2000K     0K     0K] r--/rwSM=COW          /Users/USER/*/gopls
	// __DATA_CONST                1035bc000-103a48000    [ 4656K  3824K     0K     0K] r--/rwSM=COW          /Users/USER/*/gopls
	// __LINKEDIT                  103b00000-103c98000    [ 1632K  1616K     0K     0K] r--/r-SM=COW          /Users/USER/*/gopls
	// dyld private memory         103cd8000-103cdc000    [   16K     0K     0K     0K] ---/--SM=NUL
	// shared memory               103ce4000-103ce8000    [   16K    16K    16K     0K] r--/r-SM=SHM
	// MALLOC metadata             103ce8000-103cec000    [   16K    16K    16K     0K] r--/rwx SM=COW          DefaultMallocZone_0x103ce8000 zone structure
	// MALLOC guard page           103cf0000-103cf4000    [   16K     0K     0K     0K] ---/rwx SM=COW
	// MALLOC guard page           103cfc000-103d00000    [   16K     0K     0K     0K] ---/rwx SM=COW
	// MALLOC guard page           103d00000-103d04000    [   16K     0K     0K     0K] ---/rwx SM=NUL

	banner := "==== Non-writable regions for process"
	grabbing := false
	sc := bufio.NewScanner(bytes.NewReader(data))
	for sc.Scan() {
		l := sc.Text()
		if grabbing {
			p := strings.Fields(l)
			if len(p) > 7 && p[0] == "__TEXT" && p[7] == "r-x/rwx" {
				locs := strings.Split(p[1], "-")
				start, _ := strconv.ParseUint(locs[0], 16, 64)
				end, _ := strconv.ParseUint(locs[1], 16, 64)
				return start, end
			}
		}
		if strings.HasPrefix(l, banner) {
			grabbing = true
		}
	}
	t.Fatal("vmmap no text segment found")
	return 0, 0
}
