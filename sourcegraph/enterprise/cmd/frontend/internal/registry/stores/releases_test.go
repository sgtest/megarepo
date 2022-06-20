package stores

import (
	"context"
	"reflect"
	"testing"
	"time"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
)

func TestRegistryExtensionReleases(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	user, err := db.Users().Create(ctx, database.NewUser{Username: "u"})
	if err != nil {
		t.Fatal(err)
	}
	extensions := Extensions(db)
	xExtensionID, err := extensions.Create(ctx, user.ID, 0, "x")
	if err != nil {
		t.Fatal(err)
	}
	yExtensionID, err := extensions.Create(ctx, user.ID, 0, "y")
	if err != nil {
		t.Fatal(err)
	}

	norm := func(r *Release) {
		r.CreatedAt = time.Time{}
	}

	s := Releases(db)

	t.Run("GetLatest with no releases", func(t *testing.T) {
		_, err := s.GetLatest(ctx, xExtensionID, "release", false)
		if !errcode.IsNotFound(err) {
			t.Errorf("got err %v, want errcode.IsNotFound", err)
		}
	})

	t.Run("GetLatest with nonexistent registry extension and no releases", func(t *testing.T) {
		_, err := s.GetLatest(ctx, 9999 /* doesn't exist */, "release", false)
		if !errcode.IsNotFound(err) {
			t.Errorf("got err %v, want errcode.IsNotFound", err)
		}
	})

	t.Run("GetArtifacts with no release", func(t *testing.T) {
		_, _, err := s.GetArtifacts(ctx, 9999 /* doesn't exist */)
		if !errcode.IsNotFound(err) {
			t.Errorf("got err %v, want errcode.IsNotFound", err)
		}
	})

	t.Run("Create", func(t *testing.T) {
		input := Release{
			RegistryExtensionID: xExtensionID,
			CreatorUserID:       user.ID,
			ReleaseTag:          "release",
			Manifest:            `{"m": true}`,
			Bundle:              strptr("b"),
			SourceMap:           strptr("sm"),
		}
		id, err := s.Create(ctx, &input)
		if err != nil {
			t.Fatal(err)
		}
		input.ID = id

		t.Run("GetArtifacts", func(t *testing.T) {
			bundle, sourcemap, err := s.GetArtifacts(ctx, id)
			if err != nil {
				t.Fatal(err)
			}
			if want := "b"; string(bundle) != want {
				t.Errorf("got %q, want %q", bundle, want)
			}
			if want := "sm"; string(sourcemap) != want {
				t.Errorf("got %q, want %q", sourcemap, want)
			}
		})

		t.Run("GetLatest for 1st release", func(t *testing.T) {
			r1, err := s.GetLatest(ctx, xExtensionID, "release", true)
			if err != nil {
				t.Fatal(err)
			}
			norm(r1)
			if !reflect.DeepEqual(*r1, input) {
				t.Errorf("got %+v, want %+v", r1, input)
			}
		})

		t.Run("GetLatest with wrong release tag", func(t *testing.T) {
			_, err := s.GetLatest(ctx, xExtensionID, "other", true)
			if !errcode.IsNotFound(err) {
				t.Errorf("got err %v, want errcode.IsNotFound", err)
			}
		})
	})

	var input2 Release
	t.Run("Create 2nd release and GetLatest", func(t *testing.T) {
		input2 = Release{
			RegistryExtensionID: xExtensionID,
			CreatorUserID:       user.ID,
			ReleaseTag:          "release",
			Manifest:            `{"m2": true}`,
			Bundle:              strptr("b2"),
			SourceMap:           strptr("sm2"),
		}
		id2, err := s.Create(ctx, &input2)
		if err != nil {
			t.Fatal(err)
		}
		input2.ID = id2

		r2, err := s.GetLatest(ctx, xExtensionID, "release", true)
		if err != nil {
			t.Fatal(err)
		}
		norm(r2)
		if !reflect.DeepEqual(*r2, input2) {
			t.Errorf("got %+v, want %+v", r2, input2)
		}
	})

	t.Run("GetLatestBatch", func(t *testing.T) {
		input3 := Release{
			RegistryExtensionID: yExtensionID,
			CreatorUserID:       user.ID,
			ReleaseTag:          "release",
			Manifest:            `{"m2": true}`,
			Bundle:              strptr("b2"),
			SourceMap:           strptr("sm2"),
		}
		id3, err := s.Create(ctx, &input3)
		if err != nil {
			t.Fatal(err)
		}
		input3.ID = id3

		r3, err := s.GetLatestBatch(ctx, []int32{xExtensionID, yExtensionID}, "release", true)
		if err != nil {
			t.Fatal(err)
		}
		norm(r3[0])
		norm(r3[1])
		expected := []*Release{&input2, &input3}
		if !reflect.DeepEqual(r3, expected) {
			t.Errorf("got %+v, want %+v", r3, expected)
		}
	})

	t.Run("Create fails on invalid JSON", func(t *testing.T) {
		_, err := s.Create(ctx, &Release{
			RegistryExtensionID: xExtensionID,
			CreatorUserID:       user.ID,
			ReleaseTag:          "release",
			Manifest:            `{title/`, // weird bad JSON (any invalid JSON suffices for this test)
			Bundle:              strptr(""),
			SourceMap:           strptr(""),
		})
		if want := ErrInvalidJSONInManifest; err != want {
			t.Fatalf("got error %v, want %v", err, want)
		}
	})

	t.Run("Release without bundle", func(t *testing.T) {
		input := Release{
			RegistryExtensionID: xExtensionID,
			CreatorUserID:       user.ID,
			ReleaseTag:          "release",
			Manifest:            `{"m3": true}`,
			Bundle:              nil,
			SourceMap:           nil,
		}
		id, err := s.Create(ctx, &input)
		if err != nil {
			t.Fatal(err)
		}

		bundle, sourcemap, err := s.GetArtifacts(ctx, id)
		if !errcode.IsNotFound(err) {
			t.Errorf("got err %v, want errcode.IsNotFound", err)
		}
		if bundle != nil {
			t.Error("bundle != nil")
		}
		if sourcemap != nil {
			t.Error("sourcemap != nil")
		}
	})
}

func strptr(s string) *string { return &s }
