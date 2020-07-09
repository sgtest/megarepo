package correlation

import (
	"sort"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation/datastructures"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation/lsif"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bloomfilter"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/types"
)

func TestGroupBundleData(t *testing.T) {
	state := &State{
		DocumentData: map[int]lsif.Document{
			1001: {
				URI:         "foo.go",
				Contains:    datastructures.IDSetWith(2001, 2002, 2003),
				Diagnostics: datastructures.IDSetWith(1001, 1002),
			},
			1002: {
				URI:         "bar.go",
				Contains:    datastructures.IDSetWith(2004, 2005, 2006),
				Diagnostics: datastructures.IDSetWith(1003),
			},
			1003: {
				URI:         "baz.go",
				Contains:    datastructures.IDSetWith(2007, 2008, 2009),
				Diagnostics: datastructures.NewIDSet(),
			},
		},
		RangeData: map[int]lsif.Range{
			2001: {
				StartLine:          1,
				StartCharacter:     2,
				EndLine:            3,
				EndCharacter:       4,
				DefinitionResultID: 3001,
				ReferenceResultID:  0,
				MonikerIDs:         datastructures.IDSetWith(4001, 4002),
			},
			2002: {
				StartLine:          2,
				StartCharacter:     3,
				EndLine:            4,
				EndCharacter:       5,
				DefinitionResultID: 0,
				ReferenceResultID:  3006,
				MonikerIDs:         datastructures.IDSetWith(4003, 4004)},
			2003: {
				StartLine:          3,
				StartCharacter:     4,
				EndLine:            5,
				EndCharacter:       6,
				DefinitionResultID: 3002,
				ReferenceResultID:  0,
				MonikerIDs:         datastructures.NewIDSet(),
			},
			2004: {
				StartLine:          4,
				StartCharacter:     5,
				EndLine:            6,
				EndCharacter:       7,
				DefinitionResultID: 0,
				ReferenceResultID:  3007,
				MonikerIDs:         datastructures.NewIDSet()},
			2005: {
				StartLine:          5,
				StartCharacter:     6,
				EndLine:            7,
				EndCharacter:       8,
				DefinitionResultID: 3003,
				ReferenceResultID:  0,
				MonikerIDs:         datastructures.NewIDSet(),
			},
			2006: {
				StartLine:          6,
				StartCharacter:     7,
				EndLine:            8,
				EndCharacter:       9,
				DefinitionResultID: 0,
				HoverResultID:      3008,
				MonikerIDs:         datastructures.NewIDSet()},
			2007: {
				StartLine:          7,
				StartCharacter:     8,
				EndLine:            9,
				EndCharacter:       0,
				DefinitionResultID: 3004,
				ReferenceResultID:  0,
				MonikerIDs:         datastructures.NewIDSet(),
			},
			2008: {
				StartLine:          8,
				StartCharacter:     9,
				EndLine:            0,
				EndCharacter:       1,
				DefinitionResultID: 0,
				HoverResultID:      3009,
				MonikerIDs:         datastructures.NewIDSet()},
			2009: {
				StartLine:          9,
				StartCharacter:     0,
				EndLine:            1,
				EndCharacter:       2,
				DefinitionResultID: 3005,
				ReferenceResultID:  0,
				MonikerIDs:         datastructures.NewIDSet(),
			},
		},
		DefinitionData: map[int]datastructures.DefaultIDSetMap{
			3001: {
				1001: datastructures.IDSetWith(2003),
				1002: datastructures.IDSetWith(2004),
				1003: datastructures.IDSetWith(2007),
			},
			3002: {
				1001: datastructures.IDSetWith(2002),
				1002: datastructures.IDSetWith(2005),
				1003: datastructures.IDSetWith(2008),
			},
			3003: {
				1001: datastructures.IDSetWith(2001),
				1002: datastructures.IDSetWith(2006),
				1003: datastructures.IDSetWith(2009),
			},
			3004: {
				1001: datastructures.IDSetWith(2003),
				1002: datastructures.IDSetWith(2005),
				1003: datastructures.IDSetWith(2007),
			},
			3005: {
				1001: datastructures.IDSetWith(2002),
				1002: datastructures.IDSetWith(2006),
				1003: datastructures.IDSetWith(2008),
			},
		},
		ReferenceData: map[int]datastructures.DefaultIDSetMap{
			3006: {
				1001: datastructures.IDSetWith(2003),
				1003: datastructures.IDSetWith(2007, 2009),
			},
			3007: {
				1001: datastructures.IDSetWith(2002),
				1003: datastructures.IDSetWith(2007, 2009),
			},
		},
		HoverData: map[int]string{
			3008: "foo",
			3009: "bar",
		},
		MonikerData: map[int]lsif.Moniker{
			4001: {
				Kind:                 "import",
				Scheme:               "scheme A",
				Identifier:           "ident A",
				PackageInformationID: 5001,
			},
			4002: {
				Kind:                 "import",
				Scheme:               "scheme B",
				Identifier:           "ident B",
				PackageInformationID: 0,
			},
			4003: {
				Kind:                 "export",
				Scheme:               "scheme C",
				Identifier:           "ident C",
				PackageInformationID: 5002,
			},
			4004: {
				Kind:                 "export",
				Scheme:               "scheme D",
				Identifier:           "ident D",
				PackageInformationID: 0,
			},
		},
		PackageInformationData: map[int]lsif.PackageInformation{
			5001: {
				Name:    "pkg A",
				Version: "0.1.0",
			},
			5002: {
				Name:    "pkg B",
				Version: "1.2.3",
			},
		},
		Diagnostics: map[int]lsif.DiagnosticResult{
			1001: {
				Result: []lsif.Diagnostic{
					{
						Severity:       1,
						Code:           "1234",
						Message:        "m1",
						Source:         "s1",
						StartLine:      11,
						StartCharacter: 12,
						EndLine:        13,
						EndCharacter:   14,
					},
				},
			},
			1002: {
				Result: []lsif.Diagnostic{
					{
						Severity:       2,
						Code:           "2",
						Message:        "m2",
						Source:         "s2",
						StartLine:      21,
						StartCharacter: 22,
						EndLine:        23,
						EndCharacter:   24,
					},
				},
			},
			1003: {
				Result: []lsif.Diagnostic{
					{
						Severity:       3,
						Code:           "3234",
						Message:        "m3",
						Source:         "s3",
						StartLine:      31,
						StartCharacter: 32,
						EndLine:        33,
						EndCharacter:   34,
					},
					{
						Severity:       4,
						Code:           "4234",
						Message:        "m4",
						Source:         "s4",
						StartLine:      41,
						StartCharacter: 42,
						EndLine:        43,
						EndCharacter:   44,
					},
				},
			},
		},
		ImportedMonikers: datastructures.IDSetWith(4001),
		ExportedMonikers: datastructures.IDSetWith(4003),
	}

	actualBundleData, err := groupBundleData(state, 42)
	if err != nil {
		t.Fatalf("unexpected error converting correlation state to types: %s", err)
	}
	// Ensure arrays have deterministic order so we can compare with a canned expected object structure
	normalizeGroupedBundleData(actualBundleData)

	expectedFilter, err := bloomfilter.CreateFilter([]string{"ident A"})
	if err != nil {
		t.Fatalf("unexpected error creating bloom filter: %s", err)
	}

	expectedBundleData := &GroupedBundleData{
		Meta: types.MetaData{
			NumResultChunks: 1,
		},
		Documents: map[string]types.DocumentData{
			"foo.go": {
				Ranges: map[types.ID]types.RangeData{
					"2001": {
						StartLine:          1,
						StartCharacter:     2,
						EndLine:            3,
						EndCharacter:       4,
						DefinitionResultID: "3001",
						ReferenceResultID:  "",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{"4001", "4002"},
					},
					"2002": {
						StartLine:          2,
						StartCharacter:     3,
						EndLine:            4,
						EndCharacter:       5,
						DefinitionResultID: "",
						ReferenceResultID:  "3006",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{"4003", "4004"},
					},
					"2003": {
						StartLine:          3,
						StartCharacter:     4,
						EndLine:            5,
						EndCharacter:       6,
						DefinitionResultID: "3002",
						ReferenceResultID:  "",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{},
					},
				},
				HoverResults: map[types.ID]string{},
				Monikers: map[types.ID]types.MonikerData{
					"4001": {
						Kind:                 "import",
						Scheme:               "scheme A",
						Identifier:           "ident A",
						PackageInformationID: "5001",
					},
					"4002": {
						Kind:                 "import",
						Scheme:               "scheme B",
						Identifier:           "ident B",
						PackageInformationID: "",
					},
					"4003": {
						Kind:                 "export",
						Scheme:               "scheme C",
						Identifier:           "ident C",
						PackageInformationID: "5002",
					},
					"4004": {
						Kind:                 "export",
						Scheme:               "scheme D",
						Identifier:           "ident D",
						PackageInformationID: "",
					},
				},
				PackageInformation: map[types.ID]types.PackageInformationData{
					"5001": {
						Name:    "pkg A",
						Version: "0.1.0",
					},
					"5002": {
						Name:    "pkg B",
						Version: "1.2.3",
					},
				},
				Diagnostics: []types.DiagnosticData{
					{
						Severity:       1,
						Code:           "1234",
						Message:        "m1",
						Source:         "s1",
						StartLine:      11,
						StartCharacter: 12,
						EndLine:        13,
						EndCharacter:   14,
					},
					{
						Severity:       2,
						Code:           "2",
						Message:        "m2",
						Source:         "s2",
						StartLine:      21,
						StartCharacter: 22,
						EndLine:        23,
						EndCharacter:   24,
					},
				},
			},
			"bar.go": {
				Ranges: map[types.ID]types.RangeData{
					"2004": {
						StartLine:          4,
						StartCharacter:     5,
						EndLine:            6,
						EndCharacter:       7,
						DefinitionResultID: "",
						ReferenceResultID:  "3007",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{},
					},
					"2005": {
						StartLine:          5,
						StartCharacter:     6,
						EndLine:            7,
						EndCharacter:       8,
						DefinitionResultID: "3003",
						ReferenceResultID:  "",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{},
					},
					"2006": {
						StartLine:          6,
						StartCharacter:     7,
						EndLine:            8,
						EndCharacter:       9,
						DefinitionResultID: "",
						ReferenceResultID:  "",
						HoverResultID:      "3008",
						MonikerIDs:         []types.ID{},
					},
				},
				HoverResults:       map[types.ID]string{"3008": "foo"},
				Monikers:           map[types.ID]types.MonikerData{},
				PackageInformation: map[types.ID]types.PackageInformationData{},
				Diagnostics: []types.DiagnosticData{
					{
						Severity:       3,
						Code:           "3234",
						Message:        "m3",
						Source:         "s3",
						StartLine:      31,
						StartCharacter: 32,
						EndLine:        33,
						EndCharacter:   34,
					},
					{
						Severity:       4,
						Code:           "4234",
						Message:        "m4",
						Source:         "s4",
						StartLine:      41,
						StartCharacter: 42,
						EndLine:        43,
						EndCharacter:   44,
					},
				},
			},
			"baz.go": {
				Ranges: map[types.ID]types.RangeData{
					"2007": {
						StartLine:          7,
						StartCharacter:     8,
						EndLine:            9,
						EndCharacter:       0,
						DefinitionResultID: "3004",
						ReferenceResultID:  "",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{},
					},
					"2008": {
						StartLine:          8,
						StartCharacter:     9,
						EndLine:            0,
						EndCharacter:       1,
						DefinitionResultID: "",
						ReferenceResultID:  "",
						HoverResultID:      "3009",
						MonikerIDs:         []types.ID{},
					},
					"2009": {
						StartLine:          9,
						StartCharacter:     0,
						EndLine:            1,
						EndCharacter:       2,
						DefinitionResultID: "3005",
						ReferenceResultID:  "",
						HoverResultID:      "",
						MonikerIDs:         []types.ID{},
					},
				},
				HoverResults:       map[types.ID]string{"3009": "bar"},
				Monikers:           map[types.ID]types.MonikerData{},
				PackageInformation: map[types.ID]types.PackageInformationData{},
				Diagnostics:        []types.DiagnosticData{},
			},
		},
		ResultChunks: map[int]types.ResultChunkData{
			0: {
				DocumentPaths: map[types.ID]string{
					"1001": "foo.go",
					"1002": "bar.go",
					"1003": "baz.go",
				},
				DocumentIDRangeIDs: map[types.ID][]types.DocumentIDRangeID{
					"3001": {
						{DocumentID: "1001", RangeID: "2003"},
						{DocumentID: "1002", RangeID: "2004"},
						{DocumentID: "1003", RangeID: "2007"},
					},
					"3002": {
						{DocumentID: "1001", RangeID: "2002"},
						{DocumentID: "1002", RangeID: "2005"},
						{DocumentID: "1003", RangeID: "2008"},
					},
					"3003": {
						{DocumentID: "1001", RangeID: "2001"},
						{DocumentID: "1002", RangeID: "2006"},
						{DocumentID: "1003", RangeID: "2009"},
					},
					"3004": {
						{DocumentID: "1001", RangeID: "2003"},
						{DocumentID: "1002", RangeID: "2005"},
						{DocumentID: "1003", RangeID: "2007"},
					},
					"3005": {
						{DocumentID: "1001", RangeID: "2002"},
						{DocumentID: "1002", RangeID: "2006"},
						{DocumentID: "1003", RangeID: "2008"},
					},
					"3006": {
						{DocumentID: "1001", RangeID: "2003"},
						{DocumentID: "1003", RangeID: "2007"},
						{DocumentID: "1003", RangeID: "2009"},
					},
					"3007": {
						{DocumentID: "1001", RangeID: "2002"},
						{DocumentID: "1003", RangeID: "2007"},
						{DocumentID: "1003", RangeID: "2009"},
					},
				},
			},
		},
		Definitions: []types.MonikerLocations{
			{
				Scheme:     "scheme A",
				Identifier: "ident A",
				Locations: []types.Location{
					{URI: "bar.go", StartLine: 4, StartCharacter: 5, EndLine: 6, EndCharacter: 7},
					{URI: "baz.go", StartLine: 7, StartCharacter: 8, EndLine: 9, EndCharacter: 0},
					{URI: "foo.go", StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6},
				},
			},
			{
				Scheme:     "scheme B",
				Identifier: "ident B",
				Locations: []types.Location{
					{URI: "bar.go", StartLine: 4, StartCharacter: 5, EndLine: 6, EndCharacter: 7},
					{URI: "baz.go", StartLine: 7, StartCharacter: 8, EndLine: 9, EndCharacter: 0},
					{URI: "foo.go", StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6},
				},
			},
		},
		References: []types.MonikerLocations{
			{
				Scheme:     "scheme C",
				Identifier: "ident C",
				Locations: []types.Location{
					{URI: "baz.go", StartLine: 7, StartCharacter: 8, EndLine: 9, EndCharacter: 0},
					{URI: "baz.go", StartLine: 9, StartCharacter: 0, EndLine: 1, EndCharacter: 2},
					{URI: "foo.go", StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6},
				},
			},
			{
				Scheme:     "scheme D",
				Identifier: "ident D",
				Locations: []types.Location{
					{URI: "baz.go", StartLine: 7, StartCharacter: 8, EndLine: 9, EndCharacter: 0},
					{URI: "baz.go", StartLine: 9, StartCharacter: 0, EndLine: 1, EndCharacter: 2},
					{URI: "foo.go", StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6},
				},
			},
		},
		Packages: []types.Package{
			{DumpID: 42, Scheme: "scheme C", Name: "pkg B", Version: "1.2.3"},
		},
		PackageReferences: []types.PackageReference{
			{DumpID: 42, Scheme: "scheme A", Name: "pkg A", Version: "0.1.0", Filter: expectedFilter},
		},
	}

	if diff := cmp.Diff(expectedBundleData, actualBundleData, datastructures.IDSetComparer); diff != "" {
		t.Errorf("unexpected bundle data (-want +got):\n%s", diff)
	}
}

