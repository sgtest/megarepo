package repos

import (
	"context"
	"testing"

	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/internal/testutil"

	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestPagureSource_ListRepos(t *testing.T) {
	conf := &schema.PagureConnection{
		Url:     "https://src.fedoraproject.org",
		Pattern: "ac*",
	}
	cf, save := newClientFactory(t, t.Name())
	defer save(t)

	lg := log15.New()
	lg.SetHandler(log15.DiscardHandler())

	svc := &types.ExternalService{
		Kind:   extsvc.KindPagure,
		Config: marshalJSON(t, conf),
	}

	src, err := NewPagureSource(svc, cf)
	if err != nil {
		t.Fatal(err)
	}

	src.perPage = 25 // 2 pages for 47 results

	repos, err := listAll(context.Background(), src)
	if err != nil {
		t.Fatal(err)
	}

	testutil.AssertGolden(t, "testdata/sources/"+t.Name(), update(t.Name()), repos)
}
