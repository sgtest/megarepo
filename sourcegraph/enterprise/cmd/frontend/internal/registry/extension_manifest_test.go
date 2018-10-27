package registry

import (
	"context"
	"encoding/json"
	"reflect"
	"testing"
)

func TestGetExtensionManifestWithBundleURL(t *testing.T) {
	resetMocks()
	ctx := context.Background()

	nilOrEmpty := func(s *string) string {
		if s == nil {
			return ""
		}
		return *s
	}

	t.Run(`manifest with "url"`, func(t *testing.T) {
		mocks.releases.GetLatest = func(registryExtensionID int32, releaseTag string, includeArtifacts bool) (*dbRelease, error) {
			return &dbRelease{
				Manifest: `{"name":"x","url":"u"}`,
			}, nil
		}
		defer func() { mocks.releases.GetLatest = nil }()
		manifest, err := getExtensionManifestWithBundleURL(ctx, "x", 1, "t")
		if err != nil {
			t.Fatal(err)
		}
		if want := `{"name":"x","url":"u"}`; manifest == nil || !jsonDeepEqual(*manifest, want) {
			t.Errorf("got %q, want %q", nilOrEmpty(manifest), want)
		}
	})

	t.Run(`manifest without "url"`, func(t *testing.T) {
		mocks.releases.GetLatest = func(registryExtensionID int32, releaseTag string, includeArtifacts bool) (*dbRelease, error) {
			return &dbRelease{
				Manifest: `{"name":"x"}`,
			}, nil
		}
		defer func() { mocks.releases.GetLatest = nil }()
		manifest, err := getExtensionManifestWithBundleURL(ctx, "x", 1, "t")
		if err != nil {
			t.Fatal(err)
		}
		if want := `{"name":"x","url":"/-/static/extension/0.js?x---1fmlvpbbdw2yo"}`; manifest == nil || !jsonDeepEqual(*manifest, want) {
			t.Errorf("got %q, want %q", nilOrEmpty(manifest), want)
		}
	})
}

func jsonDeepEqual(a, b string) bool {
	var va, vb interface{}
	if err := json.Unmarshal([]byte(a), &va); err != nil {
		panic(err)
	}
	if err := json.Unmarshal([]byte(b), &vb); err != nil {
		panic(err)
	}
	return reflect.DeepEqual(va, vb)
}
