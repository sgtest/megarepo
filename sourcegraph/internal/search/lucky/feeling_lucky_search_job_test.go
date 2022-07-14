package lucky

import (
	"context"
	"encoding/json"
	"strconv"
	"testing"

	"github.com/hexops/autogold"
	"github.com/sourcegraph/sourcegraph/internal/search"
	alertobserver "github.com/sourcegraph/sourcegraph/internal/search/alert"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/job/mockjob"
	"github.com/sourcegraph/sourcegraph/internal/search/limits"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/stretchr/testify/require"
)

func TestNewFeelingLuckySearchJob_Run(t *testing.T) {
	// Setup: A child job that sends the same result
	mockJob := mockjob.NewMockJob()
	mockJob.RunFunc.SetDefaultHook(func(ctx context.Context, _ job.RuntimeClients, s streaming.Sender) (*search.Alert, error) {
		s.Send(streaming.SearchEvent{
			Results: []result.Match{&result.FileMatch{
				File: result.File{Path: "haut-medoc"},
			}},
		})
		return nil, nil
	})

	mockAutoQuery := &autoQuery{description: "mock", query: query.Basic{}}

	j := FeelingLuckySearchJob{
		initialJob: mockJob,
		generators: []next{func() (*autoQuery, next) { return mockAutoQuery, nil }},
		newGeneratedJob: func(*autoQuery) job.Job {
			return mockJob
		},
	}

	var sent []result.Match
	stream := streaming.StreamFunc(func(e streaming.SearchEvent) {
		sent = append(sent, e.Results...)
	})

	t.Run("deduplicate results returned by generated jobs", func(t *testing.T) {
		j.Run(context.Background(), job.RuntimeClients{}, stream)
		require.Equal(t, 1, len(sent))
	})
}

func TestGeneratedSearchJob(t *testing.T) {
	mockJob := mockjob.NewMockJob()
	setMockJobResultSize := func(n int) {
		mockJob.RunFunc.SetDefaultHook(func(ctx context.Context, _ job.RuntimeClients, s streaming.Sender) (*search.Alert, error) {
			for i := 0; i < n; i++ {
				select {
				case <-ctx.Done():
					return nil, ctx.Err()
				default:
				}
				s.Send(streaming.SearchEvent{
					Results: []result.Match{&result.FileMatch{
						File: result.File{Path: strconv.Itoa(i)},
					}},
				})
			}
			return nil, nil
		})
	}

	test := func(resultSize int) string {
		setMockJobResultSize(resultSize)
		q, _ := query.ParseStandard("test")
		mockQuery, _ := query.ToBasicQuery(q)
		notifier := &notifier{autoQuery: &autoQuery{description: "test", query: mockQuery}}
		j := &generatedSearchJob{
			Child:           mockJob,
			NewNotification: notifier.New,
		}
		_, err := j.Run(context.Background(), job.RuntimeClients{}, streaming.NewAggregatingStream())
		if err == nil {
			return ""
		}
		return err.(*alertobserver.ErrLuckyQueries).ProposedQueries[0].Description
	}

	autogold.Want("0 results", autogold.Raw("")).Equal(t, autogold.Raw(test(0)))
	autogold.Want("1 result", autogold.Raw("test (1 result)")).Equal(t, autogold.Raw(test(1)))
	autogold.Want("limit results", autogold.Raw("test (500+ results)")).Equal(t, autogold.Raw(test(limits.DefaultMaxSearchResultsStreaming)))
}

func TestCombinations(t *testing.T) {
	test := func(input string, rulesNarrow, rulesWiden []rule) string {
		q, _ := query.ParseStandard(input)
		b, _ := query.ToBasicQuery(q)
		g := NewGenerator(b, rulesNarrow, rulesWiden)

		var autoQ *autoQuery
		type want struct {
			Description string
			Query       string
		}
		generated := []want{}

		for {
			autoQ, g = g()
			if autoQ != nil {
				generated = append(
					generated,
					want{
						Description: autoQ.description,
						Query:       query.StringHuman(autoQ.query.ToParseTree()),
					})
			}

			if g == nil {
				break
			}
		}

		result, _ := json.MarshalIndent(generated, "", "  ")
		return string(result)
	}

	t.Run("narrow and widen rules", func(t *testing.T) {
		autogold.Equal(t, autogold.Raw(test(`go commit yikes derp`, rulesNarrow, rulesWiden)))
	})

	t.Run("only narrow rules", func(t *testing.T) {
		autogold.Equal(t, autogold.Raw(test(`go commit yikes derp`, rulesNarrow, nil)))
	})

	t.Run("only widen rules", func(t *testing.T) {
		autogold.Equal(t, autogold.Raw(test(`go commit yikes derp`, nil, rulesWiden)))
	})
}

func TestNewFeelingLuckySearchJob_ResultCount(t *testing.T) {
	// This test ensures the invariant that generated queries do not run if
	// at least RESULT_THRESHOLD results are emitted by the initial job. If
	// less than RESULT_THRESHOLD results are seen, the logic will run a
	// generated query, which always panics.
	mockJob := mockjob.NewMockJob()
	mockJob.RunFunc.SetDefaultHook(func(ctx context.Context, _ job.RuntimeClients, s streaming.Sender) (*search.Alert, error) {
		for i := 0; i < RESULT_THRESHOLD; i++ {
			s.Send(streaming.SearchEvent{
				Results: []result.Match{&result.FileMatch{
					File: result.File{Path: strconv.Itoa(i)},
				}},
			})
		}
		return nil, nil
	})

	mockAutoQuery := &autoQuery{description: "mock", query: query.Basic{}}

	j := FeelingLuckySearchJob{
		initialJob: mockJob,
		generators: []next{func() (*autoQuery, next) { return mockAutoQuery, nil }},
		newGeneratedJob: func(*autoQuery) job.Job {
			return mockjob.NewStrictMockJob() // always panic, and should never get run.
		},
	}

	var sent []result.Match
	stream := streaming.StreamFunc(func(e streaming.SearchEvent) {
		sent = append(sent, e.Results...)
	})

	t.Run("do not run generated queries over RESULT_THRESHOLD", func(t *testing.T) {
		j.Run(context.Background(), job.RuntimeClients{}, stream)
		require.Equal(t, RESULT_THRESHOLD, len(sent))
	})
}
