package dbstore

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/commitgraph"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/semantic"
)

func TestDefinitionDumps(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)

	moniker1 := semantic.QualifiedMonikerData{
		MonikerData: semantic.MonikerData{
			Scheme: "gomod",
		},
		PackageInformationData: semantic.PackageInformationData{
			Name:    "leftpad",
			Version: "0.1.0",
		},
	}

	moniker2 := semantic.QualifiedMonikerData{
		MonikerData: semantic.MonikerData{
			Scheme: "npm",
		},
		PackageInformationData: semantic.PackageInformationData{
			Name:    "north-pad",
			Version: "0.2.0",
		},
	}

	// Package does not exist initially
	if dumps, err := store.DefinitionDumps(context.Background(), []semantic.QualifiedMonikerData{moniker1}); err != nil {
		t.Fatalf("unexpected error getting package: %s", err)
	} else if len(dumps) != 0 {
		t.Fatal("unexpected record")
	}

	uploadedAt := time.Unix(1587396557, 0).UTC()
	startedAt := uploadedAt.Add(time.Minute)
	finishedAt := uploadedAt.Add(time.Minute * 2)
	expected1 := Dump{
		ID:             1,
		Commit:         makeCommit(1),
		Root:           "sub/",
		VisibleAtTip:   true,
		UploadedAt:     uploadedAt,
		State:          "completed",
		FailureMessage: nil,
		StartedAt:      &startedAt,
		FinishedAt:     &finishedAt,
		RepositoryID:   50,
		RepositoryName: "n-50",
		Indexer:        "lsif-go",
	}
	expected2 := Dump{
		ID:                2,
		Commit:            makeCommit(2),
		Root:              "other/",
		VisibleAtTip:      false,
		UploadedAt:        uploadedAt,
		State:             "completed",
		FailureMessage:    nil,
		StartedAt:         &startedAt,
		FinishedAt:        &finishedAt,
		RepositoryID:      50,
		RepositoryName:    "n-50",
		Indexer:           "lsif-tsc",
		AssociatedIndexID: nil,
	}

	insertUploads(t, db, dumpToUpload(expected1), dumpToUpload(expected2))
	insertVisibleAtTip(t, db, 50, 1)

	if err := store.UpdatePackages(context.Background(), 1, []semantic.Package{
		{Scheme: "gomod", Name: "leftpad", Version: "0.1.0"},
		{Scheme: "gomod", Name: "leftpad", Version: "0.1.0"},
	}); err != nil {
		t.Fatalf("unexpected error updating packages: %s", err)
	}

	if err := store.UpdatePackages(context.Background(), 2, []semantic.Package{
		{Scheme: "npm", Name: "north-pad", Version: "0.2.0"},
	}); err != nil {
		t.Fatalf("unexpected error updating packages: %s", err)
	}

	if dumps, err := store.DefinitionDumps(context.Background(), []semantic.QualifiedMonikerData{moniker1}); err != nil {
		t.Fatalf("unexpected error getting package: %s", err)
	} else if len(dumps) != 1 {
		t.Fatal("expected one record")
	} else if diff := cmp.Diff(expected1, dumps[0]); diff != "" {
		t.Errorf("unexpected dump (-want +got):\n%s", diff)
	}

	if dumps, err := store.DefinitionDumps(context.Background(), []semantic.QualifiedMonikerData{moniker1, moniker2}); err != nil {
		t.Fatalf("unexpected error getting package: %s", err)
	} else if len(dumps) != 2 {
		t.Fatal("expected two records")
	} else if diff := cmp.Diff(expected1, dumps[0]); diff != "" {
		t.Errorf("unexpected dump (-want +got):\n%s", diff)
	} else if diff := cmp.Diff(expected2, dumps[1]); diff != "" {
		t.Errorf("unexpected dump (-want +got):\n%s", diff)
	}
}

