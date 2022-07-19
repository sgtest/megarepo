package authz

import (
	"context"
	"io/fs"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/fileutil"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSubRepoPermsPermissions(t *testing.T) {
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

	testCases := []struct {
		name     string
		userID   int32
		content  RepoContent
		clientFn func() (*SubRepoPermsClient, error)
		want     Perms
	}{
		{
			name:   "Empty path",
			userID: 1,
			content: RepoContent{
				Repo: "sample",
				Path: "",
			},
			clientFn: func() (*SubRepoPermsClient, error) {
				return NewSubRepoPermsClient(NewMockSubRepoPermissionsGetter())
			},
			want: Read,
		},
		{
			name:   "No rules",
			userID: 1,
			content: RepoContent{
				Repo: "sample",
				Path: "/dev/thing",
			},
			clientFn: func() (*SubRepoPermsClient, error) {
				getter := NewMockSubRepoPermissionsGetter()
				getter.GetByUserFunc.SetDefaultHook(func(ctx context.Context, i int32) (map[api.RepoName]SubRepoPermissions, error) {
					return map[api.RepoName]SubRepoPermissions{
						"sample": {
							PathIncludes: []string{},
							PathExcludes: []string{},
						},
					}, nil
				})
				return NewSubRepoPermsClient(getter)
			},
			want: None,
		},
		{
			name:   "Exclude",
			userID: 1,
			content: RepoContent{
				Repo: "sample",
				Path: "/dev/thing",
			},
			clientFn: func() (*SubRepoPermsClient, error) {
				getter := NewMockSubRepoPermissionsGetter()
				getter.GetByUserFunc.SetDefaultHook(func(ctx context.Context, i int32) (map[api.RepoName]SubRepoPermissions, error) {
					return map[api.RepoName]SubRepoPermissions{
						"sample": {
							PathIncludes: []string{},
							PathExcludes: []string{"/dev/*"},
						},
					}, nil
				})
				return NewSubRepoPermsClient(getter)
			},
			want: None,
		},
		{
			name:   "Include",
			userID: 1,
			content: RepoContent{
				Repo: "sample",
				Path: "/dev/thing",
			},
			clientFn: func() (*SubRepoPermsClient, error) {
				getter := NewMockSubRepoPermissionsGetter()
				getter.GetByUserFunc.SetDefaultHook(func(ctx context.Context, i int32) (map[api.RepoName]SubRepoPermissions, error) {
					return map[api.RepoName]SubRepoPermissions{
						"sample": {
							PathIncludes: []string{"*"},
						},
					}, nil
				})
				return NewSubRepoPermsClient(getter)
			},
			want: None,
		},
		{
			name:   "Exclude takes precedence",
			userID: 1,
			content: RepoContent{
				Repo: "sample",
				Path: "/dev/thing",
			},
			clientFn: func() (*SubRepoPermsClient, error) {
				getter := NewMockSubRepoPermissionsGetter()
				getter.GetByUserFunc.SetDefaultHook(func(ctx context.Context, i int32) (map[api.RepoName]SubRepoPermissions, error) {
					return map[api.RepoName]SubRepoPermissions{
						"sample": {
							PathIncludes: []string{"*"},
							PathExcludes: []string{"/dev/*"},
						},
					}, nil
				})
				return NewSubRepoPermsClient(getter)
			},
			want: None,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			client, err := tc.clientFn()
			if err != nil {
				t.Fatal(err)
			}
			have, err := client.Permissions(context.Background(), tc.userID, tc.content)
			if err != nil {
				t.Fatal(err)
			}
			if have != tc.want {
				t.Fatalf("have %v, want %v", have, tc.want)
			}
		})
	}
}

func TestFilterActorPaths(t *testing.T) {
	testPaths := []string{"file1", "file2", "file3"}
	checker := NewMockSubRepoPermissionChecker()
	ctx := context.Background()
	a := &actor.Actor{
		UID: 1,
	}
	ctx = actor.WithActor(ctx, a)
	repo := api.RepoName("foo")

	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content RepoContent) (Perms, error) {
		if content.Path == "file1" {
			return Read, nil
		}
		return None, nil
	})

	filtered, err := FilterActorPaths(ctx, checker, a, repo, testPaths)
	if err != nil {
		t.Fatal(err)
	}

	want := []string{"file1"}
	if diff := cmp.Diff(want, filtered); diff != "" {
		t.Fatal(diff)
	}
}

func TestCanReadAllPaths(t *testing.T) {
	testPaths := []string{"file1", "file2", "file3"}
	checker := NewMockSubRepoPermissionChecker()
	ctx := context.Background()
	a := &actor.Actor{
		UID: 1,
	}
	ctx = actor.WithActor(ctx, a)
	repo := api.RepoName("foo")

	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content RepoContent) (Perms, error) {
		switch content.Path {
		case "file1", "file2", "file3":
			return Read, nil
		default:
			return None, nil
		}
	})

	ok, err := CanReadAllPaths(ctx, checker, repo, testPaths)
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("Should be allowed to read all paths")
	}
	ok, err = CanReadAnyPath(ctx, checker, repo, testPaths)
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("CanReadyAnyPath should've returned true since the user can read all paths")
	}

	// Add path we can't read
	testPaths = append(testPaths, "file4")

	ok, err = CanReadAllPaths(ctx, checker, repo, testPaths)
	if err != nil {
		t.Fatal(err)
	}
	if ok {
		t.Fatal("Should fail, not allowed to read file4")
	}
	ok, err = CanReadAnyPath(ctx, checker, repo, testPaths)
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("user can read some of the testPaths, so CanReadAnyPath should return true")
	}
}

