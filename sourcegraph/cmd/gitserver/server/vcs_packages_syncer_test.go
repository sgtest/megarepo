package server

import (
	"bufio"
	"bytes"
	"context"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.org/x/exp/slices"

	"github.com/sourcegraph/sourcegraph/cmd/gitserver/server/common"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestVcsDependenciesSyncer_Fetch(t *testing.T) {
	ctx := context.Background()
	placeholder, _ := parseFakeDependency("sourcegraph/placeholder@0.0.0")

	depsSource := &fakeDepsSource{
		deps:          map[string]reposource.VersionedPackage{},
		download:      map[string]error{},
		downloadCount: map[string]int{},
	}
	depsService := &fakeDepsService{deps: map[reposource.PackageName]dependencies.PackageRepoReference{}}

	s := vcsPackagesSyncer{
		logger:      logtest.Scoped(t),
		typ:         "fake",
		scheme:      "fake",
		placeholder: placeholder,
		source:      depsSource,
		svc:         depsService,
	}

	remoteURL := &vcs.URL{URL: url.URL{Path: "fake/foo"}}

	dir := common.GitDir(t.TempDir())
	_, err := s.CloneCommand(ctx, remoteURL, string(dir))
	require.NoError(t, err)

	depsService.Add("foo@0.0.1")
	depsSource.Add("foo@0.0.1")

	t.Run("one version from service", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		require.NoError(t, err)

		s.assertRefs(t, dir, map[string]string{
			"refs/heads/latest":   "759dab7e4a7fc384522cb75519660cb0d6f6e49d",
			"refs/tags/v0.0.1":    "b47eb15deed08abc9d437c81f42c1635febaa218",
			"refs/tags/v0.0.1^{}": "759dab7e4a7fc384522cb75519660cb0d6f6e49d",
		})
		s.assertDownloadCounts(t, depsSource, map[string]int{"foo@0.0.1": 1})
	})

	s.configDeps = []string{"foo@0.0.2"}
	depsSource.Add("foo@0.0.2")
	allVersionsHaveRefs := map[string]string{
		"refs/heads/latest":   "6cff53ec57702e8eec10569a3d981dacbaee4ed3",
		"refs/tags/v0.0.1":    "b47eb15deed08abc9d437c81f42c1635febaa218",
		"refs/tags/v0.0.1^{}": "759dab7e4a7fc384522cb75519660cb0d6f6e49d",
		"refs/tags/v0.0.2":    "7e2e4506ef1f5cd97187917a67bfb7a310f78687",
		"refs/tags/v0.0.2^{}": "6cff53ec57702e8eec10569a3d981dacbaee4ed3",
	}
	oneVersionOneDownload := map[string]int{"foo@0.0.1": 1, "foo@0.0.2": 1}

	t.Run("two versions, service and config", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		require.NoError(t, err)

		s.assertRefs(t, dir, allVersionsHaveRefs)
		s.assertDownloadCounts(t, depsSource, oneVersionOneDownload)
	})

	depsSource.Delete("foo@0.0.2")

	t.Run("cached tag not re-downloaded (404 not found)", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		require.NoError(t, err)

		// v0.0.2 is still present in the git repo because we didn't send a second download request.
		s.assertRefs(t, dir, allVersionsHaveRefs)
		s.assertDownloadCounts(t, depsSource, oneVersionOneDownload)
	})

	depsSource.Add("foo@0.0.2")
	depsSource.download["foo@0.0.1"] = errors.New("401 unauthorized")

	t.Run("cached tag not re-downloaded (401 unauthorized)", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		// v0.0.1 is still present in the git repo because we didn't send a second download request.
		require.NoError(t, err)
		s.assertRefs(t, dir, allVersionsHaveRefs)
		s.assertDownloadCounts(t, depsSource, oneVersionOneDownload)
	})

	depsService.Delete("foo@0.0.1")
	onlyV2Refs := map[string]string{
		"refs/heads/latest":   "6cff53ec57702e8eec10569a3d981dacbaee4ed3",
		"refs/tags/v0.0.2":    "7e2e4506ef1f5cd97187917a67bfb7a310f78687",
		"refs/tags/v0.0.2^{}": "6cff53ec57702e8eec10569a3d981dacbaee4ed3",
	}

	t.Run("service version deleted", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		require.NoError(t, err)

		s.assertRefs(t, dir, onlyV2Refs)
		s.assertDownloadCounts(t, depsSource, oneVersionOneDownload)
	})

	s.configDeps = []string{}

	t.Run("all versions deleted", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		require.NoError(t, err)

		s.assertRefs(t, dir, map[string]string{})
		s.assertDownloadCounts(t, depsSource, oneVersionOneDownload)
	})

	depsService.Add("foo@0.0.1")
	depsSource.Add("foo@0.0.1")
	depsService.Add("foo@0.0.2")
	depsSource.Add("foo@0.0.2")
	t.Run("error aggregation", func(t *testing.T) {
		_, err := s.Fetch(ctx, remoteURL, dir, "")
		require.ErrorContains(t, err, "401 unauthorized")

		// The foo@0.0.1 tag was not created because of the 401 error.
		// The foo@0.0.2 tag was created despite the 401 error for foo@0.0.1
		s.assertRefs(t, dir, onlyV2Refs)

		// We re-downloaded both v0.0.1 and v0.0.2 since their git refs had been deleted.
		s.assertDownloadCounts(t, depsSource, map[string]int{"foo@0.0.1": 2, "foo@0.0.2": 2})
	})

	bothV2andV3Refs := map[string]string{
		// latest branch has been updated to point to 0.0.3 instead of 0.0.2
		"refs/heads/latest":   "c93e10f82d5d34341b2836202ebb6b0faa95fa71",
		"refs/tags/v0.0.2":    "7e2e4506ef1f5cd97187917a67bfb7a310f78687",
		"refs/tags/v0.0.2^{}": "6cff53ec57702e8eec10569a3d981dacbaee4ed3",
		"refs/tags/v0.0.3":    "ba94b95e16bf902e983ead70dc6ee0edd6b03a3b",
		"refs/tags/v0.0.3^{}": "c93e10f82d5d34341b2836202ebb6b0faa95fa71",
	}

	t.Run("lazy-sync version via revspec", func(t *testing.T) {
		// the v0.0.3 tag should be created on-demand through the revspec parameter
		// For context, see https://github.com/sourcegraph/sourcegraph/pull/38811
		_, err := s.Fetch(ctx, remoteURL, dir, "v0.0.3^0")
		require.ErrorContains(t, err, "401 unauthorized") // v0.0.1 is still erroring
		require.Equal(t, s.svc.(*fakeDepsService).upsertedDeps, []dependencies.MinimalPackageRepoRef{{
			Scheme:   fakeVersionedPackage{}.Scheme(),
			Name:     "foo",
			Versions: []dependencies.MinimalPackageRepoRefVersion{{Version: "0.0.3"}},
		}})
		s.assertRefs(t, dir, bothV2andV3Refs)
		// We triggered a single download for v0.0.3 since it was lazily requested.
		// We triggered a v0.0.1 download since it's still erroring.
		s.assertDownloadCounts(t, depsSource, map[string]int{"foo@0.0.1": 3, "foo@0.0.2": 2, "foo@0.0.3": 1})
	})

	depsSource.download["foo@0.0.4"] = errors.New("0.0.4 not found")
	s.svc.(*fakeDepsService).upsertedDeps = []dependencies.MinimalPackageRepoRef{}

	t.Run("lazy-sync error version via revspec", func(t *testing.T) {
		// the v0.0.4 tag cannot be created on-demand because it returns a "0.0.4 not found" error
		_, err := s.Fetch(ctx, remoteURL, dir, "v0.0.4^0")
		require.Nil(t, err)
		// // the 0.0.4 error is silently ignored, we only return the error for v0.0.1.
		// require.Equal(t, fmt.Sprint(err.Error()), "error pushing dependency {\"foo\" \"0.0.1\"}: 401 unauthorized")
		// the 0.0.4 dependency was not stored in the database because the download failed.
		require.Equal(t, s.svc.(*fakeDepsService).upsertedDeps, []dependencies.MinimalPackageRepoRef{})
		// git tags are unchanged, v0.0.2 and v0.0.3 are cached.
		s.assertRefs(t, dir, bothV2andV3Refs)
		// We triggered downloads only for v0.0.4.
		// No new downloads were triggered for cached or other errored versions.
		s.assertDownloadCounts(t, depsSource, map[string]int{"foo@0.0.1": 3, "foo@0.0.2": 2, "foo@0.0.3": 1, "foo@0.0.4": 1})
	})

	depsSource.download["org.springframework.boot:spring-boot:3.0"] = notFoundError{errors.New("Please contact Josh Long")}

	t.Run("trying to download non-existent Maven dependency", func(t *testing.T) {
		springBootDep, err := reposource.ParseMavenVersionedPackage("org.springframework.boot:spring-boot:3.0")
		if err != nil {
			t.Fatal("Cannot parse Maven dependency")
		}
		err = s.gitPushDependencyTag(ctx, string(dir), springBootDep)
		require.NotNil(t, err)
	})
}

