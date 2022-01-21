package repos

import (
	"context"
	"database/sql"
	"os"
	"sort"
	"testing"

	"github.com/keegancsmith/sqlf"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/npmpackages/npmtest"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestGetNPMDependencyRepos(t *testing.T) {
	_, store, ctx, _ := setupDependenciesInDB(t)

	type testCase struct {
		pkgName string
		matches []string
	}

	testCases := []testCase{
		{"pkg1", []string{"pkg1@1", "pkg1@2", "pkg1@3"}},
		{"pkg2", []string{"pkg2@1", "pkg2@0.1-abc"}},
		{"@scope/pkg1", []string{"@scope/pkg1@1"}},
		{"missing", []string{}},
	}

	for _, testCase := range testCases {
		deps, err := store.GetNPMDependencyRepos(ctx, dbstore.GetNPMDependencyReposOpts{
			ArtifactName: testCase.pkgName,
		})
		require.Nil(t, err)
		depStrs := []string{}
		for _, dep := range deps {
			pkg, err := reposource.ParseNPMPackageFromPackageSyntax(dep.Package)
			require.Nil(t, err)
			depStrs = append(depStrs,
				reposource.NPMDependency{*pkg, dep.Version}.PackageManagerSyntax(),
			)
		}
		sort.Strings(depStrs)
		sort.Strings(testCase.matches)
		require.Equal(t, testCase.matches, depStrs)
	}

	for _, testCase := range testCases {
		depStrs := []string{}
		lastID := 0
		for i := 0; i < len(testCase.matches); i++ {
			deps, err := store.GetNPMDependencyRepos(ctx, dbstore.GetNPMDependencyReposOpts{
				ArtifactName: testCase.pkgName,
				After:        lastID,
				Limit:        1,
			})
			require.Nil(t, err)
			require.Equal(t, len(deps), 1)
			pkg, err := reposource.ParseNPMPackageFromPackageSyntax(deps[0].Package)
			require.Nil(t, err)
			depStrs = append(depStrs, reposource.NPMDependency{*pkg, deps[0].Version}.PackageManagerSyntax())
			lastID = deps[0].ID
		}
		sort.Strings(depStrs)
		sort.Strings(testCase.matches)
		require.Equal(t, testCase.matches, depStrs)
	}
}

func setupDependenciesInDB(t *testing.T) (*sql.DB, *dbstore.Store, context.Context, []string) {
	t.Helper()
	db := dbtest.NewDB(t)
	store := dbstore.NewWithDB(db, &observation.TestContext, nil)
	ctx := context.Background()

	dependencies := []string{
		"pkg1@1",
		"pkg1@2",
		"pkg2@1",
		"@scope/pkg1@1",
		"pkg1@3",
		"pkg2@0.1-abc",
	}
	insertDependencies(t, ctx, store, dependencies)
	return db, store, ctx, dependencies
}

func TestListRepos(t *testing.T) {
	db, _, ctx, dependencies := setupDependenciesInDB(t)
	sort.Strings(dependencies)

	dir, err := os.MkdirTemp("", "")
	require.Nil(t, err)
	defer os.RemoveAll(dir)

	svc := types.ExternalService{
		Kind:   extsvc.KindNPMPackages,
		Config: `{"registry": "https://placeholder.lol", "rateLimit": {"enabled": false}}`,
	}
	packageSource, err := NewNPMPackagesSource(&svc)
	require.Nil(t, err)
	packageSource.SetDB(db)
	packageSource.client = &npmtest.MockClient{
		TarballMap: func() map[string]string {
			m := map[string]string{}
			for _, dep := range dependencies {
				m[dep] = ""
			}
			return m
		}(),
	}
	results := make(chan SourceResult, 10)
	go func() {
		packageSource.ListRepos(ctx, results)
		close(results)
	}()
	repoURLs := []string{}
	for val := range results {
		require.NotNil(t, val.Repo)
		repoURLs = append(repoURLs, string(val.Repo.Name))
	}
	sort.Strings(repoURLs)
	expectedRepoURLs := []string{}
	for _, dep := range dependencies {
		dep, err := reposource.ParseNPMDependency(dep)
		require.Nil(t, err)
		expectedRepoURLs = append(expectedRepoURLs, string(dep.Package.RepoName()))
	}
	sort.Strings(expectedRepoURLs)
	// Compare after uniquing after addressing [FIXME: deduplicate-listed-repos].
	require.Equal(t, expectedRepoURLs, repoURLs)
}

func insertDependencies(t *testing.T, ctx context.Context, s *dbstore.Store, dependencies []string) {
	for _, depStr := range dependencies {
		dep, err := reposource.ParseNPMDependency(depStr)
		require.Nil(t, err)
		// See also: enterprise/internal/codeintel/stores/dbstore/dependency_index.go:InsertCloneableDependencyRepo
		rows, err :=
			s.Store.Query(ctx, sqlf.Sprintf(
				`INSERT INTO lsif_dependency_repos (scheme, name, version) VALUES (%s, %s, %s)`,
				dbstore.NPMPackagesScheme, dep.Package.PackageSyntax(), dep.Version))
		require.Nil(t, err)
		for rows.Next() {
		}
		require.Nil(t, rows.Err())
	}
}
