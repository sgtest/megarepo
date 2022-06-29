package backend

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
)

var Mocks MockServices

type MockServices struct {
	Repos MockRepos
}

// testContext creates a new context.Context for use by tests
func testContext() context.Context {
	Mocks = MockServices{}
	gitserver.ResetMocks()

	ctx := context.Background()
	ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
	_, ctx = ot.StartSpanFromContext(ctx, "dummy")

	return ctx
}
