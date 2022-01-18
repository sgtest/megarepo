package git

import (
	"bytes"
	"context"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
)

func TestRepository_FileSystem_Symlinks(t *testing.T) {
	t.Parallel()

	gitCommands := []string{
		"touch file1",
		"mkdir dir1",
		"ln -s file1 link1",
		"ln -s ../file1 dir1/link2",
		"touch --date=2006-01-02T15:04:05Z file1 link1 dir1/link2 || touch -t " + Times[0] + " file1 link1 dir1/link2",
		"git add link1 file1 dir1/link2",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}

	// map of path to size of content
	symlinks := map[string]int64{
		"link1":      5, // file1
		"dir1/link2": 8, // ../file1
	}

	dir := InitGitRepository(t, gitCommands...)
	repo := api.RepoName(filepath.Base(dir))

	if resp, err := gitserver.DefaultClient.RequestRepoUpdate(context.Background(), repo, 0); err != nil {
		t.Fatal(err)
	} else if resp.Error != "" {
		t.Fatal(resp.Error)
	}

	commitID := api.CommitID(ComputeCommitHash(dir, true))

	ctx := context.Background()

	// file1 should be a file.
	file1Info, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, repo, commitID, "file1")
	if err != nil {
		t.Fatalf("fs.Stat(file1): %s", err)
	}
	if !file1Info.Mode().IsRegular() {
		t.Errorf("file1 Stat !IsRegular (mode: %o)", file1Info.Mode())
	}

	checkSymlinkFileInfo := func(name string, link fs.FileInfo) {
		t.Helper()
		if link.Mode()&os.ModeSymlink == 0 {
			t.Errorf("link mode is not symlink (mode: %o)", link.Mode())
		}
		if link.Name() != name {
			t.Errorf("got link.Name() == %q, want %q", link.Name(), name)
		}
	}

	// Check symlinks are links
	for symlink := range symlinks {
		fi, err := lStat(ctx, authz.DefaultSubRepoPermsChecker, repo, commitID, symlink)
		if err != nil {
			t.Fatalf("fs.lStat(%s): %s", symlink, err)
		}
		if runtime.GOOS != "windows" {
			// TODO(alexsaveliev) make it work on Windows too
			checkSymlinkFileInfo(symlink, fi)
		}
	}

	// Also check the FileInfo returned by ReadDir to ensure it's
	// consistent with the FileInfo returned by lStat.
	entries, err := ReadDir(ctx, authz.DefaultSubRepoPermsChecker, repo, commitID, ".", false)
	if err != nil {
		t.Fatalf("fs.ReadDir(.): %s", err)
	}
	found := false
	for _, entry := range entries {
		if entry.Name() == "link1" {
			found = true
			if runtime.GOOS != "windows" {
				checkSymlinkFileInfo("link1", entry)
			}
		}
	}
	if !found {
		t.Fatal("readdir did not return link1")
	}

	for symlink, size := range symlinks {
		fi, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, repo, commitID, symlink)
		if err != nil {
			t.Fatalf("fs.Stat(%s): %s", symlink, err)
		}
		if fi.Mode()&fs.ModeSymlink == 0 {
			t.Errorf("%s Stat is not a symlink (mode: %o)", symlink, fi.Mode())
		}
		if fi.Name() != symlink {
			t.Errorf("got Name %q, want %q", fi.Name(), symlink)
		}
		if fi.Size() != size {
			t.Errorf("got %s Size %d, want %d", symlink, fi.Size(), size)
		}
	}
}

