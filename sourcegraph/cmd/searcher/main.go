//docker:user sourcegraph

// searcher is a simple service which exposes an API to text search a repo at
// a specific commit. See the searcher package for more information.
package main

import (
	"context"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strconv"
	"time"

	"github.com/opentracing-contrib/go-stdlib/nethttp"
	opentracing "github.com/opentracing/opentracing-go"
	log15 "gopkg.in/inconshreveable/log15.v2"

	"github.com/sourcegraph/sourcegraph/cmd/searcher/search"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/debugserver"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/tracer"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
)

var cacheDir = env.Get("CACHE_DIR", "/tmp", "directory to store cached archives.")
var cacheSizeMB = env.Get("SEARCHER_CACHE_SIZE_MB", "0", "maximum size of the on disk cache in megabytes")
var insecureDev, _ = strconv.ParseBool(env.Get("INSECURE_DEV", "false", "Running in insecure dev (local laptop) mode"))

const port = "3181"

func main() {
	env.Lock()
	env.HandleHelpFlag()
	log.SetFlags(0)
	tracer.Init()

	go debugserver.Start()

	var cacheSizeBytes int64
	if i, err := strconv.ParseInt(cacheSizeMB, 10, 64); err != nil {
		log.Fatalf("invalid int %q for SEARCHER_CACHE_SIZE_MB: %s", cacheSizeMB, err)
	} else {
		cacheSizeBytes = i * 1000 * 1000
	}

	service := &search.Service{
		Store: &search.Store{
			FetchTar: func(ctx context.Context, repo gitserver.Repo, commit api.CommitID) (io.ReadCloser, error) {
				return git.Archive(ctx, repo, git.ArchiveOptions{Treeish: string(commit), Format: "tar"})
			},
			Path:              filepath.Join(cacheDir, "searcher-archives"),
			MaxCacheSizeBytes: cacheSizeBytes,

			// Allow roughly 10 fetches per gitserver
			MaxConcurrentFetchTar: 10 * len(gitserver.DefaultClient.Addrs),
		},
	}
	service.Store.Start()
	handler := nethttp.Middleware(opentracing.GlobalTracer(), service)

	host := ""
	if insecureDev {
		host = "127.0.0.1"
	}
	addr := net.JoinHostPort(host, port)
	server := &http.Server{
		Addr: addr,
		Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// For kubernetes liveness and readiness probes
			if r.URL.Path == "/healthz" {
				w.WriteHeader(200)
				w.Write([]byte("ok"))
				return
			}

			handler.ServeHTTP(w, r)
		}),
	}
	go shutdownOnSIGINT(server)

	log15.Info("searcher: listening", "addr", server.Addr)
	err := server.ListenAndServe()
	if err != http.ErrServerClosed {
		log.Fatal(err)
	}
}

func shutdownOnSIGINT(s *http.Server) {
	c := make(chan os.Signal, 1)
	signal.Notify(c, os.Interrupt)
	<-c
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	err := s.Shutdown(ctx)
	if err != nil {
		log.Fatal("graceful server shutdown failed, will exit:", err)
	}
}