func TestReferenceIDsAndFilters(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)

	insertUploads(t, db,
		Upload{ID: 1, Commit: makeCommit(2), Root: "sub1/"},
		Upload{ID: 2, Commit: makeCommit(3), Root: "sub2/"},
		Upload{ID: 3, Commit: makeCommit(4), Root: "sub3/"},
		Upload{ID: 4, Commit: makeCommit(3), Root: "sub4/"},
		Upload{ID: 5, Commit: makeCommit(2), Root: "sub5/"},
	)

	insertNearestUploads(t, db, 50, map[string][]commitgraph.UploadMeta{
		makeCommit(1): {
			{UploadID: 1, Distance: 1},
			{UploadID: 2, Distance: 2},
			{UploadID: 3, Distance: 3},
			{UploadID: 4, Distance: 2},
			{UploadID: 5, Distance: 1},
		},
		makeCommit(2): {
			{UploadID: 1, Distance: 0},
			{UploadID: 2, Distance: 1},
			{UploadID: 3, Distance: 2},
			{UploadID: 4, Distance: 1},
			{UploadID: 5, Distance: 0},
		},
		makeCommit(3): {
			{UploadID: 1, Distance: 1},
			{UploadID: 2, Distance: 0},
			{UploadID: 3, Distance: 1},
			{UploadID: 4, Distance: 0},
			{UploadID: 5, Distance: 1},
		},
		makeCommit(4): {
			{UploadID: 1, Distance: 2},
			{UploadID: 2, Distance: 1},
			{UploadID: 3, Distance: 0},
			{UploadID: 4, Distance: 1},
			{UploadID: 5, Distance: 2},
		},
	})

	insertPackageReferences(t, store, []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 1, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f1")},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f2")},
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 4, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 5, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f5")},
	})

	moniker := semantic.QualifiedMonikerData{
		MonikerData: semantic.MonikerData{
			Scheme: "gomod",
		},
		PackageInformationData: semantic.PackageInformationData{
			Name:    "leftpad",
			Version: "0.1.0",
		},
	}

	refs := []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 1, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f1")},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f2")},
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 4, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 5, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f5")},
	}

	testCases := []struct {
		limit    int
		offset   int
		expected []lsifstore.PackageReference
	}{
		{5, 0, refs},
		{5, 2, refs[2:]},
		{2, 1, refs[1:3]},
		{5, 5, nil},
	}

	for i, testCase := range testCases {
		t.Run(fmt.Sprintf("i=%d", i), func(t *testing.T) {
			scanner, totalCount, err := store.ReferenceIDsAndFilters(context.Background(), 50, makeCommit(1), []semantic.QualifiedMonikerData{moniker}, testCase.limit, testCase.offset)
			if err != nil {
				t.Fatalf("unexpected error getting filters: %s", err)
			}

			if totalCount != 5 {
				t.Errorf("unexpected count. want=%d have=%d", 5, totalCount)
			}

			filters, err := consumeScanner(scanner)
			if err != nil {
				t.Fatalf("unexpected error from scanner: %s", err)
			}

			if diff := cmp.Diff(testCase.expected, filters); diff != "" {
				t.Errorf("unexpected filters (-want +got):\n%s", diff)
			}
		})
	}
}

func TestReferenceIDsAndFiltersVisibility(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)

	insertUploads(t, db,
		Upload{ID: 1, Commit: makeCommit(1), Root: "sub1/"}, // not visible
		Upload{ID: 2, Commit: makeCommit(2), Root: "sub2/"}, // not visible
		Upload{ID: 3, Commit: makeCommit(3), Root: "sub1/"},
		Upload{ID: 4, Commit: makeCommit(4), Root: "sub2/"},
		Upload{ID: 5, Commit: makeCommit(5), Root: "sub5/"},
	)

	insertNearestUploads(t, db, 50, map[string][]commitgraph.UploadMeta{
		makeCommit(1): {{UploadID: 1, Distance: 0}},
		makeCommit(2): {{UploadID: 2, Distance: 0}},
		makeCommit(3): {{UploadID: 3, Distance: 0}},
		makeCommit(4): {{UploadID: 4, Distance: 0}},
		makeCommit(5): {{UploadID: 5, Distance: 0}},
		makeCommit(6): {{UploadID: 3, Distance: 3}, {UploadID: 4, Distance: 2}, {UploadID: 5, Distance: 1}},
	})

	insertPackageReferences(t, store, []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 1, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f1")},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f2")},
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 4, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 5, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f5")},
	})

	moniker := semantic.QualifiedMonikerData{
		MonikerData: semantic.MonikerData{
			Scheme: "gomod",
		},
		PackageInformationData: semantic.PackageInformationData{
			Name:    "leftpad",
			Version: "0.1.0",
		},
	}

	scanner, totalCount, err := store.ReferenceIDsAndFilters(context.Background(), 50, makeCommit(6), []semantic.QualifiedMonikerData{moniker}, 5, 0)
	if err != nil {
		t.Fatalf("unexpected error getting filters: %s", err)
	}

	if totalCount != 3 {
		t.Errorf("unexpected count. want=%d have=%d", 3, totalCount)
	}

	filters, err := consumeScanner(scanner)
	if err != nil {
		t.Fatalf("unexpected error from scanner: %s", err)
	}

	expected := []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 4, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 5, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f5")},
	}
	if diff := cmp.Diff(expected, filters); diff != "" {
		t.Errorf("unexpected filters (-want +got):\n%s", diff)
	}
}

