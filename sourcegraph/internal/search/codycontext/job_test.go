package codycontext

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/job/mockjob"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/types"

	"github.com/stretchr/testify/require"
)

func TestRun(t *testing.T) {
	codeJob := mockjob.NewMockJob()
	textJob := mockjob.NewMockJob()

	// Create the job
	searchJob := &searchJob{
		codeCount:   4,
		textCount:   2,
		codeJob:     codeJob,
		textJob:     textJob,
		fileMatcher: func(api.RepoID, string) bool { return true },
	}

	{
		// Test single error
		codeJob.RunFunc.SetDefaultReturn(nil, errors.New("code job failed"))
		alert, err := searchJob.Run(context.Background(), job.RuntimeClients{}, streaming.NewNullStream())
		require.Nil(t, alert)
		require.NotNil(t, err)
	}

	{
		// Test both jobs error
		codeJob.RunFunc.SetDefaultReturn(nil, errors.New("code job failed"))
		textJob.RunFunc.SetDefaultReturn(nil, errors.New("text job failed"))
		_, err := searchJob.Run(context.Background(), job.RuntimeClients{}, streaming.NewNullStream())
		require.NotNil(t, err)
	}

	{
		// Test we select max priority alert
		codeJob.RunFunc.SetDefaultReturn(&search.Alert{Priority: 1}, nil)
		textJob.RunFunc.SetDefaultReturn(&search.Alert{Priority: 2}, nil)
		alert, _ := searchJob.Run(context.Background(), job.RuntimeClients{}, streaming.NewNullStream())
		require.NotNil(t, alert)
		require.Equal(t, 2, alert.Priority)
	}

	{
		// Test that results are limited and combined as expected
		codeMatches := result.Matches{
			&result.FileMatch{File: result.File{Path: "file1.go"}},
			&result.FileMatch{File: result.File{Path: "file2.go"}},
			&result.FileMatch{File: result.File{Path: "file3.go"}},
			&result.FileMatch{File: result.File{Path: "file4.go"}},
			&result.FileMatch{File: result.File{Path: "file5.go"}},
		}

		textMatches := result.Matches{
			&result.FileMatch{File: result.File{Path: "file1.md"}},
			&result.FileMatch{File: result.File{Path: "file2.md"}},
			&result.FileMatch{File: result.File{Path: "file3.md"}},
			&result.FileMatch{File: result.File{Path: "file4.md"}},
			&result.FileMatch{File: result.File{Path: "file5.md"}},
		}

		codeJob.RunFunc.SetDefaultHook(
			func(ctx context.Context, clients job.RuntimeClients, sender streaming.Sender) (*search.Alert, error) {
				sender.Send(streaming.SearchEvent{Results: codeMatches})
				return &search.Alert{}, nil
			})
		textJob.RunFunc.SetDefaultHook(
			func(ctx context.Context, clients job.RuntimeClients, sender streaming.Sender) (*search.Alert, error) {
				sender.Send(streaming.SearchEvent{Results: textMatches})
				return nil, nil
			})

		stream := streaming.NewAggregatingStream()
		alert, err := searchJob.Run(context.Background(), job.RuntimeClients{}, stream)
		require.NotNil(t, alert)
		require.Nil(t, err)

		require.Equal(t, append(codeMatches[:4], textMatches[:2]...), stream.Results)
	}
}

func TestCodyIgnoreFiltering(t *testing.T) {
	codeJob := mockjob.NewMockJob()
	textJob := mockjob.NewMockJob()

	// Create the job
	searchJob := &searchJob{
		codeCount: 4,
		textCount: 2,
		codeJob:   codeJob,
		textJob:   textJob,
		// Add a file matcher that mimics a Cody ignore rule that checks both repo and path.
		fileMatcher: func(repoID api.RepoID, path string) bool {
			return repoID == 1 && strings.Contains(path, "allowed")
		},
	}

	{
		// Test that results are filtered, limited and combined as expected
		allowed := types.MinimalRepo{ID: 1, Name: "allowed"}
		not := types.MinimalRepo{ID: 2, Name: "not"}
		codeMatches := result.Matches{
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file1.go"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file2.go"}},
			&result.FileMatch{File: result.File{Repo: not, Path: "file3.go"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file4.go"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file5.go"}},
			&result.FileMatch{File: result.File{Repo: not, Path: "allowed1.go"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "allowed2.go"}},
		}

		textMatches := result.Matches{
			&result.FileMatch{File: result.File{Repo: allowed, Path: "allowed1.md"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file1.md"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file2.md"}},
			&result.FileMatch{File: result.File{Repo: not, Path: "allowed2.md"}},
			&result.FileMatch{File: result.File{Repo: not, Path: "file3.md"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file4.md"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "allowed3.md"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "file5.md"}},
			&result.FileMatch{File: result.File{Repo: allowed, Path: "allowed4.md"}},
		}

		codeJob.RunFunc.SetDefaultHook(
			func(ctx context.Context, clients job.RuntimeClients, sender streaming.Sender) (*search.Alert, error) {
				sender.Send(streaming.SearchEvent{Results: codeMatches})
				return &search.Alert{}, nil
			})
		textJob.RunFunc.SetDefaultHook(
			func(ctx context.Context, clients job.RuntimeClients, sender streaming.Sender) (*search.Alert, error) {
				sender.Send(streaming.SearchEvent{Results: textMatches})
				return nil, nil
			})

		stream := streaming.NewAggregatingStream()
		alert, err := searchJob.Run(context.Background(), job.RuntimeClients{}, stream)
		require.NotNil(t, alert)
		require.Nil(t, err)

		expectedMatches := []*result.FileMatch{
			{File: result.File{Repo: allowed, Path: "allowed2.go"}},
			{File: result.File{Repo: allowed, Path: "allowed1.md"}},
			{File: result.File{Repo: allowed, Path: "allowed3.md"}},
			// allowed4.md also matches, but the text result limit is 2
		}
		require.Equal(t, len(expectedMatches), len(stream.Results))

		for i, match := range stream.Results {
			expectedMatch := expectedMatches[i]
			if fileMatch, ok := match.(*result.FileMatch); ok {
				require.Equal(t, expectedMatch.Repo, fileMatch.Repo)
				require.Equal(t, expectedMatch.Path, fileMatch.Path)
			} else {
				t.Fatalf("expected file match, received %v", match)
			}
		}
	}
}
