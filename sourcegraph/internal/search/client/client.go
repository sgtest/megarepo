package client

import (
	"context"

	"github.com/google/zoekt"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/endpoint"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/execute"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/run"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/schema"
)

//go:generate ../../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/internal/search/client -i SearchClient -o mock_client.go
type SearchClient interface {
	Plan(
		ctx context.Context,
		db database.DB,
		version string,
		patternType *string,
		searchQuery string,
		protocol search.Protocol,
		settings *schema.Settings,
		sourcegraphDotComMode bool,
	) (*run.SearchInputs, error)

	Execute(
		ctx context.Context,
		db database.DB,
		stream streaming.Sender,
		inputs *run.SearchInputs,
	) (_ *search.Alert, err error)
}

func NewSearchClient(zoektStreamer zoekt.Streamer, searcherURLs *endpoint.Map) SearchClient {
	return &searchClient{
		zoekt:        zoektStreamer,
		searcherURLs: searcherURLs,
	}
}

type searchClient struct {
	zoekt        zoekt.Streamer
	searcherURLs *endpoint.Map
}

func (s *searchClient) Plan(
	ctx context.Context,
	db database.DB,
	version string,
	patternType *string,
	searchQuery string,
	protocol search.Protocol,
	settings *schema.Settings,
	sourcegraphDotComMode bool,
) (*run.SearchInputs, error) {
	return run.NewSearchInputs(ctx, db, version, patternType, searchQuery, protocol, settings, sourcegraphDotComMode)
}

func (s *searchClient) Execute(
	ctx context.Context,
	db database.DB,
	stream streaming.Sender,
	inputs *run.SearchInputs,
) (*search.Alert, error) {
	jobArgs := &job.Args{
		SearchInputs: inputs,
		Zoekt:        s.zoekt,
		SearcherURLs: s.searcherURLs,
	}
	return execute.Execute(ctx, db, stream, jobArgs)
}
