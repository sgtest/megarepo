package resolvers

import (
	"context"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/resolvers/apitest"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/store"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/testing"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

func TestChangesetSpecResolver(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ctx := backend.WithAuthzBypass(context.Background())
	dbtesting.SetupGlobalTestDB(t)

	userID := ct.CreateTestUser(t, false).ID

	cstore := store.New(dbconn.Global)
	esStore := database.ExternalServicesWith(cstore)

	// Creating user with matching email to the changeset spec author.
	user, err := database.GlobalUsers.Create(ctx, database.NewUser{
		Username:        "mary",
		Email:           ct.ChangesetSpecAuthorEmail,
		EmailIsVerified: true,
		DisplayName:     "Mary Tester",
	})
	if err != nil {
		t.Fatal(err)
	}

	repoStore := database.ReposWith(cstore)
	repo := newGitHubTestRepo("github.com/sourcegraph/changeset-spec-resolver-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}
	repoID := graphqlbackend.MarshalRepositoryID(repo.ID)

	testRev := api.CommitID("b69072d5f687b31b9f6ae3ceafdc24c259c4b9ec")
	mockBackendCommits(t, testRev)

	campaignSpec, err := campaigns.NewCampaignSpecFromRaw(`name: awesome-test`)
	if err != nil {
		t.Fatal(err)
	}
	campaignSpec.NamespaceUserID = userID
	if err := cstore.CreateCampaignSpec(ctx, campaignSpec); err != nil {
		t.Fatal(err)
	}

	s, err := graphqlbackend.NewSchema(dbconn.Global, &Resolver{store: cstore}, nil, nil, nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name    string
		rawSpec string
		want    func(spec *campaigns.ChangesetSpec) apitest.ChangesetSpec
	}{
		{
			name:    "GitBranchChangesetDescription",
			rawSpec: ct.NewRawChangesetSpecGitBranch(repoID, string(testRev)),
			want: func(spec *campaigns.ChangesetSpec) apitest.ChangesetSpec {
				return apitest.ChangesetSpec{
					Typename: "VisibleChangesetSpec",
					ID:       string(marshalChangesetSpecRandID(spec.RandID)),
					Description: apitest.ChangesetSpecDescription{
						Typename: "GitBranchChangesetDescription",
						BaseRepository: apitest.Repository{
							ID: string(spec.Spec.BaseRepository),
						},
						ExternalID: "",
						BaseRef:    git.AbbreviateRef(spec.Spec.BaseRef),
						HeadRepository: apitest.Repository{
							ID: string(spec.Spec.HeadRepository),
						},
						HeadRef: git.AbbreviateRef(spec.Spec.HeadRef),
						Title:   spec.Spec.Title,
						Body:    spec.Spec.Body,
						Commits: []apitest.GitCommitDescription{
							{
								Author: apitest.Person{
									Email: spec.Spec.Commits[0].AuthorEmail,
									Name:  user.Username,
									User: &apitest.User{
										ID: string(graphqlbackend.MarshalUserID(user.ID)),
									},
								},
								Diff:    spec.Spec.Commits[0].Diff,
								Message: spec.Spec.Commits[0].Message,
								Subject: "git commit message",
								Body:    "and some more content in a second paragraph.",
							},
						},
						Published: campaigns.PublishedValue{Val: false},
						Diff: struct{ FileDiffs apitest.FileDiffs }{
							FileDiffs: apitest.FileDiffs{
								DiffStat: apitest.DiffStat{
									Added:   1,
									Deleted: 1,
									Changed: 2,
								},
							},
						},
						DiffStat: apitest.DiffStat{
							Added:   1,
							Deleted: 1,
							Changed: 2,
						},
					},
					ExpiresAt: &graphqlbackend.DateTime{Time: spec.ExpiresAt().Truncate(time.Second)},
				}
			},
		},
		{
			name:    "GitBranchChangesetDescription Draft",
			rawSpec: ct.NewPublishedRawChangesetSpecGitBranch(repoID, string(testRev), campaigns.PublishedValue{Val: "draft"}),
			want: func(spec *campaigns.ChangesetSpec) apitest.ChangesetSpec {
				return apitest.ChangesetSpec{
					Typename: "VisibleChangesetSpec",
					ID:       string(marshalChangesetSpecRandID(spec.RandID)),
					Description: apitest.ChangesetSpecDescription{
						Typename: "GitBranchChangesetDescription",
						BaseRepository: apitest.Repository{
							ID: string(spec.Spec.BaseRepository),
						},
						ExternalID: "",
						BaseRef:    git.AbbreviateRef(spec.Spec.BaseRef),
						HeadRepository: apitest.Repository{
							ID: string(spec.Spec.HeadRepository),
						},
						HeadRef: git.AbbreviateRef(spec.Spec.HeadRef),
						Title:   spec.Spec.Title,
						Body:    spec.Spec.Body,
						Commits: []apitest.GitCommitDescription{
							{
								Author: apitest.Person{
									Email: spec.Spec.Commits[0].AuthorEmail,
									Name:  user.Username,
									User: &apitest.User{
										ID: string(graphqlbackend.MarshalUserID(user.ID)),
									},
								},
								Diff:    spec.Spec.Commits[0].Diff,
								Message: spec.Spec.Commits[0].Message,
								Subject: "git commit message",
								Body:    "and some more content in a second paragraph.",
							},
						},
						Published: campaigns.PublishedValue{Val: "draft"},
						Diff: struct{ FileDiffs apitest.FileDiffs }{
							FileDiffs: apitest.FileDiffs{
								DiffStat: apitest.DiffStat{
									Added:   1,
									Deleted: 1,
									Changed: 2,
								},
							},
						},
						DiffStat: apitest.DiffStat{
							Added:   1,
							Deleted: 1,
							Changed: 2,
						},
					},
					ExpiresAt: &graphqlbackend.DateTime{Time: spec.ExpiresAt().Truncate(time.Second)},
				}
			},
		},
		{
			name:    "ExistingChangesetReference",
			rawSpec: ct.NewRawChangesetSpecExisting(repoID, "9999"),
			want: func(spec *campaigns.ChangesetSpec) apitest.ChangesetSpec {
				return apitest.ChangesetSpec{
					Typename: "VisibleChangesetSpec",
					ID:       string(marshalChangesetSpecRandID(spec.RandID)),
					Description: apitest.ChangesetSpecDescription{
						Typename: "ExistingChangesetReference",
						BaseRepository: apitest.Repository{
							ID: string(spec.Spec.BaseRepository),
						},
						ExternalID: spec.Spec.ExternalID,
					},
					ExpiresAt: &graphqlbackend.DateTime{Time: spec.ExpiresAt().Truncate(time.Second)},
				}
			},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			spec, err := campaigns.NewChangesetSpecFromRaw(tc.rawSpec)
			if err != nil {
				t.Fatal(err)
			}
			spec.UserID = userID
			spec.RepoID = repo.ID
			spec.CampaignSpecID = campaignSpec.ID

			if err := cstore.CreateChangesetSpec(ctx, spec); err != nil {
				t.Fatal(err)
			}

			input := map[string]interface{}{"id": marshalChangesetSpecRandID(spec.RandID)}
			var response struct{ Node apitest.ChangesetSpec }
			apitest.MustExec(ctx, t, s, input, &response, queryChangesetSpecNode)

			want := tc.want(spec)
			if diff := cmp.Diff(want, response.Node); diff != "" {
				t.Fatalf("unexpected response (-want +got):\n%s", diff)
			}
		})
	}
}

const queryChangesetSpecNode = `
query($id: ID!) {
  node(id: $id) {
    __typename

    ... on VisibleChangesetSpec {
      id
      description {
        __typename

        ... on ExistingChangesetReference {
          baseRepository {
             id
          }
          externalID
        }

        ... on GitBranchChangesetDescription {
          baseRepository {
              id
          }
          baseRef
          baseRev

          headRepository {
              id
          }
          headRef

          title
          body

          commits {
            message
            subject
            body
            diff
            author {
              name
              email
              user {
                id
              }
            }
          }

          published

          diff {
            fileDiffs {
              diffStat { added, changed, deleted }
            }
          }
          diffStat { added, changed, deleted }
        }
      }

      expiresAt
    }
  }
}
`