func TestRepository_FileSystem(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	// In all tests, repo should contain three commits. The first commit
	// (whose ID is in the 'first' field) has a file at dir1/file1 with the
	// contents "myfile1" and the mtime 2006-01-02T15:04:05Z. The second
	// commit (whose ID is in the 'second' field) adds a file at file2 (in the
	// top-level directory of the repository) with the contents "infile2" and
	// the mtime 2014-05-06T19:20:21Z. The third commit contains an empty
	// tree.
	//
	// TODO(sqs): add symlinks, etc.
	gitCommands := []string{
		"mkdir dir1",
		"echo -n infile1 > dir1/file1",
		"touch --date=2006-01-02T15:04:05Z dir1 dir1/file1 || touch -t " + Times[0] + " dir1 dir1/file1",
		"git add dir1/file1",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
		"echo -n infile2 > 'file 2'",
		"touch --date=2014-05-06T19:20:21Z 'file 2' || touch -t " + Times[1] + " 'file 2'",
		"git add 'file 2'",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2014-05-06T19:20:21Z git commit -m commit2 --author='a <a@a.com>' --date 2014-05-06T19:20:21Z",
		"git rm 'dir1/file1' 'file 2'",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2018-05-06T19:20:21Z git commit -m commit3 --author='a <a@a.com>' --date 2018-05-06T19:20:21Z",
	}
	tests := map[string]struct {
		repo                 api.RepoName
		first, second, third api.CommitID
	}{
		"git cmd": {
			repo:   MakeGitRepository(t, gitCommands...),
			first:  "b6602ca96bdc0ab647278577a3c6edcb8fe18fb0",
			second: "c5151eceb40d5e625716589b745248e1a6c6228d",
			third:  "ba3c51080ed4a5b870952ecd7f0e15f255b24cca",
		},
	}

	for label, test := range tests {
		// notafile should not exist.
		if _, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.first, "notafile"); !os.IsNotExist(err) {
			t.Errorf("%s: fs1.Stat(notafile): got err %v, want os.IsNotExist", label, err)
			continue
		}

		// dir1 should exist and be a dir.
		dir1Info, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.first, "dir1")
		if err != nil {
			t.Errorf("%s: fs1.Stat(dir1): %s", label, err)
			continue
		}
		if !dir1Info.Mode().IsDir() {
			t.Errorf("%s: dir1 stat !IsDir", label)
		}
		if name := dir1Info.Name(); name != "dir1" {
			t.Errorf("%s: got dir1 name %q, want 'dir1'", label, name)
		}
		if dir1Info.Size() != 0 {
			t.Errorf("%s: got dir1 size %d, want 0", label, dir1Info.Size())
		}
		if got, want := "ab771ba54f5571c99ffdae54f44acc7993d9f115", dir1Info.Sys().(ObjectInfo).OID().String(); got != want {
			t.Errorf("%s: got dir1 OID %q, want %q", label, got, want)
		}

		// dir1 should contain one entry: file1.
		dir1Entries, err := ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.first, "dir1", false)
		if err != nil {
			t.Errorf("%s: fs1.ReadDir(dir1): %s", label, err)
			continue
		}
		if len(dir1Entries) != 1 {
			t.Errorf("%s: got %d dir1 entries, want 1", label, len(dir1Entries))
			continue
		}
		file1Info := dir1Entries[0]
		if got, want := file1Info.Name(), "dir1/file1"; got != want {
			t.Errorf("%s: got dir1 entry name == %q, want %q", label, got, want)
		}
		if want := int64(7); file1Info.Size() != want {
			t.Errorf("%s: got dir1 entry size == %d, want %d", label, file1Info.Size(), want)
		}
		if got, want := "a20cc2fb45631b1dd262371a058b1bf31702abaa", file1Info.Sys().(ObjectInfo).OID().String(); got != want {
			t.Errorf("%s: got dir1 entry OID %q, want %q", label, got, want)
		}

		// dir2 should not exist
		_, err = ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.first, "dir2", false)
		if !os.IsNotExist(err) {
			t.Errorf("%s: fs1.ReadDir(dir2): should not exist: %s", label, err)
			continue
		}

		// dir1/file1 should exist, contain "infile1", have the right mtime, and be a file.
		file1Data, err := ReadFile(ctx, test.repo, test.first, "dir1/file1", 0, nil)
		if err != nil {
			t.Errorf("%s: fs1.ReadFile(dir1/file1): %s", label, err)
			continue
		}
		if !bytes.Equal(file1Data, []byte("infile1")) {
			t.Errorf("%s: got file1Data == %q, want %q", label, string(file1Data), "infile1")
		}
		file1Info, err = Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.first, "dir1/file1")
		if err != nil {
			t.Errorf("%s: fs1.Stat(dir1/file1): %s", label, err)
			continue
		}
		if !file1Info.Mode().IsRegular() {
			t.Errorf("%s: file1 stat !IsRegular", label)
		}
		if got, want := file1Info.Name(), "dir1/file1"; got != want {
			t.Errorf("%s: got file1 name %q, want %q", label, got, want)
		}
		if want := int64(7); file1Info.Size() != want {
			t.Errorf("%s: got file1 size == %d, want %d", label, file1Info.Size(), want)
		}

		// file 2 shouldn't exist in the 1st commit.
		_, err = ReadFile(ctx, test.repo, test.first, "file 2", 0, nil)
		if !os.IsNotExist(err) {
			t.Errorf("%s: fs1.Open(file 2): got err %v, want os.IsNotExist (file 2 should not exist in this commit)", label, err)
		}

		// file 2 should exist in the 2nd commit.
		_, err = ReadFile(ctx, test.repo, test.second, "file 2", 0, nil)
		if err != nil {
			t.Errorf("%s: fs2.Open(file 2): %s", label, err)
			continue
		}

		// file1 should also exist in the 2nd commit.
		if _, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.second, "dir1/file1"); err != nil {
			t.Errorf("%s: fs2.Stat(dir1/file1): %s", label, err)
			continue
		}
		if _, err := ReadFile(ctx, test.repo, test.second, "dir1/file1", 0, nil); err != nil {
			t.Errorf("%s: fs2.Open(dir1/file1): %s", label, err)
			continue
		}

		// root should exist (via Stat).
		root, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.second, ".")
		if err != nil {
			t.Errorf("%s: fs2.Stat(.): %s", label, err)
			continue
		}
		if !root.Mode().IsDir() {
			t.Errorf("%s: got root !IsDir", label)
		}

		// root should have 2 entries: dir1 and file 2.
		rootEntries, err := ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.second, ".", false)
		if err != nil {
			t.Errorf("%s: fs2.ReadDir(.): %s", label, err)
			continue
		}
		if got, want := len(rootEntries), 2; got != want {
			t.Errorf("%s: got len(rootEntries) == %d, want %d", label, got, want)
			continue
		}
		if e0 := rootEntries[0]; !(e0.Name() == "dir1" && e0.Mode().IsDir()) {
			t.Errorf("%s: got root entry 0 %q IsDir=%v, want 'dir1' IsDir=true", label, e0.Name(), e0.Mode().IsDir())
		}
		if e1 := rootEntries[1]; !(e1.Name() == "file 2" && !e1.Mode().IsDir()) {
			t.Errorf("%s: got root entry 1 %q IsDir=%v, want 'file 2' IsDir=false", label, e1.Name(), e1.Mode().IsDir())
		}

		// dir1 should still only contain one entry: file1.
		dir1Entries, err = ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.second, "dir1", false)
		if err != nil {
			t.Errorf("%s: fs1.ReadDir(dir1): %s", label, err)
			continue
		}
		if len(dir1Entries) != 1 {
			t.Errorf("%s: got %d dir1 entries, want 1", label, len(dir1Entries))
			continue
		}
		if got, want := dir1Entries[0].Name(), "dir1/file1"; got != want {
			t.Errorf("%s: got dir1 entry name == %q, want %q", label, got, want)
		}

		// rootEntries should be empty for third commit
		rootEntries, err = ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, test.third, ".", false)
		if err != nil {
			t.Errorf("%s: fs3.ReadDir(.): %s", label, err)
			continue
		}
		if got, want := len(rootEntries), 0; got != want {
			t.Errorf("%s: got len(rootEntries) == %d, want %d", label, got, want)
			continue
		}
	}
}

