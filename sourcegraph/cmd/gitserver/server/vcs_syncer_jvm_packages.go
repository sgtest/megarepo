package server

import (
	"archive/zip"
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"io"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/inconshreveable/log15"

	dependenciesStore "github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/store"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/jvmpackages/coursier"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

const (
	// DO NOT CHANGE. This timestamp needs to be stable so that JVM package
	// repos consistently produce the same git revhash. Sourcegraph URLs
	// can optionally include this hash, so changing the timestamp (and hence
	// hashes) will cause existing links to JVM package repos to return 404s.
	stableGitCommitDate = "Thu Apr 8 14:24:52 2021 +0200"

	jvmMajorVersion0 = 44
)

// placeholderMavenDependency is used to set GIT_AUTHOR_NAME for git commands
// that don't create commits or tags. The name of this dependency should never
// be publicly visible so it can have any random value.
var placeholderMavenDependency = func() *reposource.MavenDependency {
	d, err := reposource.ParseMavenDependency("com.sourcegraph:sourcegraph:1.0.0")
	if err != nil {
		panic(err)
	}
	return d
}()

type JVMPackagesSyncer struct {
	Config    *schema.JVMPackagesConnection
	DepsStore repos.DependenciesStore
}

var _ VCSSyncer = &JVMPackagesSyncer{}

func (s *JVMPackagesSyncer) MavenDependencies() []string {
	if s.Config == nil || s.Config.Maven == nil || s.Config.Maven.Dependencies == nil {
		return nil
	}
	return s.Config.Maven.Dependencies
}

func (s *JVMPackagesSyncer) Type() string {
	return "jvm_packages"
}

// IsCloneable checks to see if the VCS remote URL is cloneable. Any non-nil
// error indicates there is a problem.
func (s *JVMPackagesSyncer) IsCloneable(ctx context.Context, remoteURL *vcs.URL) error {
	dependencies, err := s.packageDependencies(ctx, remoteURL.Path)
	if err != nil {
		return err
	}

	for _, dependency := range dependencies {
		_, err := coursier.FetchSources(ctx, s.Config, dependency)
		if err != nil {
			// Temporary: We shouldn't need both these checks but we're continuing to see the
			// error in production logs which implies `Is` is not matching.
			if errors.HasType(err, &coursier.ErrNoSources{}) || strings.Contains(err.Error(), "no sources for dependency") {
				// We can't do anything and it's leading to increases in our
				// src_repoupdater_sched_error alert firing more often.
				continue
			}
			return err
		}
	}

	return nil
}

// CloneCommand returns the command to be executed for cloning from remote.
// There is no external tool that performs all the step for creating a JVM
// package repository so the actual cloning happens inside this method and the
// returned command is a no-op. This means that the web UI can't display a
// helpful progress bar while cloning JVM package repositories, but that's an
// acceptable tradeoff we're willing to make.
func (s *JVMPackagesSyncer) CloneCommand(ctx context.Context, remoteURL *vcs.URL, bareGitDirectory string) (*exec.Cmd, error) {
	err := os.MkdirAll(bareGitDirectory, 0755)
	if err != nil {
		return nil, err
	}

	cmd := exec.CommandContext(ctx, "git", "--bare", "init")
	if _, err := runCommandInDirectory(ctx, cmd, bareGitDirectory, placeholderMavenDependency); err != nil {
		return nil, err
	}

	// The Fetch method is responsible for cleaning up temporary directories.
	if err := s.Fetch(ctx, remoteURL, GitDir(bareGitDirectory)); err != nil {
		return nil, err
	}

	// no-op command to satisfy VCSSyncer interface, see docstring for more details.
	return exec.CommandContext(ctx, "git", "--version"), nil
}

// Fetch adds git tags for newly added dependency versions and removes git tags
// for deleted versions.
func (s *JVMPackagesSyncer) Fetch(ctx context.Context, remoteURL *vcs.URL, dir GitDir) error {
	dependencies, err := s.packageDependencies(ctx, remoteURL.Path)
	if err != nil {
		return err
	}

	tags := map[string]bool{}

	out, err := runCommandInDirectory(ctx, exec.CommandContext(ctx, "git", "tag"), string(dir), placeholderMavenDependency)
	if err != nil {
		return err
	}

	for _, line := range strings.Split(out, "\n") {
		if len(line) == 0 {
			continue
		}
		tags[line] = true
	}

	for i, dependency := range dependencies {
		if tags[dependency.GitTagFromVersion()] {
			continue
		}
		// the gitPushDependencyTag method is responsible for cleaning up temporary directories.
		if err := s.gitPushDependencyTag(ctx, string(dir), dependency, i == 0); err != nil {
			return errors.Wrapf(err, "error pushing dependency %q", dependency.PackageManagerSyntax())
		}
	}

	dependencyTags := make(map[string]struct{}, len(dependencies))
	for _, dependency := range dependencies {
		dependencyTags[dependency.GitTagFromVersion()] = struct{}{}
	}

	for tag := range tags {
		if _, isDependencyTag := dependencyTags[tag]; !isDependencyTag {
			cmd := exec.CommandContext(ctx, "git", "tag", "-d", tag)
			if _, err := runCommandInDirectory(ctx, cmd, string(dir), placeholderMavenDependency); err != nil {
				log15.Error("Failed to delete git tag", "error", err, "tag", tag)
				continue
			}
		}
	}

	return nil
}

// RemoteShowCommand returns the command to be executed for showing remote.
func (s *JVMPackagesSyncer) RemoteShowCommand(ctx context.Context, remoteURL *vcs.URL) (cmd *exec.Cmd, err error) {
	return exec.CommandContext(ctx, "git", "remote", "show", "./"), nil
}

// packageDependencies returns the list of JVM dependencies that belong to the given URL path.
// The returned package dependencies are sorted by semantic versioning.
// A URL maps to a single JVM package, which may contain multiple versions (one git tag per version).
func (s *JVMPackagesSyncer) packageDependencies(ctx context.Context, repoUrlPath string) (dependencies []*reposource.MavenDependency, err error) {
	module, err := reposource.ParseMavenModule(repoUrlPath)
	if err != nil {
		return nil, err
	}

	var (
		totalConfigMatched int
		timedout           []*reposource.MavenDependency
	)
	for _, dependency := range s.MavenDependencies() {
		if module.MatchesDependencyString(dependency) {
			dependency, err := reposource.ParseMavenDependency(dependency)
			if err != nil {
				return nil, err
			}

			exists, err := coursier.Exists(ctx, s.Config, dependency)
			if exists {
				totalConfigMatched++
				dependencies = append(dependencies, dependency)
			} else if errors.Is(err, context.DeadlineExceeded) {
				timedout = append(timedout, dependency)
			}
			// Silently ignore non-existent dependencies because
			// they are already logged out in the `GetRepo` method
			// in internal/repos/jvm_packages.go.
		}
	}

	if len(timedout) > 0 {
		log15.Warn("non-zero number of timed-out coursier invocations", "count", len(timedout), "dependencies", timedout)
	}

	dbDeps, err := s.DepsStore.ListDependencyRepos(ctx, dependenciesStore.ListDependencyReposOpts{
		Scheme: dependenciesStore.JVMPackagesScheme,
		Name:   repoUrlPath,
	})
	if err != nil {
		return nil, errors.Wrapf(err, "failed to get JVM dependency repos from database", "repoPath", repoUrlPath)
	}

	var totalDBMatched int
	for _, dep := range dbDeps {
		parsedModule, err := reposource.ParseMavenModule(dep.Name)
		if err != nil {
			log15.Warn("error parsing maven module", "error", err, "module", dep.Name)
			continue
		}
		if module.Equal(parsedModule) {
			dependency := &reposource.MavenDependency{
				MavenModule: parsedModule,
				Version:     dep.Version,
			}
			// we dont call coursier.Exists here, as existance should be verified by repo-updater
			totalDBMatched++
			dependencies = append(dependencies, dependency)
		}
	}

	if len(dependencies) == 0 {
		return nil, errors.Errorf("no JVM dependencies for URL path %s", repoUrlPath)
	}

	log15.Info("fetched maven artifact for repo path", "repoPath", repoUrlPath, "totalDB", totalDBMatched, "totalConfig", totalConfigMatched)
	reposource.SortDependencies(dependencies)
	return dependencies, nil
}

// gitPushDependencyTag pushes a git tag to the given bareGitDirectory path. The
// tag points to a commit that adds all sources of given dependency. When
// isMainBranch is true, the main branch of the bare git directory will also be
// updated to point to the same commit as the git tag.
func (s *JVMPackagesSyncer) gitPushDependencyTag(ctx context.Context, bareGitDirectory string, dependency *reposource.MavenDependency, isLatestVersion bool) error {
	tmpDirectory, err := os.MkdirTemp("", "maven")
	if err != nil {
		return err
	}
	// Always clean up created temporary directories.
	defer os.RemoveAll(tmpDirectory)

	sourceCodeJarPath, err := coursier.FetchSources(ctx, s.Config, dependency)
	if err != nil {
		return err
	}

	cmd := exec.CommandContext(ctx, "git", "init")
	if _, err := runCommandInDirectory(ctx, cmd, tmpDirectory, dependency); err != nil {
		return err
	}

	err = s.commitJar(ctx, dependency, tmpDirectory, sourceCodeJarPath, s.Config)
	if err != nil {
		return err
	}

	cmd = exec.CommandContext(ctx, "git", "remote", "add", "origin", bareGitDirectory)
	if _, err := runCommandInDirectory(ctx, cmd, tmpDirectory, dependency); err != nil {
		return err
	}

	// Use --no-verify for security reasons. See https://github.com/sourcegraph/sourcegraph/pull/23399
	cmd = exec.CommandContext(ctx, "git", "push", "--no-verify", "--force", "origin", "--tags")
	if _, err := runCommandInDirectory(ctx, cmd, tmpDirectory, dependency); err != nil {
		return err
	}

	if isLatestVersion {
		defaultBranch, err := runCommandInDirectory(ctx, exec.CommandContext(ctx, "git", "rev-parse", "--abbrev-ref", "HEAD"), tmpDirectory, dependency)
		if err != nil {
			return err
		}
		// Use --no-verify for security reasons. See https://github.com/sourcegraph/sourcegraph/pull/23399
		cmd = exec.CommandContext(ctx, "git", "push", "--no-verify", "--force", "origin", strings.TrimSpace(defaultBranch)+":latest", dependency.GitTagFromVersion())
		if _, err := runCommandInDirectory(ctx, cmd, tmpDirectory, dependency); err != nil {
			return err
		}
	}

	return nil
}

// commitJar creates a git commit in the given working directory that adds all the file contents of the given jar file.
// A `*.jar` file works the same way as a `*.zip` file, it can even be uncompressed with the `unzip` command-line tool.
func (s *JVMPackagesSyncer) commitJar(ctx context.Context, dependency *reposource.MavenDependency,
	workingDirectory, sourceCodeJarPath string, connection *schema.JVMPackagesConnection) error {
	if err := unzipJarFile(sourceCodeJarPath, workingDirectory); err != nil {
		return errors.Wrapf(err, "failed to unzip jar file for %s to %v", dependency.PackageManagerSyntax(), sourceCodeJarPath)
	}

	file, err := os.Create(filepath.Join(workingDirectory, "lsif-java.json"))
	if err != nil {
		return err
	}
	defer file.Close()

	jvmVersion, err := inferJVMVersionFromByteCode(ctx, connection, dependency)
	if err != nil {
		return err
	}

	// See [NOTE: LSIF-config-json] for details on why we use this JSON file.
	jsonContents, err := json.Marshal(&lsifJavaJSON{
		Kind:         dependency.MavenModule.LsifJavaKind(),
		JVM:          jvmVersion,
		Dependencies: dependency.LsifJavaDependencies(),
	})
	if err != nil {
		return err
	}

	_, err = file.Write(jsonContents)
	if err != nil {
		return err
	}

	cmd := exec.CommandContext(ctx, "git", "add", ".")
	if _, err := runCommandInDirectory(ctx, cmd, workingDirectory, dependency); err != nil {
		return err
	}

	// Use --no-verify for security reasons. See https://github.com/sourcegraph/sourcegraph/pull/23399
	cmd = exec.CommandContext(ctx, "git", "commit", "--no-verify", "-m", dependency.PackageManagerSyntax(), "--date", stableGitCommitDate)
	if _, err := runCommandInDirectory(ctx, cmd, workingDirectory, dependency); err != nil {
		return err
	}

	cmd = exec.CommandContext(ctx, "git", "tag", "-m", dependency.PackageManagerSyntax(), dependency.GitTagFromVersion())
	if _, err := runCommandInDirectory(ctx, cmd, workingDirectory, dependency); err != nil {
		return err
	}

	return nil
}

func unzipJarFile(jarPath, destination string) (err error) {
	reader, err := zip.OpenReader(jarPath)
	if err != nil {
		return err
	}
	defer reader.Close()
	destinationDirectory := strings.TrimSuffix(destination, string(os.PathSeparator)) + string(os.PathSeparator)

	for _, file := range reader.File {
		cleanedOutputPath, isPotentiallyMalicious :=
			isPotentiallyMaliciousFilepathInArchive(file.Name, destinationDirectory)
		if isPotentiallyMalicious {
			continue
		}
		err := copyZipFileEntry(file, cleanedOutputPath)
		if err != nil {
			return err
		}
	}

	return nil
}

func copyZipFileEntry(entry *zip.File, outputPath string) (err error) {
	inputFile, err := entry.Open()
	if err != nil {
		return err
	}
	defer func() {
		err1 := inputFile.Close()
		if err == nil {
			err = err1
		}
	}()

	if err = os.MkdirAll(path.Dir(outputPath), 0700); err != nil {
		return err
	}
	outputFile, err := os.OpenFile(outputPath, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0600)
	if err != nil {
		return err
	}
	defer func() {
		err1 := outputFile.Close()
		if err == nil {
			err = err1
		}
	}()

	_, err = io.Copy(outputFile, inputFile)
	return err
}

// inferJVMVersionFromByteCode returns the JVM version that was used to compile
// the bytecode in the given jar file.
func inferJVMVersionFromByteCode(ctx context.Context, connection *schema.JVMPackagesConnection,
	dependency *reposource.MavenDependency) (string, error) {
	if dependency.IsJDK() {
		return dependency.Version, nil
	}

	byteCodeJarPath, err := coursier.FetchByteCode(ctx, connection, dependency)
	if err != nil {
		return "", err
	}
	majorVersionString, err := classFileMajorVersion(byteCodeJarPath)
	if err != nil {
		return "", err
	}
	majorVersion, err := strconv.Atoi(majorVersionString)
	if err != nil {
		return "", errors.Wrapf(err, "failed to convert string %s to int", majorVersion)
	}

	// Java 1.1 (aka "Java 1") has major version 45 and Java 8 has major
	// version 52. To go from the major version of Java version we subtract
	// 44.
	jvmVersion := majorVersion - jvmMajorVersion0

	// The motivation to round the JVM version to the nearst stable release
	// is so that we reduce the number of JDKs on sourcegraph.com. By having
	// fewer JDK versions, features like "find references" will return
	// aggregated results for non-LTS releases.
	roundedJvmVersion := roundJVMVersionToNearestStableVersion(jvmVersion)

	return strconv.Itoa(roundedJvmVersion), nil
}

// roundJVMVersionToNearestStableVersion returns the oldest stable JVM version
// that is compatible with the given version. Java uses a time-based release
// schedule since Java 11. A new major version is released every 6 month and
// every 6th release is an LTS release. This means that a new LTS release gets
// published every 3rd year.  See
// https://www.baeldung.com/java-time-based-releases for more details.  This
// method rounds up non-LTS versions to the nearest LTS release. For example, a
// library that's published for Java 10 should be indexed with Java 11.
func roundJVMVersionToNearestStableVersion(javaVersion int) int {
	if javaVersion <= 8 {
		return 8
	}
	if javaVersion <= 11 {
		return 11
	}
	// TODO: bump this up to 17 once Java 17 LTS has been released, see https://github.com/sourcegraph/lsif-java/issues/263
	if javaVersion <= 16 {
		return 16
	}
	// Version from the future, do not round up to the next stable release.
	return javaVersion
}

type lsifJavaJSON struct {
	Kind         string   `json:"kind"`
	JVM          string   `json:"jvm"`
	Dependencies []string `json:"dependencies"`
}

// classFileMajorVersion returns the "major" version of the first `*.class` file
// inside the given jar file. For example, a jar file for a Java 8 library has
// the major version 52.
func classFileMajorVersion(byteCodeJarPath string) (string, error) {
	file, err := os.OpenFile(byteCodeJarPath, os.O_RDONLY, 0644)
	if err != nil {
		return "", err
	}
	defer file.Close()

	stat, err := os.Stat(byteCodeJarPath)
	if err != nil {
		return "", err
	}
	zipReader, err := zip.NewReader(file, stat.Size())
	if err != nil {
		return "", errors.Wrapf(err, "failed to read jar file %s", byteCodeJarPath)
	}

	for _, zipEntry := range zipReader.File {
		if !strings.HasSuffix(zipEntry.Name, ".class") {
			continue
		}
		version, err := classFileEntryMajorVersion(byteCodeJarPath, zipEntry)
		if err != nil {
			return "", nil
		}
		if version == "" {
			continue // Not a classfile
		}
		return version, nil
	}

	// We didn't find any `*.class` files so we can use any Java version.
	// Maven don't have to contain classfiles, some artifacts like
	// 'io.smallrye:smallrye-health-ui:3.1.1' only contain HTML/css/png/js
	// files.
	return "8", nil
}

func classFileEntryMajorVersion(byteCodeJarPath string, zipEntry *zip.File) (string, error) {
	classFileReader, err := zipEntry.Open()
	if err != nil {
		return "", err
	}

	magicBytes := make([]byte, 8)
	read, err := classFileReader.Read(magicBytes)
	defer classFileReader.Close()
	if err != nil {
		return "", err
	}
	if read != len(magicBytes) {
		return "", errors.Errorf("failed to read 8 bytes from file %s", byteCodeJarPath)
	}

	// The structure of `*.class` files is documented here
	// https://docs.oracle.com/javase/specs/jvms/se16/html/jvms-4.html#jvms-4.1 and also
	// https://en.wikipedia.org/wiki/Java_class_file#General_layout
	// - Bytes 0-4 must match 0xcafebabe.
	// - Bytes 4-5 represent the uint16 formatted "minor" version.
	// - Bytes 5-6 represent the uint16 formatted "major" version.
	// We're only interested in the major version.
	var cafebabe uint32
	var minor uint16
	var major uint16
	buf := bytes.NewReader(magicBytes)
	binary.Read(buf, binary.BigEndian, &cafebabe)
	if cafebabe != 0xcafebabe {
		return "", nil // Not a classfile
	}
	binary.Read(buf, binary.BigEndian, &minor)
	binary.Read(buf, binary.BigEndian, &major)
	return strconv.Itoa(int(major)), nil
}
