package server

import (
	"archive/zip"
	"context"
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"path"
	"reflect"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/jvmpackages/coursier"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

const (
	exampleJar               = "sources.jar"
	exampleByteCodeJar       = "bytes.jar"
	exampleJar2              = "sources2.jar"
	exampleByteCodeJar2      = "bytes2.jar"
	exampleFilePath          = "Example.java"
	exampleClassfilePath     = "Example.class"
	exampleFileContents      = "package example;\npublic class Example {}\n"
	exampleFileContents2     = "package example;\npublic class Example { public static final int x = 42; }\n"
	exampleVersion           = "1.0.0"
	exampleVersion2          = "2.0.0"
	exampleVersionedPackage  = "org.example:example:1.0.0"
	exampleVersionedPackage2 = "org.example:example:2.0.0"
	examplePackageUrl        = "maven/org.example/example"

	// These magic numbers come from the table here https://en.wikipedia.org/wiki/Java_class_file#General_layout
	java5MajorVersion  = 49
	java11MajorVersion = 53
)

func createPlaceholderJar(t *testing.T, dir string, contents []byte, jarName, contentPath string) {
	t.Helper()
	jarPath, err := os.Create(path.Join(dir, jarName))
	assert.Nil(t, err)
	zipWriter := zip.NewWriter(jarPath)
	exampleWriter, err := zipWriter.Create(contentPath)
	assert.Nil(t, err)
	_, err = exampleWriter.Write(contents)
	assert.Nil(t, err)
	assert.Nil(t, zipWriter.Close())
	assert.Nil(t, jarPath.Close())
}

func createPlaceholderSourcesJar(t *testing.T, dir, contents, jarName string) {
	t.Helper()
	createPlaceholderJar(t, dir, []byte(contents), jarName, exampleFilePath)
}

func createPlaceholderByteCodeJar(t *testing.T, contents []byte, dir, jarName string) {
	t.Helper()
	createPlaceholderJar(t, dir, contents, jarName, exampleClassfilePath)
}

func assertCommandOutput(t *testing.T, cmd *exec.Cmd, workingDir, expectedOut string) {
	t.Helper()
	cmd.Dir = workingDir
	showOut, err := cmd.Output()
	assert.Nil(t, errors.Wrapf(err, "cmd=%q", cmd))
	if string(showOut) != expectedOut {
		t.Fatalf("got %q, want %q", showOut, expectedOut)
	}
}

func coursierScript(t *testing.T, dir string) string {
	coursierPath, err := os.OpenFile(path.Join(dir, "coursier"), os.O_CREATE|os.O_RDWR, 07777)
	assert.Nil(t, err)
	defer coursierPath.Close()
	script := fmt.Sprintf(`#!/usr/bin/env bash
ARG="$5"
CLASSIFIER="$7"
if [[ "$ARG" =~ "%s" ]]; then
  if [[ "$CLASSIFIER" =~ "sources" ]]; then
    echo "%s"
  else
    echo "%s"
  fi
elif [[ "$ARG" =~ "%s" ]]; then
  if [[ "$CLASSIFIER" =~ "sources" ]]; then
    echo "%s"
  else
    echo "%s"
  fi
else
  echo "Invalid argument $1"
  exit 1
fi
`,
		exampleVersion, path.Join(dir, exampleJar), path.Join(dir, exampleByteCodeJar),
		exampleVersion2, path.Join(dir, exampleJar2), path.Join(dir, exampleByteCodeJar2),
	)
	_, err = coursierPath.WriteString(script)
	assert.Nil(t, err)
	return coursierPath.Name()
}

func (s JVMPackagesSyncer) runCloneCommand(t *testing.T, bareGitDirectory string, dependencies []string) {
	url := vcs.URL{
		URL: url.URL{Path: examplePackageUrl},
	}
	s.Config.Maven.Dependencies = dependencies
	cmd, err := s.CloneCommand(context.Background(), &url, bareGitDirectory)
	assert.Nil(t, err)
	assert.Nil(t, cmd.Run())
}

var maliciousPaths []string = []string{
	// Absolute paths
	"/sh", "/usr/bin/sh",
	// Paths into .git which may trigger when git runs a hook
	".git/blah", ".git/hooks/pre-commit",
	// Relative paths which stray outside
	"../foo/../bar", "../../../usr/bin/sh",
}

const harmlessPath = "src/harmless.java"

func TestNoMaliciousFiles(t *testing.T) {
	dir, err := os.MkdirTemp("", "")
	assert.Nil(t, err)
	defer os.RemoveAll(dir)

	jarPath := path.Join(dir, "sampletext.zip")
	extractPath := path.Join(dir, "extracted")
	assert.Nil(t, os.Mkdir(extractPath, os.ModePerm))

	createMaliciousJar(t, jarPath)

	s := JVMPackagesSyncer{
		Config:  &schema.JVMPackagesConnection{Maven: &schema.Maven{Dependencies: []string{}}},
		DBStore: &simpleJVMPackageDBStoreMock{},
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // cancel now  to prevent any network IO
	dep := &reposource.MavenDependency{MavenModule: &reposource.MavenModule{}}
	err = s.commitJar(ctx, dep, extractPath, jarPath, &schema.JVMPackagesConnection{Maven: &schema.Maven{}})
	assert.NotNil(t, err)

	dirEntries, err := os.ReadDir(extractPath)
	baseline := map[string]int{"lsif-java.json": 0, strings.Split(harmlessPath, string(os.PathSeparator))[0]: 0}
	assert.Nil(t, err)
	paths := map[string]int{}
	for _, dirEntry := range dirEntries {
		paths[dirEntry.Name()] = 0
	}
	if !reflect.DeepEqual(baseline, paths) {
		t.Errorf("expected paths: %v\n   found paths:%v", baseline, paths)
	}
}

func createMaliciousJar(t *testing.T, name string) {
	f, err := os.Create(name)
	assert.Nil(t, err)
	defer f.Close()
	writer := zip.NewWriter(f)
	defer writer.Close()

	for _, filepath := range maliciousPaths {
		_, err = writer.Create(filepath)
		assert.Nil(t, err)
	}
	_, err = writer.Create(harmlessPath)
	assert.Nil(t, err)
}

func TestJVMCloneCommand(t *testing.T) {
	dir, err := os.MkdirTemp("", "")
	assert.Nil(t, err)
	defer os.RemoveAll(dir)

	createPlaceholderSourcesJar(t, dir, exampleFileContents, exampleJar)
	createPlaceholderByteCodeJar(t,
		[]byte{0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, java5MajorVersion, 0xab}, dir, exampleByteCodeJar)
	createPlaceholderSourcesJar(t, dir, exampleFileContents2, exampleJar2)
	createPlaceholderByteCodeJar(t,
		[]byte{0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, java11MajorVersion, 0xab}, dir, exampleByteCodeJar2)

	coursier.CoursierBinary = coursierScript(t, dir)

	s := JVMPackagesSyncer{
		Config:  &schema.JVMPackagesConnection{Maven: &schema.Maven{Dependencies: []string{}}},
		DBStore: &simpleJVMPackageDBStoreMock{},
	}
	bareGitDirectory := path.Join(dir, "git")

	s.runCloneCommand(t, bareGitDirectory, []string{exampleVersionedPackage})
	assertCommandOutput(t,
		exec.Command("git", "tag", "--list"),
		bareGitDirectory,
		"v1.0.0\n",
	)
	assertCommandOutput(t,
		exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleVersion, exampleFilePath)),
		bareGitDirectory,
		exampleFileContents,
	)

	s.runCloneCommand(t, bareGitDirectory, []string{exampleVersionedPackage, exampleVersionedPackage2})
	assertCommandOutput(t,
		exec.Command("git", "tag", "--list"),
		bareGitDirectory,
		"v1.0.0\nv2.0.0\n", // verify that the v2.0.0 tag got added
	)

	assertCommandOutput(t,
		exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleVersion, "lsif-java.json")),
		bareGitDirectory,
		// Assert that Java 8 is used for a library compiled with Java 5.
		fmt.Sprintf(`{"kind":"maven","jvm":"%s","dependencies":["%s"]}`, "8", exampleVersionedPackage),
	)
	assertCommandOutput(t,
		exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleVersion2, "lsif-java.json")),
		bareGitDirectory,
		// Assert that Java 11 is used for a library compiled with Java 11.
		fmt.Sprintf(`{"kind":"maven","jvm":"%s","dependencies":["%s"]}`, "11", exampleVersionedPackage2),
	)

	assertCommandOutput(t,
		exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleVersion, exampleFilePath)),
		bareGitDirectory,
		exampleFileContents,
	)

	assertCommandOutput(t,
		exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleVersion2, exampleFilePath)),
		bareGitDirectory,
		exampleFileContents2,
	)

	s.runCloneCommand(t, bareGitDirectory, []string{exampleVersionedPackage})
	assertCommandOutput(t,
		exec.Command("git", "show", fmt.Sprintf("v%s:%s", exampleVersion, exampleFilePath)),
		bareGitDirectory,
		exampleFileContents,
	)
	assertCommandOutput(t,
		exec.Command("git", "tag", "--list"),
		bareGitDirectory,
		"v1.0.0\n", // verify that the v2.0.0 tag has been removed.
	)
}

type simpleJVMPackageDBStoreMock struct{}

func (m *simpleJVMPackageDBStoreMock) GetJVMDependencyRepos(ctx context.Context, filter dbstore.GetJVMDependencyReposOpts) ([]dbstore.JVMDependencyRepo, error) {
	return []dbstore.JVMDependencyRepo{}, nil
}

// Sanity check errors.HasType
func TestErrorHasType(t *testing.T) {
	err := &coursier.ErrNoSources{}
	if !errors.HasType(err, &coursier.ErrNoSources{}) {
		t.Fatal("should be true")
	}
	if errors.Is(nil, &coursier.ErrNoSources{}) {
		t.Fatal("should be false")
	}
}
