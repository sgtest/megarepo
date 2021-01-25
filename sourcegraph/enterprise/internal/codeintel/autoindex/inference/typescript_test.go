package inference

import (
	"fmt"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
)

func TestLSIFTscJobRecognizerCanIndex(t *testing.T) {
	recognizer := lsifTscJobRecognizer{}
	testCases := []struct {
		paths    []string
		expected bool
	}{
		{paths: []string{"tsconfig.json"}, expected: true},
		{paths: []string{"a/tsconfig.json"}, expected: true},
		{paths: []string{"package.json"}, expected: false},
		{paths: []string{"node_modules/foo/bar/tsconfig.json"}, expected: false},
		{paths: []string{"foo/bar-tsconfig.json"}, expected: false},
	}

	for _, testCase := range testCases {
		name := strings.Join(testCase.paths, ", ")

		t.Run(name, func(t *testing.T) {
			if value := recognizer.CanIndex(testCase.paths, NewMockGitserverClientWrapper()); value != testCase.expected {
				t.Errorf("unexpected result from CanIndex. want=%v have=%v", testCase.expected, value)
			}
		})
	}
}

func TestLsifTscJobRecognizerInferIndexJobsTsConfigRoot(t *testing.T) {
	recognizer := lsifTscJobRecognizer{}
	paths := []string{
		"tsconfig.json",
	}

	expectedIndexJobs := []IndexJob{
		{
			DockerSteps: nil,
			Root:        "",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
	}
	if diff := cmp.Diff(expectedIndexJobs, recognizer.InferIndexJobs(paths, NewMockGitserverClientWrapper())); diff != "" {
		t.Errorf("unexpected index jobs (-want +got):\n%s", diff)
	}
}

func TestLsifTscJobRecognizerInferIndexJobsTsConfigSubdirs(t *testing.T) {
	recognizer := lsifTscJobRecognizer{}
	paths := []string{
		"a/tsconfig.json",
		"b/tsconfig.json",
		"c/tsconfig.json",
	}

	expectedIndexJobs := []IndexJob{
		{
			DockerSteps: nil,
			Root:        "a",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
		{
			DockerSteps: nil,
			Root:        "b",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
		{
			DockerSteps: nil,
			Root:        "c",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
	}
	if diff := cmp.Diff(expectedIndexJobs, recognizer.InferIndexJobs(paths, NewMockGitserverClientWrapper())); diff != "" {
		t.Errorf("unexpected index jobs (-want +got):\n%s", diff)
	}
}

func TestLsifTscJobRecognizerInferIndexJobsInstallSteps(t *testing.T) {
	recognizer := lsifTscJobRecognizer{}
	paths := []string{
		"tsconfig.json",
		"package.json",
		"foo/baz/tsconfig.json",
		"foo/bar/baz/tsconfig.json",
		"foo/bar/bonk/tsconfig.json",
		"foo/bar/bonk/package.json",
		"foo/bar/package.json",
		"foo/bar/yarn.lock",
	}

	expectedIndexJobs := []IndexJob{
		{
			DockerSteps: []DockerStep{
				{
					Root:     "",
					Image:    nodeInstallImage,
					Commands: []string{"npm install"},
				},
			},
			Root:        "",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
		{
			DockerSteps: []DockerStep{
				{
					Root:     "",
					Image:    nodeInstallImage,
					Commands: []string{"npm install"},
				},
			},
			Root:        "foo/baz",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
		{
			DockerSteps: []DockerStep{
				{
					Root:     "",
					Image:    nodeInstallImage,
					Commands: []string{"npm install"},
				},
				{
					Root:     "foo/bar",
					Image:    nodeInstallImage,
					Commands: []string{"yarn --ignore-engines"},
				},
			},
			Root:        "foo/bar/baz",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
		{
			DockerSteps: []DockerStep{
				{
					Root:     "",
					Image:    nodeInstallImage,
					Commands: []string{"npm install"},
				},
				{
					Root:     "foo/bar",
					Image:    nodeInstallImage,
					Commands: []string{"yarn --ignore-engines"},
				},
				{
					Root:     "foo/bar/bonk",
					Image:    nodeInstallImage,
					Commands: []string{"npm install"},
				},
			},
			Root:        "foo/bar/bonk",
			Indexer:     lsifTscImage,
			IndexerArgs: []string{"lsif-tsc", "-p", "."},
			Outfile:     "",
		},
	}
	if diff := cmp.Diff(expectedIndexJobs, recognizer.InferIndexJobs(paths, NewMockGitserverClientWrapper())); diff != "" {
		t.Errorf("unexpected index jobs (-want +got):\n%s", diff)
	}
}

func TestLSIFTscJobRecognizerPatterns(t *testing.T) {
	recognizer := lsifTscJobRecognizer{}
	paths := []string{
		"tsconfig.json",
		"subdir/tsconfig.json",
	}

	for _, path := range paths {
		match := false
		for _, pattern := range recognizer.Patterns() {
			if pattern.MatchString(path) {
				match = true
				break
			}
		}

		if !match {
			t.Error(fmt.Sprintf("failed to match %s", path))
		}
	}
}

func TestLSIFTscLernaConfig(t *testing.T) {
	mockGit := NewMockGitserverClientWrapper()
	mockGit.RawContentsFunc.PushReturn([]byte(`{"npmClient": "yarn"}`), nil)
	mockGit.RawContentsFunc.PushReturn([]byte(`{"npmClient": "npm"}`), nil)
	mockGit.RawContentsFunc.PushReturn([]byte(`{"npmClient": "yarn"}`), nil)

	recognizer := lsifTscJobRecognizer{}

	paths := [][]string{
		{
			"package.json",
			"lerna.json",
			"tsconfig.json",
		},
		{
			"package.json",
			"lerna.json",
			"tsconfig.json",
		},
		{
			"package.json",
			"tsconfig.json",
		},
		{
			"foo/package.json",
			"yarn.lock",
			"lerna.json",
			"package.json",
			"foo/bar/tsconfig.json",
		},
	}

	expectedJobs := [][]IndexJob{
		{
			{
				DockerSteps: []DockerStep{
					{
						Root:     "",
						Image:    "node:alpine3.12",
						Commands: []string{"yarn --ignore-engines"},
					},
				},
				LocalSteps:  nil,
				Root:        "",
				Indexer:     lsifTscImage,
				IndexerArgs: []string{"lsif-tsc", "-p", "."},
				Outfile:     "",
			},
		},
		{
			{
				DockerSteps: []DockerStep{
					{
						Root:     "",
						Image:    "node:alpine3.12",
						Commands: []string{"npm install"},
					},
				},
				LocalSteps:  nil,
				Root:        "",
				Indexer:     lsifTscImage,
				IndexerArgs: []string{"lsif-tsc", "-p", "."},
				Outfile:     "",
			},
		},
		{
			{
				DockerSteps: []DockerStep{
					{
						Root:     "",
						Image:    "node:alpine3.12",
						Commands: []string{"npm install"},
					},
				},
				LocalSteps:  nil,
				Root:        "",
				Indexer:     lsifTscImage,
				IndexerArgs: []string{"lsif-tsc", "-p", "."},
				Outfile:     "",
			},
		},
		{
			{
				DockerSteps: []DockerStep{
					{
						Root:     "",
						Image:    "node:alpine3.12",
						Commands: []string{"yarn --ignore-engines"},
					},
					{
						Root:     "foo",
						Image:    "node:alpine3.12",
						Commands: []string{"yarn --ignore-engines"},
					},
				},
				LocalSteps:  nil,
				Root:        "foo/bar",
				Indexer:     lsifTscImage,
				IndexerArgs: []string{"lsif-tsc", "-p", "."},
				Outfile:     "",
			},
		},
	}

	for i, paths := range paths {
		if diff := cmp.Diff(expectedJobs[i], recognizer.InferIndexJobs(paths, mockGit)); diff != "" {
			t.Errorf("unexpected index jobs (-want +got):\n%s", diff)
		}
	}
}
