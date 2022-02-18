package main

import (
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/gqltestutil"
	"github.com/sourcegraph/sourcegraph/schema"
)

const (
	repoName   = "perforce/test-perms"
	aliceEmail = "alice@perforce.sgdev.org"
)

func TestSubRepoPermissionsPerforce(t *testing.T) {
	checkPerforceEnvironment(t)
	enableSubRepoPermissions(t)
	createPerforceExternalService(t)
	userClient, repoName := createTestUserAndWaitForRepo(t)

	// Test cases

	t.Run("can read README.md", func(t *testing.T) {
		blob, err := userClient.GitBlob(repoName, "master", "README.md")
		if err != nil {
			t.Fatal(err)
		}
		wantBlob := `This depot is used to test user and group permissions.
`
		if diff := cmp.Diff(wantBlob, blob); diff != "" {
			t.Fatalf("Blob mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("cannot read hack.sh", func(t *testing.T) {
		// Should not be able to read hack.sh
		blob, err := userClient.GitBlob(repoName, "master", "Security/hack.sh")
		if err != nil {
			t.Fatal(err)
		}

		// This is the desired behaviour at the moment, see where we check for
		// os.IsNotExist error in GitCommitResolver.Blob
		wantBlob := ``

		if diff := cmp.Diff(wantBlob, blob); diff != "" {
			t.Fatalf("Blob mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("file list excludes excluded files", func(t *testing.T) {
		files, err := userClient.GitListFilenames(repoName, "master")
		if err != nil {
			t.Fatal(err)
		}

		// Notice that Security/hack.sh is excluded
		wantFiles := []string{
			"Backend/main.go",
			"Frontend/app.ts",
			"README.md",
		}

		if diff := cmp.Diff(wantFiles, files); diff != "" {
			t.Fatalf("fileNames mismatch (-want +got):\n%s", diff)
		}
	})
}

func TestSubRepoPermissionsSearch(t *testing.T) {
	checkPerforceEnvironment(t)
	enableSubRepoPermissions(t)
	createPerforceExternalService(t)
	userClient, _ := createTestUserAndWaitForRepo(t)

	err := client.WaitForReposToBeIndexed(repoName)
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name          string
		query         string
		zeroResult    bool
		minMatchCount int64
	}{
		{
			name:          "indexed search, nonzero result",
			query:         `index:only This depot is used to test`,
			minMatchCount: 1,
		},
		{
			name:          "unindexed multiline search, nonzero result",
			query:         `index:no This depot is used to test`,
			minMatchCount: 1,
		},
		{
			name:       "indexed search of restricted content",
			query:      `index:only uploading your secrets`,
			zeroResult: true,
		},
		{
			name:       "unindexed search of restricted content",
			query:      `index:no uploading your secrets`,
			zeroResult: true,
		},
		{
			name:       "structural, indexed search of restricted content",
			query:      `repo:^perforce/test-perms$ echo "..." index:only patterntype:structural`,
			zeroResult: true,
		},
		{
			name:       "structural, unindexed search of restricted content",
			query:      `repo:^perforce/test-perms$ echo "..." index:no patterntype:structural`,
			zeroResult: true,
		},
		{
			name:          "structural, indexed search, nonzero result",
			query:         `println(...) index:only patterntype:structural`,
			minMatchCount: 1,
		},
		{
			name:          "structural, unindexed search, nonzero result",
			query:         `println(...) index:no patterntype:structural`,
			minMatchCount: 1,
		},
		{
			name:          "filename search, nonzero result",
			query:         `repo:^perforce/test-perms$ type:path app`,
			minMatchCount: 1,
		},
		{
			name:       "filename search of restricted content",
			query:      `repo:^perforce/test-perms$ type:path hack`,
			zeroResult: true,
		},
		{
			name:          "content search, nonzero result",
			query:         `repo:^perforce/test-perms$ type:file let`,
			minMatchCount: 1,
		},
		{
			name:       "content search of restricted content",
			query:      `repo:^perforce/test-perms$ type:file echo`,
			zeroResult: true,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			results, err := userClient.SearchFiles(test.query)
			if err != nil {
				t.Fatal(err)
			}

			if test.zeroResult {
				if len(results.Results) > 0 {
					t.Fatalf("Want zero result but got %d", len(results.Results))
				}
			} else {
				if len(results.Results) == 0 {
					t.Fatal("Want non-zero results but got 0")
				}
			}

			if results.MatchCount < test.minMatchCount {
				t.Fatalf("Want at least %d match count but got %d", test.minMatchCount, results.MatchCount)
			}
		})
	}
}

func createTestUserAndWaitForRepo(t *testing.T) (*gqltestutil.Client, string) {
	t.Helper()

	// We need to create the `alice` user with a specific e-mail address. This user is
	// configured on our dogfood perforce instance with limited access to the
	// test-perms depot.
	// Alice has access to root, Backend and Frontend directories. (there are .md, .ts and .go files)
	// Alice doesn't have access to Security directory. (there is a .sh file)
	alicePassword := "alicessupersecurepassword"
	t.Log("Creating Alice")
	userClient, err := gqltestutil.SignUp(*baseURL, aliceEmail, "alice", alicePassword)
	if err != nil {
		t.Fatal(err)
	}

	aliceID := userClient.AuthenticatedUserID()
	t.Cleanup(func() {
		if err := client.DeleteUser(aliceID, true); err != nil {
			t.Fatal(err)
		}
	})

	if err := client.SetUserEmailVerified(aliceID, aliceEmail, true); err != nil {
		t.Fatal(err)
	}

	err = userClient.WaitForReposToBeCloned(repoName)
	if err != nil {
		t.Fatal(err)
	}
	return userClient, repoName
}

func enableSubRepoPermissions(t *testing.T) {
	t.Helper()

	siteConfig, err := client.SiteConfiguration()
	if err != nil {
		t.Fatal(err)
	}
	oldSiteConfig := new(schema.SiteConfiguration)
	*oldSiteConfig = *siteConfig
	t.Cleanup(func() {
		err = client.UpdateSiteConfiguration(oldSiteConfig)
		if err != nil {
			t.Fatal(err)
		}
	})

	siteConfig.ExperimentalFeatures = &schema.ExperimentalFeatures{
		Perforce: "enabled",
		SubRepoPermissions: &schema.SubRepoPermissions{
			Enabled: true,
		},
	}
	err = client.UpdateSiteConfiguration(siteConfig)
	if err != nil {
		t.Fatal(err)
	}
}
