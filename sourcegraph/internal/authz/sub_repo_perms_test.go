package authz

import (
	"context"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
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