func TestRepository_FileSystem_quoteChars(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	// The repo contains 3 files: one whose filename includes a
	// non-ASCII char, one whose filename contains a double quote, and
	// one whose filename contains a backslash. These should be parsed
	// and unquoted properly.
	//
	// Filenames with double quotes are always quoted in some versions
	// of git, so we might encounter quoted paths even if
	// core.quotepath is off. We test twice, with it both on AND
	// off. (Note: Although
	// https://www.kernel.org/pub/software/scm/git/docs/git-config.html
	// says that double quotes, backslashes, and single quotes are
	// always quoted, this is not true on all git versions, such as
	// @sqs's current git version 2.7.0.)
	wantNames := []string{"⊗.txt", `".txt`, `\.txt`}
	sort.Strings(wantNames)
	gitCommands := []string{
		`touch ⊗.txt '".txt' \\.txt`,
		`git add ⊗.txt '".txt' \\.txt`,
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}
	tests := map[string]struct {
		repo api.RepoName
	}{
		"git cmd (quotepath=on)": {
			repo: MakeGitRepository(t, append([]string{"git config core.quotepath on"}, gitCommands...)...),
		},
		"git cmd (quotepath=off)": {
			repo: MakeGitRepository(t, append([]string{"git config core.quotepath off"}, gitCommands...)...),
		},
	}

	for label, test := range tests {
		commitID, err := ResolveRevision(ctx, test.repo, "master", ResolveRevisionOptions{})
		if err != nil {
			t.Fatal(err)
		}

		entries, err := ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, commitID, ".", false)
		if err != nil {
			t.Errorf("%s: fs.ReadDir(.): %s", label, err)
			continue
		}
		names := make([]string, len(entries))
		for i, e := range entries {
			names[i] = e.Name()
		}
		sort.Strings(names)

		if !reflect.DeepEqual(names, wantNames) {
			t.Errorf("%s: got names %v, want %v", label, names, wantNames)
			continue
		}

		for _, name := range wantNames {
			stat, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, commitID, name)
			if err != nil {
				t.Errorf("%s: Stat(%q): %s", label, name, err)
				continue
			}
			if stat.Name() != name {
				t.Errorf("%s: got Name == %q, want %q", label, stat.Name(), name)
				continue
			}
		}
	}
}

