package cody

import (
	"context"
	"testing"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestIsCodyEnabled(t *testing.T) {
	oldMock := licensing.MockCheckFeature
	licensing.MockCheckFeature = func(feature licensing.Feature) error {
		// App doesn't have a proper license so always return an error when checking
		if deploy.IsApp() {
			return errors.New("Mock check feature error")
		}
		return nil
	}
	t.Cleanup(func() {
		licensing.MockCheckFeature = oldMock
	})

	t.Run("Unauthenticated user", func(t *testing.T) {
		conf.Mock(&conf.Unified{
			SiteConfiguration: schema.SiteConfiguration{
				Completions: &schema.Completions{
					Enabled: true,
				},
			},
		})
		t.Cleanup(func() {
			conf.Mock(nil)
		})
		ctx := context.Background()
		ctx = actor.WithActor(ctx, &actor.Actor{UID: 0})
		if IsCodyEnabled(ctx) {
			t.Error("Expected IsCodyEnabled to return false for unauthenticated actor")
		}
	})

	t.Run("Authenticated user", func(t *testing.T) {
		conf.Mock(&conf.Unified{
			SiteConfiguration: schema.SiteConfiguration{
				Completions: &schema.Completions{
					Enabled: true,
				},
			},
		})
		t.Cleanup(func() {
			conf.Mock(nil)
		})
		ctx := context.Background()
		ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
		if !IsCodyEnabled(ctx) {
			t.Error("Expected IsCodyEnabled to return true for authenticated actor")
		}
	})
	t.Run("Disabled completions", func(t *testing.T) {
		conf.Mock(&conf.Unified{
			SiteConfiguration: schema.SiteConfiguration{
				Completions: &schema.Completions{
					Enabled: false,
				},
			},
		})
		t.Cleanup(func() {
			conf.Mock(nil)
		})
		ctx := context.Background()
		ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
		if IsCodyEnabled(ctx) {
			t.Error("Expected IsCodyEnabled to return false when completions are disabled")
		}
	})

	t.Run("No completions config", func(t *testing.T) {
		conf.Mock(&conf.Unified{
			SiteConfiguration: schema.SiteConfiguration{},
		})
		t.Cleanup(func() {
			conf.Mock(nil)
		})
		ctx := context.Background()
		ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
		if IsCodyEnabled(ctx) {
			t.Error("Expected IsCodyEnabled to return false when completions are not configured")
		}
	})

	t.Run("CodyRestrictUsersFeatureFlag", func(t *testing.T) {
		t.Run("feature flag disabled", func(t *testing.T) {
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					ExperimentalFeatures: &schema.ExperimentalFeatures{
						CodyRestrictUsersFeatureFlag: true,
					},
					Completions: &schema.Completions{
						Enabled: true,
					},
				},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
			})

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 0})
			if IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return false for unauthenticated user with CodyRestrictUsersFeatureFlag enabled")
			}
			ctx = context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return false for authenticated user when CodyRestrictUsersFeatureFlag is set and no feature flag is present for the user")
			}
		})
		t.Run("feature flag enabled", func(t *testing.T) {
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					ExperimentalFeatures: &schema.ExperimentalFeatures{
						CodyRestrictUsersFeatureFlag: true,
					},
					Completions: &schema.Completions{
						Enabled: true,
					},
				},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
			})

			ctx := context.Background()
			ctx = featureflag.WithFlags(ctx, featureflag.NewMemoryStore(map[string]bool{"cody-experimental": true}, map[string]bool{"cody-experimental": true}, nil))
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 0})
			if IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return false when cody-experimental feature flag is enabled")
			}
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if !IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return true when cody-experimental feature flag is enabled")
			}
		})
	})

	t.Run("CodyEnabledInApp", func(t *testing.T) {
		t.Run("Completions configured", func(t *testing.T) {
			deploy.Mock(deploy.App)
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					Completions: &schema.Completions{
						Enabled: true,
					},
				},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
				deploy.Mock(deploy.Kubernetes)
			})

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if !IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return true in App when completions are configured")
			}
		})

		t.Run("Dotcom Token present", func(t *testing.T) {
			deploy.Mock(deploy.App)
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					App: &schema.App{
						DotcomAuthToken: "TOKEN",
					},
				},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
				deploy.Mock(deploy.Kubernetes)
			})

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if !IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return true in App when dotcom token is present")
			}
		})

		t.Run("No configuration", func(t *testing.T) {
			deploy.Mock(deploy.App)
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
				deploy.Mock(deploy.Kubernetes)
			})

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return false in App when no dotcom token or completions configuration is present")
			}
		})

		t.Run("Disabled completions", func(t *testing.T) {
			deploy.Mock(deploy.App)
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					Completions: &schema.Completions{
						Enabled: false,
					},
				},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
				deploy.Mock(deploy.Kubernetes)
			})

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return false in App completions configuration and disabled")
			}
		})

		t.Run("Empty dotcom token", func(t *testing.T) {
			deploy.Mock(deploy.App)
			conf.Mock(&conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					App: &schema.App{
						DotcomAuthToken: "",
					},
				},
			})
			t.Cleanup(func() {
				conf.Mock(nil)
				deploy.Mock(deploy.Kubernetes)
			})

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})
			if IsCodyEnabled(ctx) {
				t.Error("Expected IsCodyEnabled to return false in App when no dotcom token is present")
			}
		})
	})
}
