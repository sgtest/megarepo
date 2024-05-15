package db

import (
	"context"
	"testing"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
)

type MockDiscussionThreads struct {
	Create func(ctx context.Context, newThread *types.DiscussionThread) (*types.DiscussionThread, error)
	Update func(ctx context.Context, threadID int64, opts *DiscussionThreadsUpdateOptions) (*types.DiscussionThread, error)
	List   func(ctx context.Context, opt *DiscussionThreadsListOptions) ([]*types.DiscussionThread, error)
	Count  func(ctx context.Context, opt *DiscussionThreadsListOptions) (int, error)
}

func (s *MockDiscussionThreads) MockCreate_Return(t *testing.T, returns *types.DiscussionThread, returnsErr error) (called *bool, calledWith *types.DiscussionThread) {
	called, calledWith = new(bool), &types.DiscussionThread{}
	s.Create = func(ctx context.Context, newThread *types.DiscussionThread) (*types.DiscussionThread, error) {
		*called = true
		return returns, returnsErr
	}
	return called, calledWith
}

func (s *MockDiscussionThreads) MockUpdate_Return(t *testing.T, returns *types.DiscussionThread, returnsErr error) (called *bool) {
	called = new(bool)
	s.Update = func(ctx context.Context, threadID int64, opts *DiscussionThreadsUpdateOptions) (*types.DiscussionThread, error) {
		*called = true
		return returns, returnsErr
	}
	return called
}
