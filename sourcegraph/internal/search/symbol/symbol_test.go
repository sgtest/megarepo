package symbol

import (
	"context"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	srp "github.com/sourcegraph/sourcegraph/internal/authz/subrepoperms"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
	"github.com/stretchr/testify/assert"
)

func TestSearchZoektDoesntPanicWithNilQuery(t *testing.T) {
	// As soon as we reach Streamer.Search function, we can consider test successful,
	// that's why we can just mock it.
	mockStreamer := NewMockStreamer()
	expectedErr := errors.New("short circuit")
	mockStreamer.SearchFunc.SetDefaultReturn(nil, expectedErr)
	search.IndexedMock = mockStreamer
	t.Cleanup(func() {
		search.IndexedMock = nil
	})

	_, err := searchZoekt(context.Background(), types.MinimalRepo{ID: 1}, "commitID", nil, "branch", nil, nil, nil)
	assert.ErrorIs(t, err, expectedErr)
}

func TestFilterZoektResults(t *testing.T) {
	conf.Mock(&conf.Unified{
		SiteConfiguration: schema.SiteConfiguration{
			ExperimentalFeatures: &schema.ExperimentalFeatures{
				SubRepoPermissions: &schema.SubRepoPermissions{
					Enabled: true,
				},
			},
		},
	})
	t.Cleanup(func() { conf.Mock(nil) })

	repoName := api.RepoName("foo")
	ctx := context.Background()
	ctx = actor.WithActor(ctx, &actor.Actor{
		UID: 1,
	})
	checker, err := srp.NewSimpleChecker(repoName, []string{"/**", "-/*_test.go"})
	if err != nil {
		t.Fatal(err)
	}
	results := []*result.SymbolMatch{
		{
			Symbol: result.Symbol{},
			File: &result.File{
				Path: "foo.go",
			},
		},
		{
			Symbol: result.Symbol{},
			File: &result.File{
				Path: "foo_test.go",
			},
		},
	}
	filtered, err := FilterZoektResults(ctx, checker, repoName, results)
	if err != nil {
		t.Fatal(err)
	}
	assert.Len(t, filtered, 1)
	r := filtered[0]
	assert.Equal(t, r.File.Path, "foo.go")
}