//
//

func normalizeGroupedBundleData(groupedBundleData *GroupedBundleData) {
	for _, document := range groupedBundleData.Documents {
		sortDiagnostics(document.Diagnostics)

		for _, r := range document.Ranges {
			sortMonikerIDs(r.MonikerIDs)
		}
	}

	for _, resultChunk := range groupedBundleData.ResultChunks {
		for _, documentRanges := range resultChunk.DocumentIDRangeIDs {
			sortDocumentIDRangeIDs(documentRanges)
		}
	}

	sortMonikerLocations(groupedBundleData.Definitions)
	sortMonikerLocations(groupedBundleData.References)
}

func sortMonikerIDs(s []types.ID) {
	sort.Slice(s, func(i, j int) bool {
		return strings.Compare(string(s[i]), string(s[j])) < 0
	})
}

func sortDiagnostics(s []types.DiagnosticData) {
	sort.Slice(s, func(i, j int) bool {
		return strings.Compare(s[i].Message, s[j].Message) < 0
	})
}

func sortDocumentIDRangeIDs(s []types.DocumentIDRangeID) {
	sort.Slice(s, func(i, j int) bool {
		if cmp := strings.Compare(string(s[i].DocumentID), string(s[j].DocumentID)); cmp != 0 {
			return cmp < 0
		} else {
			return strings.Compare(string(s[i].RangeID), string(s[j].RangeID)) < 0
		}
	})
}

func sortMonikerLocations(monikerLocations []types.MonikerLocations) {
	sort.Slice(monikerLocations, func(i, j int) bool {
		if cmp := strings.Compare(monikerLocations[i].Scheme, monikerLocations[j].Scheme); cmp != 0 {
			return cmp < 0
		} else if cmp := strings.Compare(monikerLocations[i].Identifier, monikerLocations[j].Identifier); cmp != 0 {
			return cmp < 0
		}
		return false
	})

	for _, ml := range monikerLocations {
		sortLocations(ml.Locations)
	}
}

func sortLocations(locations []types.Location) {
	sort.Slice(locations, func(i, j int) bool {
		if cmp := strings.Compare(locations[i].URI, locations[j].URI); cmp != 0 {
			return cmp < 0
		}

		return locations[i].StartLine < locations[j].StartLine
	})
}
