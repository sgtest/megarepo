#!/bin/bash
# Copyright 2022 The Go Authors. All rights reserved.
# Use of this source code is governed by a BSD-style
# license that can be found in the LICENSE file.

# This could be a good use for embed but go/doc/comment
# is built into the bootstrap go command, so it can't use embed.
# Also not using embed lets us emit a string array directly
# and avoid init-time work.

(
echo "// Copyright 2022 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

// Code generated by 'go generate' DO NOT EDIT.
//go:generate ./mkstd.sh

package comment

var stdPkgs = []string{"
go list std | grep -v / | sort | sed 's/.*/"&",/'
echo "}"
) | gofmt >std.go.tmp && mv std.go.tmp std.go
