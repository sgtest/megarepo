package repos

import (
	"context"
	"os"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/testutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGerritSource_ListRepos(t *testing.T) {
	cfName := t.Name()
	t.Run("no filtering", func(t *testing.T) {
		conf := &schema.GerritConnection{
			Url:      "https://gerrit.sgdev.org",
			Username: os.Getenv("GERRIT_USERNAME"),
			Password: os.Getenv("GERRIT_PASSWORD"),
		}
		cf, save := newClientFactory(t, cfName)
		defer save(t)

		svc := &types.ExternalService{
			Kind:   extsvc.KindGerrit,
			Config: extsvc.NewUnencryptedConfig(marshalJSON(t, conf)),
		}

		ctx := context.Background()
		src, err := NewGerritSource(ctx, svc, cf)
		require.NoError(t, err)

		src.perPage = 25

		repos, err := listAll(ctx, src)
		require.NoError(t, err)

		testutil.AssertGolden(t, "testdata/sources/GERRIT/"+t.Name(), update(t.Name()), repos)
	})

	t.Run("with filtering", func(t *testing.T) {
		conf := &schema.GerritConnection{
			Projects: []string{
				"src-cli",
			},
			Url:      "https://gerrit.sgdev.org",
			Username: os.Getenv("GERRIT_USERNAME"),
			Password: os.Getenv("GERRIT_PASSWORD"),
		}
		cf, save := newClientFactory(t, cfName)
		defer save(t)

		svc := &types.ExternalService{
			Kind:   extsvc.KindGerrit,
			Config: extsvc.NewUnencryptedConfig(marshalJSON(t, conf)),
		}

		ctx := context.Background()
		src, err := NewGerritSource(ctx, svc, cf)
		require.NoError(t, err)

		src.perPage = 25

		repos, err := listAll(ctx, src)
		require.NoError(t, err)

		assert.Len(t, repos, 1)
		repoNames := make([]string, 0, len(repos))
		for _, repo := range repos {
			repoNames = append(repoNames, repo.ExternalRepo.ID)
		}
		assert.ElementsMatch(t, repoNames, []string{
			"src-cli",
		})
	})
}
