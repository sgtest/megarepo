package search

import (
	"archive/tar"
	"bytes"
	"context"
	"io"
	"io/ioutil"
	"os"
	"sync/atomic"
	"testing"
	"time"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
)

func TestPrepareZip(t *testing.T) {
	s, cleanup := tmpStore(t)
	defer cleanup()

	wantRepo := gitserver.Repo{Name: "foo"}
	wantCommit := api.CommitID("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")

	returnFetch := make(chan struct{})
	var gotRepo gitserver.Repo
	var gotCommit api.CommitID
	var fetchZipCalled int64
	s.FetchTar = func(ctx context.Context, repo gitserver.Repo, commit api.CommitID) (io.ReadCloser, error) {
		<-returnFetch
		atomic.AddInt64(&fetchZipCalled, 1)
		gotRepo = repo
		gotCommit = commit
		return emptyTar(t), nil
	}

	// Fetch same commit in parallel to ensure single-flighting works
	startPrepareZip := make(chan struct{})
	prepareZipErr := make(chan error)
	for i := 0; i < 10; i++ {
		go func() {
			<-startPrepareZip
			_, err := s.prepareZip(context.Background(), wantRepo, wantCommit)
			prepareZipErr <- err
		}()
	}
	close(startPrepareZip)
	close(returnFetch)
	for i := 0; i < 10; i++ {
		err := <-prepareZipErr
		if err != nil {
			t.Fatal("expected prepareZip to succeed:", err)
		}
	}

	if gotCommit != wantCommit {
		t.Errorf("fetched wrong commit. got=%v want=%v", gotCommit, wantCommit)
	}
	if gotRepo != wantRepo {
		t.Errorf("fetched wrong repo. got=%v want=%v", gotRepo, wantRepo)
	}

	// Wait for item to appear on disk cache, then test again to ensure we
	// use the disk cache.
	onDisk := false
	for i := 0; i < 500; i++ {
		files, _ := ioutil.ReadDir(s.Path)
		if len(files) != 0 {
			onDisk = true
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if !onDisk {
		t.Fatal("timed out waiting for items to appear in cache at", s.Path)
	}
	_, err := s.prepareZip(context.Background(), wantRepo, wantCommit)
	if err != nil {
		t.Fatal("expected prepareZip to succeed:", err)
		return
	}
}

func TestPrepareZip_fetchTarFail(t *testing.T) {
	fetchErr := errors.New("test")
	s, cleanup := tmpStore(t)
	defer cleanup()
	s.FetchTar = func(ctx context.Context, repo gitserver.Repo, commit api.CommitID) (io.ReadCloser, error) {
		return nil, fetchErr
	}
	_, err := s.prepareZip(context.Background(), gitserver.Repo{Name: "foo"}, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
	if errors.Cause(err) != fetchErr {
		t.Fatalf("expected prepareZip to fail with %v, failed with %v", fetchErr, err)
	}
}

func tmpStore(t *testing.T) (*Store, func()) {
	d, err := ioutil.TempDir("", "search_test")
	if err != nil {
		t.Fatal(err)
		return nil, nil
	}
	return &Store{
		Path: d,
	}, func() { os.RemoveAll(d) }
}

func emptyTar(t *testing.T) io.ReadCloser {
	buf := new(bytes.Buffer)
	w := tar.NewWriter(buf)
	err := w.Close()
	if err != nil {
		t.Fatal(err)
		return nil
	}
	return ioutil.NopCloser(bytes.NewReader(buf.Bytes()))
}
