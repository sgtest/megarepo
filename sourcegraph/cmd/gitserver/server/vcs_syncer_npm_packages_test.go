package server

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path"
	"reflect"
	"strconv"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/live"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/npm"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/npm/npmtest"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

const (
	exampleTSFilepath           = "Example.ts"
	exampleJSFilepath           = "Example.js"
	exampleTSFileContents       = "export X; interface X { x: number }"
	exampleJSFileContents       = "var x = 1; var y = 'hello'; x = y;"
	exampleNpmVersion           = "1.0.0"
	exampleNpmVersion2          = "2.0.0-abc"
	exampleNpmVersionedPackage  = "example@1.0.0"
	exampleNpmVersionedPackage2 = "example@2.0.0-abc"
	exampleTgz                  = "example-1.0.0.tgz"
	exampleTgz2                 = "example-2.0.0-abc.tgz"
	exampleNpmPackageURL        = "npm/example"
)

func TestNoMaliciousFilesNpm(t *testing.T) {
	dir := t.TempDir()

	extractPath := path.Join(dir, "extracted")
	assert.Nil(t, os.Mkdir(extractPath, os.ModePerm))

	tgz := bytes.NewReader(createMaliciousTgz(t))

	err := decompressTgz(tgz, extractPath)
	assert.Nil(t, err) // Malicious files are skipped

	dirEntries, err := os.ReadDir(extractPath)
	baseline := []string{"harmless.java"}
	assert.Nil(t, err)
	paths := []string{}
	for _, dirEntry := range dirEntries {
		paths = append(paths, dirEntry.Name())
	}
	if !reflect.DeepEqual(baseline, paths) {
		t.Errorf("expected paths: %v\n   found paths:%v", baseline, paths)
	}
}

func createMaliciousTgz(t *testing.T) []byte {
	fileInfos := []fileInfo{
		{harmlessPath, []byte("harmless")},
	}
	for _, filepath := range maliciousPaths {
		fileInfos = append(fileInfos, fileInfo{filepath, []byte("malicious")})
	}
	return createTgz(t, fileInfos)
}

func TestNpmCloneCommand(t *testing.T) {
	dir := t.TempDir()

	tgz1 := createTgz(t, []fileInfo{{exampleJSFilepath, []byte(exampleJSFileContents)}})
	tgz2 := createTgz(t, []fileInfo{{exampleTSFilepath, []byte(exampleTSFileContents)}})

	client := npmtest.MockClient{
		Packages: map[string]*npm.PackageInfo{
			"example": {
				Versions: map[string]*npm.DependencyInfo{
					exampleNpmVersion: {
						Dist: npm.DependencyInfoDist{TarballURL: exampleNpmVersion},
					},
					exampleNpmVersion2: {
						Dist: npm.DependencyInfoDist{TarballURL: exampleNpmVersion2},
					},
				},
			},
		},
		Tarballs: map[string][]byte{
			exampleNpmVersion:  tgz1,
			exampleNpmVersion2: tgz2,
		},
	}

	depsSvc := live.TestService(database.NewDB(dbtest.NewDB(t)), nil)

	s := NewNpmPackagesSyncer(
		schema.NpmPackagesConnection{Dependencies: []string{}},
		depsSvc,
		&client,
		"urn",
	).(*vcsDependenciesSyncer)

	bareGitDirectory := path.Join(dir, "git")
	s.runCloneCommand(t, exampleNpmPackageURL, bareGitDirectory, []string{exampleNpmVersionedPackage})
	checkSingleTag := func() {
		assertCommandOutput(t,
			exec.Command("git", "tag", "--list"),
			bareGitDirectory,
			fmt.Sprintf("v%s\n", exampleNpmVersion))
		assertCommandOutput(t,
			exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleNpmVersion, exampleJSFilepath)),
			bareGitDirectory,
			exampleJSFileContents,
		)
	}
	checkSingleTag()

	s.runCloneCommand(t, exampleNpmPackageURL, bareGitDirectory, []string{exampleNpmVersionedPackage, exampleNpmVersionedPackage2})
	checkTagAdded := func() {
		assertCommandOutput(t,
			exec.Command("git", "tag", "--list"),
			bareGitDirectory,
			fmt.Sprintf("v%s\nv%s\n", exampleNpmVersion, exampleNpmVersion2), // verify that a new tag was added
		)
		assertCommandOutput(t,
			exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleNpmVersion, exampleJSFilepath)),
			bareGitDirectory,
			exampleJSFileContents,
		)
		assertCommandOutput(t,
			exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleNpmVersion2, exampleTSFilepath)),
			bareGitDirectory,
			exampleTSFileContents,
		)
	}
	checkTagAdded()

	s.runCloneCommand(t, exampleNpmPackageURL, bareGitDirectory, []string{exampleNpmVersionedPackage})
	checkTagRemoved := func() {
		assertCommandOutput(t,
			exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleNpmVersion, exampleJSFilepath)),
			bareGitDirectory,
			exampleJSFileContents,
		)
		assertCommandOutput(t,
			exec.Command("git", "tag", "--list"),
			bareGitDirectory,
			fmt.Sprintf("v%s\n", exampleNpmVersion), // verify that second tag has been removed.
		)
	}
	checkTagRemoved()

	// Now run the same tests with the database output instead.
	if _, err := depsSvc.UpsertDependencyRepos(context.Background(), []dependencies.Repo{
		{
			ID:      1,
			Scheme:  dependencies.NpmPackagesScheme,
			Name:    "example",
			Version: exampleNpmVersion,
		},
	}); err != nil {
		t.Fatalf(err.Error())
	}
	s.runCloneCommand(t, exampleNpmPackageURL, bareGitDirectory, []string{})
	checkSingleTag()

	if _, err := depsSvc.UpsertDependencyRepos(context.Background(), []dependencies.Repo{
		{
			ID:      2,
			Scheme:  dependencies.NpmPackagesScheme,
			Name:    "example",
			Version: exampleNpmVersion2,
		},
	}); err != nil {
		t.Fatalf(err.Error())
	}
	s.runCloneCommand(t, exampleNpmPackageURL, bareGitDirectory, []string{})
	checkTagAdded()

	if err := depsSvc.DeleteDependencyReposByID(context.Background(), 2); err != nil {
		t.Fatalf(err.Error())
	}
	s.runCloneCommand(t, exampleNpmPackageURL, bareGitDirectory, []string{})
	checkTagRemoved()
}

