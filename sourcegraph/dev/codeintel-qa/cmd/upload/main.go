package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"sort"
	"time"

	"github.com/sourcegraph/sourcegraph/dev/codeintel-qa/internal"
)

var (
	indexDir             string
	numConcurrentUploads int
	verbose              bool
	pollInterval         time.Duration
	timeout              time.Duration

	start = time.Now()
)

func init() {
	// Default assumes running from the dev/codeintel-qa directory
	flag.StringVar(&indexDir, "index-dir", "./testdata/indexes", "The location of the testdata directory")
	flag.IntVar(&numConcurrentUploads, "num-concurrent-uploads", 5, "The maximum number of concurrent uploads")
	flag.BoolVar(&verbose, "verbose", false, "Display full state from graphql")
	flag.DurationVar(&pollInterval, "poll-interval", time.Second*5, "The time to wait between graphql requests")
	flag.DurationVar(&timeout, "timeout", 0, "The time it should take to upload and process all targets")
}

func main() {
	// FIXME: Flaky test in CI. Re-enable https://github.com/sourcegraph/sourcegraph/issues/37671
	return
	if err := flag.CommandLine.Parse(os.Args[1:]); err != nil {
		fmt.Fprintf(os.Stderr, "error: %s\n", err)
		os.Exit(1)
	}

	ctx := context.Background()
	if timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}

	if err := mainErr(ctx); err != nil {
		fmt.Printf("%s error: %s\n", internal.EmojiFailure, err.Error())
		os.Exit(1)
	}
}

func mainErr(ctx context.Context) error {
	if err := internal.InitializeGraphQLClient(); err != nil {
		return err
	}

	commitsByRepo, err := internal.CommitsByRepo(indexDir)
	if err != nil {
		return err
	}

	repoNames := make([]string, 0, len(commitsByRepo))
	for name := range commitsByRepo {
		repoNames = append(repoNames, name)
	}
	sort.Strings(repoNames)

	limiter := internal.NewLimiter(numConcurrentUploads)
	defer limiter.Close()

	uploads, err := uploadAll(ctx, commitsByRepo, limiter)
	if err != nil {
		return err
	}

	if err := monitor(ctx, repoNames, uploads); err != nil {
		return err
	}

	return nil
}
