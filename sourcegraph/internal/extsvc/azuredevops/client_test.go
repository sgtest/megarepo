package azuredevops

import (
	"context"
	"flag"
	"net/http"
	"net/url"
	"path/filepath"
	"testing"

	"github.com/dnaeon/go-vcr/cassette"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/httptestutil"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/testutil"
	"github.com/sourcegraph/sourcegraph/schema"
)

var update = flag.Bool("update", false, "update testdata")

func TestClient_ListRepositoriesByProjectOrOrg(t *testing.T) {
	cli, save := NewTestClient(t, "ListRepositoriesByProjectOrOrg", *update)
	t.Cleanup(save)

	opts := ListRepositoriesByProjectOrOrgArgs{
		ProjectOrOrgName: "sgtestazure",
	}

	resp, err := cli.ListRepositoriesByProjectOrOrg(context.Background(), opts)
	if err != nil {
		t.Fatal(err)
	}

	testutil.AssertGolden(t, "testdata/golden/ListProjects.json", *update, resp)
}

func TestClient_AzureServicesProfile(t *testing.T) {
	cli, save := NewTestClient(t, "AzureServicesProfile", *update)
	t.Cleanup(save)

	resp, err := cli.AzureServicesProfile(context.Background())
	if err != nil {
		t.Fatal(err)
	}

	testutil.AssertGolden(t, "testdata/golden/AzureServicesConnectionData.json", *update, resp)
}

// NewTestClient returns an azuredevops.Client that records its interactions
// to testdata/vcr/.
func NewTestClient(t testing.TB, name string, update bool) (*Client, func()) {
	t.Helper()

	cassete := filepath.Join("testdata/vcr/", normalize(name))
	rec, err := httptestutil.NewRecorder(cassete, update)
	if err != nil {
		t.Fatal(err)
	}
	rec.SetMatcher(ignoreHostMatcher)

	hc, err := httpcli.NewFactory(nil, httptestutil.NewRecorderOpt(rec)).Doer()
	if err != nil {
		t.Fatal(err)
	}

	c := &schema.AzureDevOpsConnection{
		Url:      "https://dev.azure.com",
		Username: "testuser",
		Token:    "testtoken",
	}

	cli, err := NewClient("urn", c, hc)
	if err != nil {
		t.Fatal(err)
	}

	return cli, func() {
		if err := rec.Stop(); err != nil {
			t.Errorf("failed to update test data: %s", err)
		}
	}
}

var normalizer = lazyregexp.New("[^A-Za-z0-9-]+")

func normalize(path string) string {
	return normalizer.ReplaceAllLiteralString(path, "-")
}

func ignoreHostMatcher(r *http.Request, i cassette.Request) bool {
	if r.Method != i.Method {
		return false
	}
	u, err := url.Parse(i.URL)
	if err != nil {
		return false
	}
	u.Host = r.URL.Host
	u.Scheme = r.URL.Scheme
	return r.URL.String() == u.String()
}