func TestSubRepoPermissionsCanReadDirectoriesInPath(t *testing.T) {
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
	repoName := api.RepoName("repo")

	testCases := []struct {
		pathIncludes  []string
		canReadAll    []string
		cannotReadAny []string
	}{
		{
			pathIncludes:  []string{"foo/bar/thing.txt"},
			canReadAll:    []string{"foo/", "foo/bar/"},
			cannotReadAny: []string{"foo/thing.txt", "foo/bar/other.txt"},
		},
		{
			pathIncludes: []string{"foo/bar/**"},
			canReadAll:   []string{"foo/", "foo/bar/", "foo/bar/baz/", "foo/bar/baz/fox/"},
		},
		{
			pathIncludes:  []string{"foo/bar/"},
			canReadAll:    []string{"foo/", "foo/bar/"},
			cannotReadAny: []string{"foo/thing.txt", "foo/bar/thing.txt"},
		},
		{
			pathIncludes:  []string{"baz/*/foo/bar/thing.txt"},
			canReadAll:    []string{"baz/", "baz/x/", "baz/x/foo/bar/"},
			cannotReadAny: []string{"baz/thing.txt"},
		},
		// We can't support rules that start with a wildcard, see comment in expandDirs
		{
			pathIncludes:  []string{"**/foo/bar/thing.txt"},
			cannotReadAny: []string{"foo/", "foo/bar/"},
		},
		{
			pathIncludes:  []string{"*/foo/bar/thing.txt"},
			cannotReadAny: []string{"foo/", "foo/bar/"},
		},
	}

	for _, tc := range testCases {
		t.Run("", func(t *testing.T) {
			getter := NewMockSubRepoPermissionsGetter()
			getter.GetByUserFunc.SetDefaultHook(func(ctx context.Context, i int32) (map[api.RepoName]SubRepoPermissions, error) {
				return map[api.RepoName]SubRepoPermissions{
					repoName: {
						PathIncludes: tc.pathIncludes,
					},
				}, nil
			})
			client, err := NewSubRepoPermsClient(getter)
			if err != nil {
				t.Fatal(err)
			}

			ctx := context.Background()

			for _, path := range tc.canReadAll {
				content := RepoContent{
					Repo: repoName,
					Path: path,
				}
				perm, err := client.Permissions(ctx, 1, content)
				if err != nil {
					t.Error(err)
				}
				if !perm.Include(Read) {
					t.Errorf("Should be able to read %q, cannot", path)
				}
			}

			for _, path := range tc.cannotReadAny {
				content := RepoContent{
					Repo: repoName,
					Path: path,
				}
				perm, err := client.Permissions(ctx, 1, content)
				if err != nil {
					t.Error(err)
				}
				if perm.Include(Read) {
					t.Errorf("Should not be able to read %q, can", path)
				}
			}
		})
	}
}

func TestSubRepoPermsPermissionsCache(t *testing.T) {
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

	getter := NewMockSubRepoPermissionsGetter()
	client, err := NewSubRepoPermsClient(getter)
	if err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()
	content := RepoContent{
		Repo: api.RepoName("thing"),
		Path: "/stuff",
	}

	// Should hit DB only once
	for i := 0; i < 3; i++ {
		_, err = client.Permissions(ctx, 1, content)
		if err != nil {
			t.Fatal(err)
		}

		h := getter.GetByUserFunc.History()
		if len(h) != 1 {
			t.Fatal("Should have been called once")
		}
	}

	// Trigger expiry
	client.since = func(time time.Time) time.Duration {
		return defaultCacheTTL + 1
	}

	_, err = client.Permissions(ctx, 1, content)
	if err != nil {
		t.Fatal(err)
	}

	h := getter.GetByUserFunc.History()
	if len(h) != 2 {
		t.Fatal("Should have been called twice")
	}
}

func TestSubRepoEnabled(t *testing.T) {
	t.Run("checker is nil", func(t *testing.T) {
		if SubRepoEnabled(nil) {
			t.Errorf("expected checker to be invalid since it is nil")
		}
	})
	t.Run("checker is not enabled", func(t *testing.T) {
		checker := NewMockSubRepoPermissionChecker()
		checker.EnabledFunc.SetDefaultHook(func() bool {
			return false
		})
		if SubRepoEnabled(checker) {
			t.Errorf("expected checker to be invalid since it is disabled")
		}
	})
	t.Run("checker is enabled", func(t *testing.T) {
		checker := NewMockSubRepoPermissionChecker()
		checker.EnabledFunc.SetDefaultHook(func() bool {
			return true
		})
		if !SubRepoEnabled(checker) {
			t.Errorf("expected checker to be valid since it is enabled")
		}
	})
}

func TestRepoContentFromFileInfo(t *testing.T) {
	repo := api.RepoName("my-repo")
	t.Run("adding trailing slash to directory", func(t *testing.T) {
		fi := &fileutil.FileInfo{
			Name_: "app",
			Mode_: fs.ModeDir,
		}
		rc := repoContentFromFileInfo(repo, fi)
		expected := RepoContent{
			Repo: repo,
			Path: "app/",
		}
		assert.Equal(t, expected, rc)
	})
	t.Run("doesn't add trailing slash if not directory", func(t *testing.T) {
		fi := &fileutil.FileInfo{
			Name_: "my-file.txt",
		}
		rc := repoContentFromFileInfo(repo, fi)
		expected := RepoContent{
			Repo: repo,
			Path: "my-file.txt",
		}
		assert.Equal(t, expected, rc)
	})
}
