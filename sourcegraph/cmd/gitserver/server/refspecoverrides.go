package server

import (
	"context"
	"net/url"
	"os/exec"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/env"
)

// HACK(keegancsmith) workaround to experiment with cloning less in a large
// monorepo. https://github.com/sourcegraph/customer/issues/19
var refspecOverrides = strings.Fields(env.Get("SRC_GITSERVER_REFSPECS", "", "EXPERIMENTAL: override refspec we fetch. Space separated."))

// HACK(keegancsmith) workaround to experiment with cloning less in a large
// monorepo. https://github.com/sourcegraph/customer/issues/19
func useRefspecOverrides() bool {
	return len(refspecOverrides) > 0
}

// HACK(keegancsmith) workaround to experiment with cloning less in a large
// monorepo. https://github.com/sourcegraph/customer/issues/19
func refspecOverridesFetchCmd(ctx context.Context, remoteURL *url.URL) *exec.Cmd {
	return exec.CommandContext(ctx, "git", append([]string{"fetch", "--progress", "--prune", remoteURL.String()}, refspecOverrides...)...)
}