func TestReferenceIDsAndFiltersRemoteVisibility(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)

	insertUploads(t, db,
		Upload{ID: 1, Commit: makeCommit(1)},
		Upload{ID: 2, Commit: makeCommit(2), RepositoryID: 51},
		Upload{ID: 3, Commit: makeCommit(3), RepositoryID: 52},
		Upload{ID: 4, Commit: makeCommit(4), RepositoryID: 53},
		Upload{ID: 5, Commit: makeCommit(5), RepositoryID: 54},
		Upload{ID: 6, Commit: makeCommit(6), RepositoryID: 55},
		Upload{ID: 7, Commit: makeCommit(6), RepositoryID: 56},
		Upload{ID: 8, Commit: makeCommit(7), RepositoryID: 57},
	)
	insertVisibleAtTip(t, db, 50, 1)
	insertVisibleAtTip(t, db, 51, 2)
	insertVisibleAtTip(t, db, 52, 3)
	insertVisibleAtTip(t, db, 53, 4)
	insertVisibleAtTip(t, db, 54, 5)
	insertVisibleAtTip(t, db, 56, 7)
	insertVisibleAtTipNonDefaultBranch(t, db, 57, 8)

	insertPackageReferences(t, store, []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 1, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f1")}, // same repo, not visible in git
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f2")},
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 4, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 5, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f5")},
		{Package: lsifstore.Package{DumpID: 6, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f6")}, // remote repo not visible at tip
		{Package: lsifstore.Package{DumpID: 7, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f7")},
		{Package: lsifstore.Package{DumpID: 8, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f8")}, // visible on non-default branch
	})

	moniker := semantic.QualifiedMonikerData{
		MonikerData: semantic.MonikerData{
			Scheme: "gomod",
		},
		PackageInformationData: semantic.PackageInformationData{
			Name:    "leftpad",
			Version: "0.1.0",
		},
	}

	scanner, totalCount, err := store.ReferenceIDsAndFilters(context.Background(), 50, makeCommit(6), []semantic.QualifiedMonikerData{moniker}, 5, 0)
	if err != nil {
		t.Fatalf("unexpected error getting filters: %s", err)
	}

	if totalCount != 5 {
		t.Errorf("unexpected count. want=%d have=%d", 5, totalCount)
	}

	filters, err := consumeScanner(scanner)
	if err != nil {
		t.Fatalf("unexpected error from scanner: %s", err)
	}

	expected := []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f2")},
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 4, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 5, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f5")},
		{Package: lsifstore.Package{DumpID: 7, Scheme: "gomod", Name: "leftpad", Version: "0.1.0"}, Filter: []byte("f7")},
	}
	if diff := cmp.Diff(expected, filters); diff != "" {
		t.Errorf("unexpected filters (-want +got):\n%s", diff)
	}
}

func TestReferencesForUpload(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)

	insertUploads(t, db,
		Upload{ID: 1, Commit: makeCommit(2), Root: "sub1/"},
		Upload{ID: 2, Commit: makeCommit(3), Root: "sub2/"},
		Upload{ID: 3, Commit: makeCommit(4), Root: "sub3/"},
		Upload{ID: 4, Commit: makeCommit(3), Root: "sub4/"},
		Upload{ID: 5, Commit: makeCommit(2), Root: "sub5/"},
	)

	insertPackageReferences(t, store, []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 1, Scheme: "gomod", Name: "leftpad", Version: "1.1.0"}, Filter: []byte("f1")},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "2.1.0"}, Filter: []byte("f2")},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "3.1.0"}, Filter: []byte("f3")},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "4.1.0"}, Filter: []byte("f4")},
		{Package: lsifstore.Package{DumpID: 3, Scheme: "gomod", Name: "leftpad", Version: "5.1.0"}, Filter: []byte("f5")},
	})

	scanner, err := store.ReferencesForUpload(context.Background(), 2)
	if err != nil {
		t.Fatalf("unexpected error getting filters: %s", err)
	}

	filters, err := consumeScanner(scanner)
	if err != nil {
		t.Fatalf("unexpected error from scanner: %s", err)
	}

	expected := []lsifstore.PackageReference{
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "2.1.0"}, Filter: nil},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "3.1.0"}, Filter: nil},
		{Package: lsifstore.Package{DumpID: 2, Scheme: "gomod", Name: "leftpad", Version: "4.1.0"}, Filter: nil},
	}
	if diff := cmp.Diff(expected, filters); diff != "" {
		t.Errorf("unexpected filters (-want +got):\n%s", diff)
	}
}

// consumeScanner reads all values from the scanner into memory.
func consumeScanner(scanner PackageReferenceScanner) (references []lsifstore.PackageReference, _ error) {
	for {
		reference, exists, err := scanner.Next()
		if err != nil {
			return nil, err
		}
		if !exists {
			break
		}

		references = append(references, reference)
	}
	if err := scanner.Close(); err != nil {
		return nil, err
	}

	return references, nil
}