func createTgz(t *testing.T, fileInfos []fileInfo) []byte {
	t.Helper()

	var buf bytes.Buffer
	gzipWriter := gzip.NewWriter(&buf)
	tarWriter := tar.NewWriter(gzipWriter)

	for _, fileinfo := range fileInfos {
		require.NoError(t, addFileToTarball(t, tarWriter, fileinfo))
	}

	require.NoError(t, tarWriter.Close())
	require.NoError(t, gzipWriter.Close())

	return buf.Bytes()
}

func addFileToTarball(t *testing.T, tarWriter *tar.Writer, info fileInfo) error {
	t.Helper()
	header, err := tar.FileInfoHeader(&info, "")
	if err != nil {
		return err
	}
	header.Name = info.path
	if err = tarWriter.WriteHeader(header); err != nil {
		return errors.Wrapf(err, "failed to write header for %s", info.path)
	}
	_, err = tarWriter.Write(info.contents)
	return err
}

type fileInfo struct {
	path     string
	contents []byte
}

var _ fs.FileInfo = &fileInfo{}

func (info *fileInfo) Name() string       { return path.Base(info.path) }
func (info *fileInfo) Size() int64        { return int64(len(info.contents)) }
func (info *fileInfo) Mode() fs.FileMode  { return 0600 }
func (info *fileInfo) ModTime() time.Time { return time.Unix(0, 0) }
func (info *fileInfo) IsDir() bool        { return false }
func (info *fileInfo) Sys() any           { return nil }

func TestDecompressTgz(t *testing.T) {
	table := []struct {
		paths  []string
		expect []string
	}{
		// Check that stripping the outermost shared directory works if all
		// paths have a common outermost directory.
		{[]string{"d/f1", "d/f2"}, []string{"f1", "f2"}},
		{[]string{"d1/d2/f1", "d1/d2/f2"}, []string{"d2"}},
		{[]string{"d1/f1", "d2/f2", "d3/f3"}, []string{"d1", "d2", "d3"}},
		{[]string{"f1", "d1/f2", "d1/f3"}, []string{"d1", "f1"}},
	}

	for i, testData := range table {
		t.Run(strconv.Itoa(i), func(t *testing.T) {
			dir := t.TempDir()

			var fileInfos []fileInfo
			for _, path := range testData.paths {
				fileInfos = append(fileInfos, fileInfo{path: path, contents: []byte("x")})
			}

			tgz := bytes.NewReader(createTgz(t, fileInfos))

			require.NoError(t, decompressTgz(tgz, dir))

			have, err := fs.Glob(os.DirFS(dir), "*")
			require.NoError(t, err)

			require.Equal(t, testData.expect, have)
		})
	}
}

// Regression test for: https://github.com/sourcegraph/sourcegraph/issues/30554
func TestDecompressTgzNoOOB(t *testing.T) {
	testCases := [][]tar.Header{
		{
			{Typeflag: tar.TypeDir, Name: "non-empty"},
			{Typeflag: tar.TypeReg, Name: "non-empty/f1"},
		},
		{
			{Typeflag: tar.TypeDir, Name: "empty"},
			{Typeflag: tar.TypeReg, Name: "non-empty/f1"},
		},
		{
			{Typeflag: tar.TypeDir, Name: "empty"},
			{Typeflag: tar.TypeDir, Name: "non-empty/"},
			{Typeflag: tar.TypeReg, Name: "non-empty/f1"},
		},
	}

	for _, testCase := range testCases {
		testDecompressTgzNoOOBImpl(t, testCase)
	}
}

func testDecompressTgzNoOOBImpl(t *testing.T, entries []tar.Header) {
	buffer := bytes.NewBuffer([]byte{})

	gzipWriter := gzip.NewWriter(buffer)
	tarWriter := tar.NewWriter(gzipWriter)
	for _, entry := range entries {
		tarWriter.WriteHeader(&entry)
		if entry.Typeflag == tar.TypeReg {
			tarWriter.Write([]byte("filler"))
		}
	}
	tarWriter.Close()
	gzipWriter.Close()

	reader := bytes.NewReader(buffer.Bytes())

	outDir := t.TempDir()

	require.NotPanics(t, func() {
		decompressTgz(reader, outDir)
	})
}