func TestRepository_FileSystem_gitSubmodules(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	submodDir := InitGitRepository(t,
		"touch f",
		"git add f",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	)
	const submodCommit = "94aa9078934ce2776ccbb589569eca5ef575f12e"

	gitCommands := []string{
		"git submodule add " + filepath.ToSlash(submodDir) + " submod",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m 'add submodule' --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}
	tests := map[string]struct {
		repo api.RepoName
	}{
		"git cmd": {
			repo: MakeGitRepository(t, gitCommands...),
		},
	}

	for label, test := range tests {
		commitID, err := ResolveRevision(ctx, test.repo, "master", ResolveRevisionOptions{})
		if err != nil {
			t.Fatal(err)
		}

		checkSubmoduleFileInfo := func(label string, submod fs.FileInfo) {
			if want := "submod"; submod.Name() != want {
				t.Errorf("%s: submod.Name(): got %q, want %q", label, submod.Name(), want)
			}
			// A submodule should have a special file mode and should
			// store information about its origin.
			if submod.Mode().IsRegular() {
				t.Errorf("%s: IsRegular", label)
			}
			if submod.Mode().IsDir() {
				t.Errorf("%s: IsDir", label)
			}
			if mode := submod.Mode(); mode&ModeSubmodule == 0 {
				t.Errorf("%s: submod.Mode(): got %o, want & ModeSubmodule (%o) != 0", label, mode, ModeSubmodule)
			}
			si, ok := submod.Sys().(Submodule)
			if !ok {
				t.Errorf("%s: submod.Sys(): got %v, want Submodule", label, si)
			}
			if want := filepath.ToSlash(submodDir); si.URL != want {
				t.Errorf("%s: (Submodule).URL: got %q, want %q", label, si.URL, want)
			}
			if si.CommitID != submodCommit {
				t.Errorf("%s: (Submodule).CommitID: got %q, want %q", label, si.CommitID, submodCommit)
			}
		}

		// Check the submodule fs.FileInfo both when it's returned by
		// Stat and when it's returned in a list by ReadDir.
		submod, err := Stat(ctx, authz.DefaultSubRepoPermsChecker, test.repo, commitID, "submod")
		if err != nil {
			t.Errorf("%s: fs.Stat(submod): %s", label, err)
			continue
		}
		checkSubmoduleFileInfo(label+" (Stat)", submod)
		entries, err := ReadDir(ctx, authz.DefaultSubRepoPermsChecker, test.repo, commitID, ".", false)
		if err != nil {
			t.Errorf("%s: fs.ReadDir(.): %s", label, err)
			continue
		}
		// .gitmodules file is entries[0]
		checkSubmoduleFileInfo(label+" (ReadDir)", entries[1])

		_, err = ReadFile(ctx, test.repo, commitID, "submod", 0, nil)
		if err != nil {
			t.Errorf("%s: fs.Open(submod): %s", label, err)
			continue
		}
	}
}

