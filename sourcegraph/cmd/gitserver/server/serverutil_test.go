package server

import (
	"fmt"
	"io/ioutil"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
)

func TestConfigureRemoteGitCommand(t *testing.T) {
	expectedEnv := []string{
		"GIT_ASKPASS=true",
		"GIT_SSH_COMMAND=ssh -o BatchMode=yes -o ConnectTimeout=30",
	}
	tests := []struct {
		input        *exec.Cmd
		tlsConfig    *tlsConfig
		expectedEnv  []string
		expectedArgs []string
	}{
		{
			input:        exec.Command("git", "clone"),
			expectedEnv:  expectedEnv,
			expectedArgs: []string{"git", "-c", "credential.helper=", "-c", "protocol.version=2", "clone"},
		},
		{
			input:        exec.Command("git", "fetch"),
			expectedEnv:  expectedEnv,
			expectedArgs: []string{"git", "-c", "credential.helper=", "-c", "protocol.version=2", "fetch"},
		},
		{
			input:       exec.Command("git", "ls-remote"),
			expectedEnv: expectedEnv,

			// Don't use protocol.version=2 for ls-remote because it hurts perf.
			expectedArgs: []string{"git", "-c", "credential.helper=", "ls-remote"},
		},

		// tlsConfig tests
		{
			input: exec.Command("git", "ls-remote"),
			tlsConfig: &tlsConfig{
				SSLNoVerify: true,
			},
			expectedEnv:  append(expectedEnv, "GIT_SSL_NO_VERIFY=true"),
			expectedArgs: []string{"git", "-c", "credential.helper=", "ls-remote"},
		},
		{
			input: exec.Command("git", "ls-remote"),
			tlsConfig: &tlsConfig{
				SSLCAInfo: "/tmp/foo.certs",
			},
			expectedEnv:  append(expectedEnv, "GIT_SSL_CAINFO=/tmp/foo.certs"),
			expectedArgs: []string{"git", "-c", "credential.helper=", "ls-remote"},
		},
	}

	for _, test := range tests {
		t.Run(strings.Join(test.input.Args, " "), func(t *testing.T) {
			conf := test.tlsConfig
			if conf == nil {
				conf = &tlsConfig{}
			}
			configureRemoteGitCommand(test.input, conf)
			if !reflect.DeepEqual(test.input.Env, test.expectedEnv) {
				t.Errorf("\ngot:  %s\nwant: %s\n", test.input.Env, test.expectedEnv)
			}
			if !reflect.DeepEqual(test.input.Args, test.expectedArgs) {
				t.Errorf("\ngot:  %s\nwant: %s\n", test.input.Args, test.expectedArgs)
			}
		})
	}
}

func TestConfigureRemoteGitCommand_tls(t *testing.T) {
	baseEnv := []string{
		"GIT_ASKPASS=true",
		"GIT_SSH_COMMAND=ssh -o BatchMode=yes -o ConnectTimeout=30",
	}

	cases := []struct {
		conf *tlsConfig
		want []string
	}{{
		conf: &tlsConfig{},
		want: nil,
	}, {
		conf: &tlsConfig{
			SSLNoVerify: true,
		},
		want: []string{
			"GIT_SSL_NO_VERIFY=true",
		},
	}}
	for _, tc := range cases {
		cmd := exec.Command("git", "clone")
		configureRemoteGitCommand(cmd, tc.conf)
		want := append(baseEnv, tc.want...)
		if !reflect.DeepEqual(cmd.Env, want) {
			t.Errorf("mismatch for %#+v (-want +got):\n%s", tc.conf, cmp.Diff(want, cmd.Env))
		}
	}
}