type fakeDepsService struct {
	deps         map[reposource.PackageName]dependencies.PackageRepoReference
	upsertedDeps []dependencies.MinimalPackageRepoRef
}

func (s *fakeDepsService) InsertPackageRepoRefs(ctx context.Context, depsToAdd []dependencies.MinimalPackageRepoRef) (newRepos []dependencies.PackageRepoReference, newVersions []dependencies.PackageRepoRefVersion, _ error) {
	for i := range depsToAdd {
		depsToAdd[i].LastCheckedAt = nil
		for j := range depsToAdd[i].Versions {
			depsToAdd[i].Versions[j].LastCheckedAt = nil
		}
	}
	s.upsertedDeps = append(s.upsertedDeps, depsToAdd...)
	for _, depToAdd := range depsToAdd {
		if existingDep, exists := s.deps[depToAdd.Name]; exists {
			for _, version := range depToAdd.Versions {
				if !slices.ContainsFunc(existingDep.Versions, func(v dependencies.PackageRepoRefVersion) bool {
					return v.Version == version.Version
				}) {
					existingDep.Versions = append(existingDep.Versions, dependencies.PackageRepoRefVersion{
						PackageRefID: existingDep.ID,
						Version:      version.Version,
						Blocked:      version.Blocked,
					})
					s.deps[depToAdd.Name] = existingDep
					newVersions = append(newVersions, dependencies.PackageRepoRefVersion{
						Version: version.Version,
						Blocked: version.Blocked,
					})
				}
			}
		} else {
			versionsForDep := make([]dependencies.PackageRepoRefVersion, 0, len(depToAdd.Versions))
			for _, version := range depToAdd.Versions {
				versionsForDep = append(versionsForDep, dependencies.PackageRepoRefVersion{
					Version: version.Version,
					Blocked: version.Blocked,
				})
			}
			s.deps[depToAdd.Name] = dependencies.PackageRepoReference{
				Scheme:   depToAdd.Scheme,
				Name:     depToAdd.Name,
				Versions: versionsForDep,
			}
			newRepos = append(newRepos, dependencies.PackageRepoReference{
				Scheme:   depToAdd.Scheme,
				Name:     depToAdd.Name,
				Versions: versionsForDep,
			})
		}
	}
	return
}

