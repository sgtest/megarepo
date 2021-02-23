package lsifstore

import (
	"context"
	"fmt"
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestDatabaseMonikersByPosition(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	populateTestStore(t)
	store := NewStore(dbconn.Global, &observation.TestContext)

	// `func NewMetaData(id, root string, info ToolInfo) *MetaData {`
	//       ^^^^^^^^^^^

	if actual, err := store.MonikersByPosition(context.Background(), testBundleID, "protocol/protocol.go", 92, 10); err != nil {
		t.Fatalf("unexpected error %s", err)
	} else {
		expected := [][]MonikerData{
			{
				{
					Kind:                 "export",
					Scheme:               "gomod",
					Identifier:           "github.com/sourcegraph/lsif-go/protocol:NewMetaData",
					PackageInformationID: "60",
				},
			},
		}

		if diff := cmp.Diff(expected, actual); diff != "" {
			t.Errorf("unexpected moniker result (-want +got):\n%s", diff)
		}
	}
}

func TestDatabaseBulkMonikerResults(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	populateTestStore(t)
	store := NewStore(dbconn.Global, &observation.TestContext)

	edgeDefinitionLocations := []Location{
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(410, 5, 410, 9)},
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(411, 1, 411, 8)},
	}

	edgeReferenceLocations := []Location{
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(507, 1, 507, 5)},
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(530, 1, 530, 5)},
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(516, 8, 516, 12)},
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(410, 5, 410, 9)},
		{DumpID: testBundleID, Path: "protocol/protocol.go", Range: newRange(470, 8, 470, 12)},
		{DumpID: testBundleID, Path: "internal/index/helper.go", Range: newRange(78, 8, 78, 12)},
	}

	markdownReferenceLocations := []Location{
		{DumpID: testBundleID, Path: "internal/index/helper.go", Range: newRange(78, 6, 78, 16)},
	}

	combinedReferences := append(markdownReferenceLocations, edgeReferenceLocations...)
	edgeMoniker := MonikerData{Scheme: "gomod", Identifier: "github.com/sourcegraph/lsif-go/protocol:Edge"}
	markdownMoniker := MonikerData{Scheme: "gomod", Identifier: "github.com/slimsag/godocmd:ToMarkdown"}

	testCases := []struct {
		tableName          string
		uploadIDs          []int
		monikers           []MonikerData
		limit              int
		offset             int
		expectedLocations  []Location
		expectedTotalCount int
	}{
		// empty cases
		{"definitions", []int{}, []MonikerData{edgeMoniker}, 5, 0, nil, 0},
		{"definitions", []int{testBundleID}, []MonikerData{}, 5, 0, nil, 0},

		// single definitions
		{"definitions", []int{testBundleID}, []MonikerData{edgeMoniker}, 5, 0, edgeDefinitionLocations, 2},
		{"definitions", []int{testBundleID}, []MonikerData{edgeMoniker}, 1, 0, edgeDefinitionLocations[:1], 2},
		{"definitions", []int{testBundleID}, []MonikerData{edgeMoniker}, 5, 1, edgeDefinitionLocations[1:], 2},

		// single references
		{"references", []int{testBundleID}, []MonikerData{edgeMoniker}, 5, 0, edgeReferenceLocations[:5], 29},
		{"references", []int{testBundleID}, []MonikerData{edgeMoniker}, 2, 2, edgeReferenceLocations[2:4], 29},
		{"references", []int{testBundleID}, []MonikerData{markdownMoniker}, 5, 0, markdownReferenceLocations, 1},

		// multiple monikers
		{"references", []int{testBundleID}, []MonikerData{edgeMoniker, markdownMoniker}, 5, 0, combinedReferences[:5], 30},
		{"references", []int{testBundleID}, []MonikerData{edgeMoniker, markdownMoniker}, 5, 1, combinedReferences[1:6], 30},
	}

	for i, testCase := range testCases {
		t.Run(fmt.Sprintf("i=%d", i), func(t *testing.T) {
			if actual, totalCount, err := store.BulkMonikerResults(
				context.Background(),
				testCase.tableName,
				testCase.uploadIDs,
				testCase.monikers,
				testCase.limit,
				testCase.offset,
			); err != nil {
				t.Fatalf("unexpected error for test case #%d: %s", i, err)
			} else {
				if totalCount != testCase.expectedTotalCount {
					t.Errorf("unexpected moniker result total count for test case #%d. want=%d have=%d", i, testCase.expectedTotalCount, totalCount)
				}

				if diff := cmp.Diff(testCase.expectedLocations, actual); diff != "" {
					t.Errorf("unexpected moniker result locations for test case #%d (-want +got):\n%s", i, diff)
				}
			}
		})
	}
}