func TestProgressWriter(t *testing.T) {
	testCases := []struct {
		name   string
		writes []string
		text   string
	}{
		{
			name:   "identity",
			writes: []string{"hello"},
			text:   "hello",
		},
		{
			name:   "single write begin newline",
			writes: []string{"\nhelloworld"},
			text:   "\nhelloworld",
		},
		{
			name:   "single write contains newline",
			writes: []string{"hello\nworld"},
			text:   "hello\nworld",
		},
		{
			name:   "single write end newline",
			writes: []string{"helloworld\n"},
			text:   "helloworld\n",
		},
		{
			name:   "first write end newline",
			writes: []string{"hello\n", "world"},
			text:   "hello\nworld",
		},
		{
			name:   "second write begin newline",
			writes: []string{"hello", "\nworld"},
			text:   "hello\nworld",
		},
		{
			name:   "single write begin return",
			writes: []string{"\rhelloworld"},
			text:   "helloworld",
		},
		{
			name:   "single write contains return",
			writes: []string{"hello\rworld"},
			text:   "world",
		},
		{
			name:   "single write end return",
			writes: []string{"helloworld\r"},
			text:   "helloworld\r",
		},
		{
			name:   "first write contains return",
			writes: []string{"hel\rlo", "world"},
			text:   "loworld",
		},
		{
			name:   "first write end return",
			writes: []string{"hello\r", "world"},
			text:   "world",
		},
		{
			name:   "second write begin return",
			writes: []string{"hello", "\rworld"},
			text:   "world",
		},
		{
			name:   "second write contains return",
			writes: []string{"hello", "wor\rld"},
			text:   "ld",
		},
		{
			name:   "second write ends return",
			writes: []string{"hello", "world\r"},
			text:   "helloworld\r",
		},
		{
			name:   "third write",
			writes: []string{"hello", "world\r", "hola"},
			text:   "hola",
		},
		{
			name:   "progress one write",
			writes: []string{"progress\n1%\r20%\r100%\n"},
			text:   "progress\n100%\n",
		},
		{
			name:   "progress multiple writes",
			writes: []string{"progress\n", "1%\r", "2%\r", "100%"},
			text:   "progress\n100%",
		},
		{
			name:   "one two three four",
			writes: []string{"one\ntwotwo\nthreethreethree\rfourfourfourfour\n"},
			text:   "one\ntwotwo\nfourfourfourfour\n",
		},
		{
			name:   "real git",
			writes: []string{"Cloning into bare repository '/Users/nick/.sourcegraph/repos/github.com/nicksnyder/go-i18n/.git'...\nremote: Counting objects: 2148, done.        \nReceiving objects:   0% (1/2148)   \rReceiving objects: 100% (2148/2148), 473.65 KiB | 366.00 KiB/s, done.\nResolving deltas:   0% (0/1263)   \rResolving deltas: 100% (1263/1263), done.\n"},
			text:   "Cloning into bare repository '/Users/nick/.sourcegraph/repos/github.com/nicksnyder/go-i18n/.git'...\nremote: Counting objects: 2148, done.        \nReceiving objects: 100% (2148/2148), 473.65 KiB | 366.00 KiB/s, done.\nResolving deltas: 100% (1263/1263), done.\n",
		},
	}
	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			var w progressWriter
			for _, write := range testCase.writes {
				_, _ = w.Write([]byte(write))
			}
			if actual := w.String(); testCase.text != actual {
				t.Fatalf("\ngot:\n%s\nwant:\n%s\n", actual, testCase.text)
			}
		})
	}
}

func TestUpdateFileIfDifferent(t *testing.T) {
	dir, err := ioutil.TempDir("", t.Name())
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(dir)

	target := filepath.Join(dir, "sg_refhash")

	write := func(content string) {
		err := ioutil.WriteFile(target, []byte(content), 0600)
		if err != nil {
			t.Fatal(err)
		}
	}
	read := func() string {
		b, err := ioutil.ReadFile(target)
		if err != nil {
			t.Fatal(err)
		}
		return string(b)
	}
	update := func(content string) bool {
		ok, err := updateFileIfDifferent(target, []byte(content))
		if err != nil {
			t.Fatal(err)
		}
		return ok
	}

	// File doesn't exist so should do an update
	if !update("foo") {
		t.Fatal("expected update")
	}
	if read() != "foo" {
		t.Fatal("file content changed")
	}

	// File does exist and already says foo. So should not update
	if update("foo") {
		t.Fatal("expected no update")
	}
	if read() != "foo" {
		t.Fatal("file content changed")
	}

	// Content is different so should update
	if !update("bar") {
		t.Fatal("expected update to update file")
	}
	if read() != "bar" {
		t.Fatal("file content did not change")
	}

	// Write something different
	write("baz")
	if update("baz") {
		t.Fatal("expected update to not update file")
	}
	if read() != "baz" {
		t.Fatal("file content did not change")
	}
	if update("baz") {
		t.Fatal("expected update to not update file")
	}
}

func TestLogErrors(t *testing.T) {
	cases := []struct {
		stderr string
		logged string
	}{{
		stderr: "",
		logged: "",
	}, {
		stderr: "fatal: bad object HEAD\n",
		logged: "",
	}, {
		stderr: "error: packfile .git/objects/pack/pack-a.pack does not match index",
		logged: "org/repo error: packfile .git/objects/pack/pack-a.pack does not match index\n",
	}, {
		stderr: `error: packfile .git/objects/pack/pack-a.pack does not match index
error: packfile .git/objects/pack/pack-b.pack does not match index
fatal: bad object HEAD
`,
		logged: `org/repo error: packfile .git/objects/pack/pack-a.pack does not match index
org/repo error: packfile .git/objects/pack/pack-b.pack does not match index
`,
	}}
	for _, c := range cases {
		var b strings.Builder
		printf := func(format string, v ...interface{}) {
			fmt.Fprintf(&b, format, v...)
		}
		logErrors(printf, "org/repo", []byte(c.stderr))
		got := b.String()
		if got != c.logged {
			t.Errorf("mismatch (-want +got):\n%s", cmp.Diff(c.logged, got))
		}
	}
}

func TestFlushingResponseWriter(t *testing.T) {
	flush := make(chan struct{})
	fw := &flushingResponseWriter{
		w: httptest.NewRecorder(),
		flusher: flushFunc(func() {
			flush <- struct{}{}
		}),
	}
	done := make(chan struct{})
	go func() {
		fw.periodicFlush()
		close(done)
	}()

	_, _ = fw.Write([]byte("hi"))

	select {
	case <-flush:
		close(flush)
	case <-time.After(5 * time.Second):
		t.Fatal("periodic flush did not happen")
	}

	fw.Close()

	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("periodic flush goroutine did not close")
	}
}

type flushFunc func()

func (f flushFunc) Flush() {
	f()
}
