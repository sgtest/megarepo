package api

import (
	"context"
	"testing"

	"github.com/google/go-cmp/cmp"
	store "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	storemocks "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore/mocks"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore"
	bundlemocks "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore/mocks"
)

func TestHover(t *testing.T) {
	mockStore := storemocks.NewMockStore()
	mockBundleStore := bundlemocks.NewMockStore()
	mockGitserverClient := NewMockGitserverClient()

	setMockStoreGetDumpByID(t, mockStore, map[int]store.Dump{42: testDump1})
	setMockBundleStoreHover(t, mockBundleStore, 42, "main.go", 10, 50, "text", testRange1, true)

	api := testAPI(mockStore, mockBundleStore, mockGitserverClient)
	text, r, exists, err := api.Hover(context.Background(), "sub1/main.go", 10, 50, 42)
	if err != nil {
		t.Fatalf("expected error getting hover text: %s", err)
	}
	if !exists {
		t.Fatalf("expected hover text to exist.")
	}

	if text != "text" {
		t.Errorf("unexpected text. want=%s have=%s", "text", text)
	}
	if diff := cmp.Diff(testRange1, r); diff != "" {
		t.Errorf("unexpected range (-want +got):\n%s", diff)
	}
}

func TestHoverUnknownDump(t *testing.T) {
	mockStore := storemocks.NewMockStore()
	mockBundleStore := bundlemocks.NewMockStore()
	mockGitserverClient := NewMockGitserverClient()
	setMockStoreGetDumpByID(t, mockStore, nil)

	api := testAPI(mockStore, mockBundleStore, mockGitserverClient)
	if _, _, _, err := api.Hover(context.Background(), "sub1/main.go", 10, 50, 42); err != ErrMissingDump {
		t.Fatalf("unexpected error getting hover text. want=%q have=%q", ErrMissingDump, err)
	}
}

func TestHoverRemoteDefinitionHoverText(t *testing.T) {
	mockStore := storemocks.NewMockStore()
	mockBundleStore := bundlemocks.NewMockStore()
	mockGitserverClient := NewMockGitserverClient()

	setMockStoreGetDumpByID(t, mockStore, map[int]store.Dump{42: testDump1, 50: testDump2})
	setMockBundleStoreDefinitions(t, mockBundleStore, 42, "main.go", 10, 50, nil)
	setMockBundleStoreMonikersByPosition(t, mockBundleStore, 42, "main.go", 10, 50, [][]lsifstore.MonikerData{{testMoniker1}})
	setMockBundleStorePackageInformation(t, mockBundleStore, 42, "main.go", "1234", testPackageInformation)
	setMockStoreGetPackage(t, mockStore, "gomod", "leftpad", "0.1.0", testDump2, true)
	setMockBundleStoreMonikerResults(t, mockBundleStore, 50, "definitions", "gomod", "pad", 0, 100, []lsifstore.Location{
		{DumpID: 50, Path: "foo.go", Range: testRange1},
		{DumpID: 50, Path: "bar.go", Range: testRange2},
		{DumpID: 50, Path: "baz.go", Range: testRange3},
	}, 15)
	setMultiMockBundleStoreHover(
		t,
		mockBundleStore,
		hoverSpec{42, "main.go", 10, 50, "", lsifstore.Range{}, false},
		hoverSpec{50, "foo.go", 10, 50, "text", testRange4, true},
	)

	api := testAPI(mockStore, mockBundleStore, mockGitserverClient)
	text, r, exists, err := api.Hover(context.Background(), "sub1/main.go", 10, 50, 42)
	if err != nil {
		t.Fatalf("expected error getting hover text: %s", err)
	}
	if !exists {
		t.Fatalf("expected hover text to exist.")
	}

	if text != "text" {
		t.Errorf("unexpected text. want=%s have=%s", "text", text)
	}
	if diff := cmp.Diff(testRange4, r); diff != "" {
		t.Errorf("unexpected range (-want +got):\n%s", diff)
	}
}

func TestHoverUnknownDefinition(t *testing.T) {
	mockStore := storemocks.NewMockStore()
	mockBundleStore := bundlemocks.NewMockStore()
	mockGitserverClient := NewMockGitserverClient()

	setMockStoreGetDumpByID(t, mockStore, map[int]store.Dump{42: testDump1})
	setMockBundleStoreHover(t, mockBundleStore, 42, "main.go", 10, 50, "", lsifstore.Range{}, false)
	setMockBundleStoreDefinitions(t, mockBundleStore, 42, "main.go", 10, 50, nil)
	setMockBundleStoreMonikersByPosition(t, mockBundleStore, 42, "main.go", 10, 50, [][]lsifstore.MonikerData{{testMoniker1}})
	setMockBundleStorePackageInformation(t, mockBundleStore, 42, "main.go", "1234", testPackageInformation)
	setMockStoreGetPackage(t, mockStore, "gomod", "leftpad", "0.1.0", store.Dump{}, false)

	api := testAPI(mockStore, mockBundleStore, mockGitserverClient)
	_, _, exists, err := api.Hover(context.Background(), "sub1/main.go", 10, 50, 42)
	if err != nil {
		t.Fatalf("unexpected error getting hover text: %s", err)
	}
	if exists {
		t.Errorf("unexpected hover text")
	}
}
