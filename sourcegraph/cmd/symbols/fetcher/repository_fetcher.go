package fetcher

import (
	"archive/tar"
	"context"
	"io"
	"strings"

	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/cmd/symbols/gitserver"
	"github.com/sourcegraph/sourcegraph/cmd/symbols/types"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type RepositoryFetcher interface {
	FetchRepositoryArchive(ctx context.Context, args types.SearchArgs, paths []string) <-chan parseRequestOrError
}

type repositoryFetcher struct {
	gitserverClient     gitserver.GitserverClient
	operations          *operations
	maxTotalPathsLength int
}

type ParseRequest struct {
	Path string
	Data []byte
}

type parseRequestOrError struct {
	ParseRequest ParseRequest
	Err          error
}

func NewRepositoryFetcher(gitserverClient gitserver.GitserverClient, maxTotalPathsLength int, observationContext *observation.Context) RepositoryFetcher {
	return &repositoryFetcher{
		gitserverClient:     gitserverClient,
		operations:          newOperations(observationContext),
		maxTotalPathsLength: maxTotalPathsLength,
	}
}

func (f *repositoryFetcher) FetchRepositoryArchive(ctx context.Context, args types.SearchArgs, paths []string) <-chan parseRequestOrError {
	requestCh := make(chan parseRequestOrError)

	go func() {
		defer close(requestCh)

		if err := f.fetchRepositoryArchive(ctx, args, paths, func(request ParseRequest) {
			requestCh <- parseRequestOrError{ParseRequest: request}
		}); err != nil {
			requestCh <- parseRequestOrError{Err: err}
		}
	}()

	return requestCh
}

func (f *repositoryFetcher) fetchRepositoryArchive(ctx context.Context, args types.SearchArgs, paths []string, callback func(request ParseRequest)) (err error) {
	ctx, trace, endObservation := f.operations.fetchRepositoryArchive.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("repo", string(args.Repo)),
		log.String("commitID", string(args.CommitID)),
		log.Int("paths", len(paths)),
		log.String("paths", strings.Join(paths, ":")),
	}})
	defer endObservation(1, observation.Args{})

	f.operations.fetching.Inc()
	defer f.operations.fetching.Dec()

	fetchAndRead := func(paths []string) error {
		rc, err := f.gitserverClient.FetchTar(ctx, args.Repo, args.CommitID, paths)
		if err != nil {
			return errors.Wrap(err, "gitserverClient.FetchTar")
		}
		defer rc.Close()

		err = readTar(ctx, tar.NewReader(rc), callback, trace)
		if err != nil {
			return errors.Wrap(err, "readTar")
		}

		return nil
	}

	if len(paths) == 0 {
		// Full archive
		return fetchAndRead(nil)
	}

	// Partial archive
	for _, pathBatch := range batchByTotalLength(paths, f.maxTotalPathsLength) {
		err = fetchAndRead(pathBatch)
		if err != nil {
			return err
		}
	}

	return nil
}

// batchByTotalLength returns batches of paths where each batch contains at most maxTotalLength
// characters, except when a single path exceeds the soft max, in which case that long path will be put
// into its own batch.
func batchByTotalLength(paths []string, maxTotalLength int) [][]string {
	batches := [][]string{}
	currentBatch := []string{}
	currentLength := 0

	for _, path := range paths {
		if len(currentBatch) > 0 && currentLength+len(path) > maxTotalLength {
			batches = append(batches, currentBatch)
			currentBatch = []string{}
			currentLength = 0
		}

		currentBatch = append(currentBatch, path)
		currentLength += len(path)
	}

	batches = append(batches, currentBatch)

	return batches
}

func readTar(ctx context.Context, tarReader *tar.Reader, callback func(request ParseRequest), traceLog observation.TraceLogger) error {
	for {
		if ctx.Err() != nil {
			return ctx.Err()
		}

		tarHeader, err := tarReader.Next()
		if err == io.EOF {
			return nil
		} else if err != nil {
			return err
		}

		if tarHeader.FileInfo().IsDir() || tarHeader.Typeflag == tar.TypeXGlobalHeader {
			continue
		}

		data := make([]byte, int(tarHeader.Size))
		traceLog.Log(log.Event("reading tar file contents"))
		if _, err := io.ReadFull(tarReader, data); err != nil {
			return err
		}
		traceLog.Log(log.Int("n", int(tarHeader.Size)))
		callback(ParseRequest{Path: tarHeader.Name, Data: data})
	}
}