func (s *fakeDepsService) IsPackageRepoVersionAllowed(ctx context.Context, scheme string, pkg reposource.PackageName, version string) (allowed bool, err error) {
	return true, nil
}

func (s *fakeDepsService) ListPackageRepoRefs(ctx context.Context, opts dependencies.ListDependencyReposOpts) ([]dependencies.PackageRepoReference, int, bool, error) {
	return []dependencies.PackageRepoReference{s.deps[opts.Name]}, 1, false, nil
}

func (s *fakeDepsService) Add(deps ...string) {
	for _, d := range deps {
		dep, _ := parseFakeDependency(d)
		name := dep.PackageSyntax()
		if d, ok := s.deps[name]; !ok {
			s.deps[name] = dependencies.PackageRepoReference{
				Scheme: dep.Scheme(),
				Name:   name,
				Versions: []dependencies.PackageRepoRefVersion{
					{Version: dep.PackageVersion()},
				},
			}
		} else {
			d.Versions = append(d.Versions, dependencies.PackageRepoRefVersion{Version: dep.PackageVersion()})
			s.deps[name] = d
		}
	}
}

func (s *fakeDepsService) Delete(deps ...string) {
	for _, d := range deps {
		depToDelete, _ := parseFakeDependency(d)
		name := depToDelete.PackageSyntax()
		version := depToDelete.PackageVersion()
		dep := s.deps[name]
		if idx := slices.IndexFunc(dep.Versions, func(v dependencies.PackageRepoRefVersion) bool {
			return v.Version == version
		}); idx > -1 {
			dep.Versions = slices.Delete(dep.Versions, idx, idx+1)
			s.deps[name] = dep
		}
	}
}

