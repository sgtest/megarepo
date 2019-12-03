package httpapi

import (
	"context"
	"reflect"
	"strconv"
	"testing"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
)

func TestRepoShieldFmt(t *testing.T) {
	want := map[int]string{
		50:    " 50 projects",
		100:   " 100 projects",
		1000:  " 1.0k projects",
		1001:  " 1.0k projects",
		1500:  " 1.5k projects",
		15410: " 15.4k projects",
	}
	for input, want := range want {
		t.Run(strconv.Itoa(input), func(t *testing.T) {
			got := badgeValueFmt(input)
			if got != want {
				t.Fatalf("input %d got %q want %q", input, got, want)
			}
		})
	}
}

func TestRepoShield(t *testing.T) {
	c := newTest()

	wantResp := map[string]interface{}{
		"value": " 200 projects",
	}

	backend.Mocks.Repos.GetByName = func(ctx context.Context, name api.RepoName) (*types.Repo, error) {
		switch name {
		case "github.com/gorilla/mux":
			return &types.Repo{ID: 2, Name: name}, nil
		default:
			panic("wrong path")
		}
	}
	backend.Mocks.Repos.ResolveRev = func(ctx context.Context, repo *types.Repo, rev string) (api.CommitID, error) {
		if repo.ID != 2 || rev != "master" {
			t.Error("wrong arguments to ResolveRev")
		}
		return "aed", nil
	}
	backend.MockCountGoImporters = func(ctx context.Context, source api.RepoName) (int, error) {
		if source != "github.com/gorilla/mux" {
			t.Error("wrong repo source to TotalRefs")
		}
		return 200, nil
	}

	var resp map[string]interface{}
	if err := c.GetJSON("/repos/github.com/gorilla/mux/-/shield", &resp); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(resp, wantResp) {
		t.Errorf("got %+v, want %+v", resp, wantResp)
	}
}
