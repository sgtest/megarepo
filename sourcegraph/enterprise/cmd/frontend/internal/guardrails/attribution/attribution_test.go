package attribution

import (
	"context"
	"fmt"
	"testing"

	"github.com/Khan/genqlient/graphql"
	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/log/logtest"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/guardrails/dotcom"
	"github.com/sourcegraph/sourcegraph/internal/database"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/client"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/zoekt"
)

func TestAttribution(t *testing.T) {
	ctx := context.Background()

	// inputs
	localCount, dotcomCount := 5, 5
	limit := localCount + dotcomCount + 1
	localNames := genRepoNames("localrepo-", localCount)
	dotcomNames := genRepoNames("dotcomrepo-", dotcomCount)

	// we want the localNames back followed by dotcomNames
	wantCount := localCount + dotcomCount
	wantNames := append(genRepoNames("localrepo-", localCount), genRepoNames("dotcomrepo-", dotcomCount)...)

	svc := &Service{
		SearchClient:              mockSearchClient(t, localNames),
		SourcegraphDotComClient:   mockDotComClient(t, dotcomNames),
		SourcegraphDotComFederate: true,
	}

	result, err := svc.SnippetAttribution(ctx, "test", limit)
	if err != nil {
		t.Fatal(err)
	}

	want := &SnippetAttributions{
		TotalCount:      wantCount,
		LimitHit:        false,
		RepositoryNames: wantNames,
	}
	if d := cmp.Diff(want, result); d != "" {
		t.Fatalf("unexpected (-want, +got):\n%s", d)
	}
}

func genRepoNames(prefix string, count int) []string {
	var names []string
	for i := 1; i <= count; i++ {
		names = append(names, fmt.Sprintf("%s%d", prefix, i))
	}
	return names
}

// mockSearchClient returns a client which will return matches. This exercises
// more of the search code path to give a bit more confidence we are correctly
// calling Plan and Execute vs a dumb SearchClient mock.
func mockSearchClient(t testing.TB, repoNames []string) client.SearchClient {
	repos := database.NewMockRepoStore()
	repos.ListMinimalReposFunc.SetDefaultReturn([]types.MinimalRepo{}, nil)
	repos.CountFunc.SetDefaultReturn(0, nil)

	db := database.NewMockDB()
	db.ReposFunc.SetDefaultReturn(repos)

	var matches []zoekt.FileMatch
	for i, name := range repoNames {
		matches = append(matches, zoekt.FileMatch{
			RepositoryID: uint32(i),
			Repository:   name,
		})
	}
	mockZoekt := &searchbackend.FakeStreamer{
		Repos: []*zoekt.RepoListEntry{},
		Results: []*zoekt.SearchResult{{
			Files: matches,
		}},
	}

	return client.MockedZoekt(logtest.Scoped(t), db, mockZoekt)
}

func mockDotComClient(t testing.TB, repoNames []string) dotcom.Client {
	return makeRequester(func(ctx context.Context, req *graphql.Request, resp *graphql.Response) error {
		// :O :O generated type names :O :O
		var nodes []dotcom.SnippetAttributionSnippetAttributionSnippetAttributionConnectionNodesSnippetAttribution
		for _, name := range repoNames {
			nodes = append(nodes, dotcom.SnippetAttributionSnippetAttributionSnippetAttributionConnectionNodesSnippetAttribution{
				RepositoryName: name,
			})
		}

		data := resp.Data.(*dotcom.SnippetAttributionResponse)
		*data = dotcom.SnippetAttributionResponse{
			// :O
			SnippetAttribution: dotcom.SnippetAttributionSnippetAttributionSnippetAttributionConnection{
				TotalCount: len(repoNames),
				Nodes:      nodes,
			},
		}

		return nil
	})
}

type makeRequester func(ctx context.Context, req *graphql.Request, resp *graphql.Response) error

func (f makeRequester) MakeRequest(ctx context.Context, req *graphql.Request, resp *graphql.Response) error {
	return f(ctx, req, resp)
}
