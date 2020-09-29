package testing

import (
	"context"
	"fmt"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
)

// FakeChangesetSource is a fake implementation of the repos.ChangesetSource
// interface to be used in tests.
type FakeChangesetSource struct {
	Svc *repos.ExternalService

	CreateChangesetCalled  bool
	UpdateChangesetCalled  bool
	ListReposCalled        bool
	ExternalServicesCalled bool
	LoadChangesetsCalled   bool
	CloseChangesetCalled   bool
	ReopenChangesetCalled  bool

	// The Changeset.HeadRef to be expected in CreateChangeset/UpdateChangeset calls.
	WantHeadRef string
	// The Changeset.BaseRef to be expected in CreateChangeset/UpdateChangeset calls.
	WantBaseRef string

	// The metadata the FakeChangesetSource should set on the created/updated
	// Changeset with changeset.SetMetadata.
	FakeMetadata interface{}

	// Whether or not the changeset already ChangesetExists on the code host at the time
	// when CreateChangeset is called.
	ChangesetExists bool

	// error to be returned from every method
	Err error

	// ClosedChangesets contains the changesets that were passed to CloseChangeset
	ClosedChangesets []*repos.Changeset

	// CreatedChangesets contains the changesets that were passed to
	// CreateChangeset
	CreatedChangesets []*repos.Changeset

	// LoadedChangesets contains the changesets that were passed to LoadChangesets
	LoadedChangesets []*repos.Changeset

	// UpdateChangesets contains the changesets that were passed to
	// UpdateChangeset
	UpdatedChangesets []*repos.Changeset

	// ReopenedChangesets contains the changesets that were passed to ReopenedChangeset
	ReopenedChangesets []*repos.Changeset
}

func (s *FakeChangesetSource) CreateChangeset(ctx context.Context, c *repos.Changeset) (bool, error) {
	s.CreateChangesetCalled = true

	if s.Err != nil {
		return s.ChangesetExists, s.Err
	}

	if c.Repo == nil {
		return false, NoReposErr
	}

	if c.HeadRef != s.WantHeadRef {
		return s.ChangesetExists, fmt.Errorf("wrong HeadRef. want=%s, have=%s", s.WantHeadRef, c.HeadRef)
	}

	if c.BaseRef != s.WantBaseRef {
		return s.ChangesetExists, fmt.Errorf("wrong BaseRef. want=%s, have=%s", s.WantBaseRef, c.BaseRef)
	}

	if err := c.SetMetadata(s.FakeMetadata); err != nil {
		return s.ChangesetExists, err
	}

	s.CreatedChangesets = append(s.CreatedChangesets, c)
	return s.ChangesetExists, s.Err
}

func (s *FakeChangesetSource) UpdateChangeset(ctx context.Context, c *repos.Changeset) error {
	s.UpdateChangesetCalled = true

	if s.Err != nil {
		return s.Err
	}
	if c.Repo == nil {
		return NoReposErr
	}

	if c.BaseRef != s.WantBaseRef {
		return fmt.Errorf("wrong BaseRef. want=%s, have=%s", s.WantBaseRef, c.BaseRef)
	}

	s.UpdatedChangesets = append(s.UpdatedChangesets, c)
	return c.SetMetadata(s.FakeMetadata)
}

var fakeNotImplemented = errors.New("not implemented in FakeChangesetSource")

func (s *FakeChangesetSource) ListRepos(ctx context.Context, results chan repos.SourceResult) {
	s.ListReposCalled = true

	results <- repos.SourceResult{Source: s, Err: fakeNotImplemented}
}

func (s *FakeChangesetSource) ExternalServices() repos.ExternalServices {
	s.ExternalServicesCalled = true

	return repos.ExternalServices{s.Svc}
}
func (s *FakeChangesetSource) LoadChangesets(ctx context.Context, cs ...*repos.Changeset) error {
	s.LoadChangesetsCalled = true

	if s.Err != nil {
		return s.Err
	}

	for _, c := range cs {
		if c.Repo == nil {
			return NoReposErr
		}

		if err := c.SetMetadata(s.FakeMetadata); err != nil {
			return err
		}
	}

	s.LoadedChangesets = append(s.LoadedChangesets, cs...)
	return nil
}

var NoReposErr = errors.New("no repository set on repos.Changeset")

func (s *FakeChangesetSource) CloseChangeset(ctx context.Context, c *repos.Changeset) error {
	s.CloseChangesetCalled = true

	if s.Err != nil {
		return s.Err
	}

	if c.Repo == nil {
		return NoReposErr
	}

	s.ClosedChangesets = append(s.ClosedChangesets, c)

	return c.SetMetadata(s.FakeMetadata)
}

func (s *FakeChangesetSource) ReopenChangeset(ctx context.Context, c *repos.Changeset) error {
	s.ReopenChangesetCalled = true

	if s.Err != nil {
		return s.Err
	}

	if c.Repo == nil {
		return NoReposErr
	}

	s.ReopenedChangesets = append(s.ReopenedChangesets, c)

	return c.SetMetadata(s.FakeMetadata)
}

// FakeGitserverClient is a test implementation of the GitserverClient
// interface required by ExecChangesetJob.
type FakeGitserverClient struct {
	Response    string
	ResponseErr error

	CreateCommitFromPatchCalled bool
}

func (f *FakeGitserverClient) CreateCommitFromPatch(ctx context.Context, req protocol.CreateCommitFromPatchRequest) (string, error) {
	f.CreateCommitFromPatchCalled = true
	return f.Response, f.ResponseErr
}