func TestListFiles(t *testing.T) {
	t.Parallel()

	pattern := regexp.MustCompile("file")

	runFileListingTest(t, func(ctx context.Context, checker authz.SubRepoPermissionChecker, repo api.RepoName) ([]string, error) {
		return ListFiles(ctx, repo, "HEAD", pattern, checker)
	})
}

func TestLsFiles(t *testing.T) {
	t.Parallel()

	runFileListingTest(t, func(ctx context.Context, checker authz.SubRepoPermissionChecker, repo api.RepoName) ([]string, error) {
		return LsFiles(ctx, checker, repo, "HEAD")
	})
}

// runFileListingTest tests the specified function which must return a list of filenames and an error. The test first
// tests the basic case (all paths returned), then the case with sub-repo permissions specified.
func runFileListingTest(t *testing.T,
	listingFunctionToTest func(context.Context, authz.SubRepoPermissionChecker, api.RepoName) ([]string, error)) {
	t.Helper()
	gitCommands := []string{
		"touch file1",
		"touch file2",
		"touch file3",
		"git add file1 file2 file3",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}

	repo := MakeGitRepository(t, gitCommands...)

	ctx := context.Background()

	checker := authz.NewMockSubRepoPermissionChecker()
	// Start disabled
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return false
	})

	files, err := listingFunctionToTest(ctx, checker, repo)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{
		"file1", "file2", "file3",
	}
	if diff := cmp.Diff(want, files); diff != "" {
		t.Fatal(diff)
	}

	// With filtering
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content authz.RepoContent) (authz.Perms, error) {
		if content.Path == "file1" {
			return authz.Read, nil
		}
		return authz.None, nil
	})
	ctx = actor.WithActor(ctx, &actor.Actor{
		UID: 1,
	})
	files, err = listingFunctionToTest(ctx, checker, repo)
	if err != nil {
		t.Fatal(err)
	}
	want = []string{
		"file1",
	}
	if diff := cmp.Diff(want, files); diff != "" {
		t.Fatal(diff)
	}
}

func TestParseDirectoryChildrenRoot(t *testing.T) {
	dirnames := []string{""}
	paths := []string{
		".github",
		".gitignore",
		"LICENSE",
		"README.md",
		"cmd",
		"go.mod",
		"go.sum",
		"internal",
		"protocol",
	}

	expected := map[string][]string{
		"": paths,
	}

	if diff := cmp.Diff(expected, parseDirectoryChildren(dirnames, paths)); diff != "" {
		t.Errorf("unexpected directory children result (-want +got):\n%s", diff)
	}
}

func TestParseDirectoryChildrenNonRoot(t *testing.T) {
	dirnames := []string{"cmd/", "protocol/", "cmd/protocol/"}
	paths := []string{
		"cmd/lsif-go",
		"protocol/protocol.go",
		"protocol/writer.go",
	}

	expected := map[string][]string{
		"cmd/":          {"cmd/lsif-go"},
		"protocol/":     {"protocol/protocol.go", "protocol/writer.go"},
		"cmd/protocol/": nil,
	}

	if diff := cmp.Diff(expected, parseDirectoryChildren(dirnames, paths)); diff != "" {
		t.Errorf("unexpected directory children result (-want +got):\n%s", diff)
	}
}

func TestParseDirectoryChildrenDifferentDepths(t *testing.T) {
	dirnames := []string{"cmd/", "protocol/", "cmd/protocol/"}
	paths := []string{
		"cmd/lsif-go",
		"protocol/protocol.go",
		"protocol/writer.go",
		"cmd/protocol/main.go",
	}

	expected := map[string][]string{
		"cmd/":          {"cmd/lsif-go"},
		"protocol/":     {"protocol/protocol.go", "protocol/writer.go"},
		"cmd/protocol/": {"cmd/protocol/main.go"},
	}

	if diff := cmp.Diff(expected, parseDirectoryChildren(dirnames, paths)); diff != "" {
		t.Errorf("unexpected directory children result (-want +got):\n%s", diff)
	}
}