type fakeDepsSource struct {
	deps          map[string]reposource.VersionedPackage
	download      map[string]error
	downloadCount map[string]int
}

func (s *fakeDepsSource) Add(deps ...string) {
	for _, d := range deps {
		dep, _ := parseFakeDependency(d)
		s.deps[d] = dep
	}
}

func (s *fakeDepsSource) Delete(deps ...string) {
	for _, d := range deps {
		delete(s.deps, d)
	}
}

func (s *fakeDepsSource) Download(ctx context.Context, dir string, dep reposource.VersionedPackage) error {
	s.downloadCount[dep.VersionedPackageSyntax()] = 1 + s.downloadCount[dep.VersionedPackageSyntax()]

	err := s.download[dep.VersionedPackageSyntax()]
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, "README.md"), []byte("README for "+dep.VersionedPackageSyntax()), 0o666)
}

func (fakeDepsSource) ParseVersionedPackageFromNameAndVersion(name reposource.PackageName, version string) (reposource.VersionedPackage, error) {
	return parseFakeDependency(string(name) + "@" + version)
}

func (fakeDepsSource) ParseVersionedPackageFromConfiguration(dep string) (reposource.VersionedPackage, error) {
	return parseFakeDependency(dep)
}

func (fakeDepsSource) ParsePackageFromName(name reposource.PackageName) (reposource.Package, error) {
	return parseFakeDependency(string(name))
}

func (s *fakeDepsSource) ParsePackageFromRepoName(repoName api.RepoName) (reposource.Package, error) {
	return s.ParsePackageFromName(reposource.PackageName(strings.TrimPrefix(string(repoName), "fake/")))
}

type fakeVersionedPackage struct {
	name    reposource.PackageName
	version string
}

func parseFakeDependency(dep string) (reposource.VersionedPackage, error) {
	i := strings.LastIndex(dep, "@")
	if i == -1 {
		return fakeVersionedPackage{name: reposource.PackageName(dep)}, nil
	}
	return fakeVersionedPackage{name: reposource.PackageName(dep[:i]), version: dep[i+1:]}, nil
}

func (f fakeVersionedPackage) Scheme() string                        { return "fake" }
func (f fakeVersionedPackage) PackageSyntax() reposource.PackageName { return f.name }
func (f fakeVersionedPackage) VersionedPackageSyntax() string {
	return string(f.name) + "@" + f.version
}
func (f fakeVersionedPackage) PackageVersion() string    { return f.version }
func (f fakeVersionedPackage) Description() string       { return string(f.name) + "@" + f.version }
func (f fakeVersionedPackage) RepoName() api.RepoName    { return api.RepoName("fake/" + f.name) }
func (f fakeVersionedPackage) GitTagFromVersion() string { return "v" + f.version }
func (f fakeVersionedPackage) Less(other reposource.VersionedPackage) bool {
	return f.VersionedPackageSyntax() > other.VersionedPackageSyntax()
}

func (s *vcsPackagesSyncer) runCloneCommand(t *testing.T, examplePackageURL, bareGitDirectory string, dependencies []string) {
	u := vcs.URL{
		URL: url.URL{Path: examplePackageURL},
	}
	s.configDeps = dependencies
	cmd, err := s.CloneCommand(context.Background(), &u, bareGitDirectory)
	assert.Nil(t, err)
	assert.Nil(t, cmd.Run())
}

func (s *vcsPackagesSyncer) assertDownloadCounts(t *testing.T, depsSource *fakeDepsSource, want map[string]int) {
	t.Helper()

	require.Equal(t, want, depsSource.downloadCount)
}

func (s *vcsPackagesSyncer) assertRefs(t *testing.T, dir common.GitDir, want map[string]string) {
	t.Helper()

	cmd := exec.Command("git", "show-ref", "--head", "--dereference")
	cmd.Dir = string(dir)

	out, _ := cmd.CombinedOutput()

	sc := bufio.NewScanner(bytes.NewReader(out))
	have := map[string]string{}
	for sc.Scan() {
		fs := strings.Fields(sc.Text())
		have[fs[1]] = fs[0]
	}

	require.NoError(t, sc.Err())
	require.Equal(t, want, have)
}