func TestCleanDirectoriesForLsTree(t *testing.T) {
	args := []string{"", "foo", "bar/", "baz"}
	actual := cleanDirectoriesForLsTree(args)
	expected := []string{".", "foo/", "bar/", "baz/"}

	if diff := cmp.Diff(expected, actual); diff != "" {
		t.Errorf("unexpected ls-tree args (-want +got):\n%s", diff)
	}
}

func TestListDirectoryChildren(t *testing.T) {
	gitCommands := []string{
		"mkdir -p dir{1..3}/sub{1..3}",
		"touch dir1/sub1/file",
		"touch dir1/sub2/file",
		"touch dir2/sub1/file",
		"touch dir2/sub2/file",
		"touch dir3/sub1/file",
		"touch dir3/sub3/file",
		"git add .",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}

	repo := MakeGitRepository(t, gitCommands...)

	ctx := context.Background()

	checker := authz.NewMockSubRepoPermissionChecker()
	// Start disabled
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return false
	})

	dirnames := []string{"dir1/", "dir2/", "dir3/"}
	children, err := ListDirectoryChildren(ctx, checker, repo, "HEAD", dirnames)
	if err != nil {
		t.Fatal(err)
	}
	expected := map[string][]string{
		"dir1/": {"dir1/sub1", "dir1/sub2"},
		"dir2/": {"dir2/sub1", "dir2/sub2"},
		"dir3/": {"dir3/sub1", "dir3/sub3"},
	}
	if diff := cmp.Diff(expected, children); diff != "" {
		t.Fatal(diff)
	}

	// With filtering
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content authz.RepoContent) (authz.Perms, error) {
		if strings.Contains(content.Path, "dir1/") {
			return authz.Read, nil
		}
		return authz.None, nil
	})
	ctx = actor.WithActor(ctx, &actor.Actor{
		UID: 1,
	})
	children, err = ListDirectoryChildren(ctx, checker, repo, "HEAD", dirnames)
	if err != nil {
		t.Fatal(err)
	}
	expected = map[string][]string{
		"dir1/": {"dir1/sub1", "dir1/sub2"},
		"dir2/": nil,
		"dir3/": nil,
	}
	if diff := cmp.Diff(expected, children); diff != "" {
		t.Fatal(diff)
	}
}

func TestStat(t *testing.T) {
	t.Parallel()

	gitCommands := []string{
		"mkdir dir1",
		"touch dir1/file1",
		"git add dir1/file1",
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}

	dir := InitGitRepository(t, gitCommands...)
	repo := api.RepoName(filepath.Base(dir))

	if resp, err := gitserver.DefaultClient.RequestRepoUpdate(context.Background(), repo, 0); err != nil {
		t.Fatal(err)
	} else if resp.Error != "" {
		t.Fatal(resp.Error)
	}

	commitID := api.CommitID(ComputeCommitHash(dir, true))

	ctx := context.Background()

	checker := authz.NewMockSubRepoPermissionChecker()
	// Start disabled
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return false
	})

	fileInfo, err := Stat(ctx, checker, repo, commitID, "dir1/file1")
	if err != nil {
		t.Fatal(err)
	}
	want := "dir1/file1"
	if diff := cmp.Diff(want, fileInfo.Name()); diff != "" {
		t.Fatal(diff)
	}

	// With filtering
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content authz.RepoContent) (authz.Perms, error) {
		if strings.HasPrefix(content.Path, "dir2") {
			return authz.Read, nil
		}
		return authz.None, nil
	})
	ctx = actor.WithActor(ctx, &actor.Actor{
		UID: 1,
	})

	_, err = Stat(ctx, checker, repo, commitID, "dir1/file1")
	if err == nil {
		t.Fatal(err)
	}
	want = "ls-tree dir1/file1: file does not exist"
	if diff := cmp.Diff(want, err.Error()); diff != "" {
		t.Fatal(diff)
	}
}
