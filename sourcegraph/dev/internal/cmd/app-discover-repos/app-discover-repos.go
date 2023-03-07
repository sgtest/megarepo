package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/sourcegraph/internal/service/servegit"
	"github.com/sourcegraph/sourcegraph/internal/singleprogram/filepicker"
)

const usage = `

app-discover-repos runs the same discovery logic used by app to discover local
repositories. It will print some additional debug information.`

func main() {
	liblog := log.Init(log.Resource{
		Name:       "app-discover-repos",
		Version:    "dev",
		InstanceID: os.Getenv("HOSTNAME"),
	})
	defer liblog.Sync()

	flag.Usage = func() {
		fmt.Fprintf(flag.CommandLine.Output(), "Usage of %s:\n\n%s\n\n", os.Args[0], strings.TrimSpace(usage))
		flag.PrintDefaults()
	}

	var c servegit.Config
	c.Load()

	root := flag.String("root", c.CWDRoot, "the directory we search from.")
	block := flag.Bool("block", false, "by default we stream out the repos we find. This is not exactly what sourcegraph uses, so enable this flag for the same behaviour.")
	picker := flag.Bool("picker", false, "try run the file picker.")
	lsRemote := flag.Bool("git-ls-remote", false, "run git ls-remote on each CloneURL to validate git.")
	verbose := flag.Bool("v", false, "verbose output.")

	flag.Parse()

	if *picker {
		p, ok := filepicker.Lookup(log.Scoped("picker", ""))
		if !ok {
			fmt.Fprintf(os.Stderr, "filepicker not found\n")
		} else {
			path, err := p(context.Background())
			if err != nil {
				fatalf("filepicker error: %v\n", err)
			}
			fmt.Fprintf(os.Stderr, "filepicker picked %q\n", path)
			*root = path
		}
	}

	srv := &servegit.Serve{
		ServeConfig: c.ServeConfig,
		Logger:      log.Scoped("serve", ""),
	}

	if *lsRemote {
		if err := srv.Start(); err != nil {
			fatalf("failed to start server: %v\n", err)
		}
	}

	printRepo := func(r servegit.Repo) {
		if *verbose {
			fmt.Printf("%s\t%s\t%s\n", r.Name, r.URI, r.ClonePath)
		} else {
			fmt.Println(r.Name)
		}
		if *lsRemote {
			cloneURL := fmt.Sprintf("http://%s/%s", srv.Addr, strings.TrimPrefix(r.ClonePath, "/"))
			fmt.Printf("running git ls-remote %s HEAD\n", cloneURL)
			cmd := exec.Command("git", "ls-remote", cloneURL, "HEAD")
			cmd.Stderr = os.Stderr
			cmd.Stdout = os.Stdout
			if err := cmd.Run(); err != nil {
				fatalf("failed to run ls-remote: %v", err)
			}
		}
	}

	if *block {
		repos, err := srv.Repos(*root)
		if err != nil {
			fatalf("Repos returned error: %v\n", err)
		}
		for _, r := range repos {
			printRepo(r)
		}
	} else {
		repoC := make(chan servegit.Repo, 4)
		go func() {
			defer close(repoC)
			err := srv.Walk(*root, repoC)
			if err != nil {
				fatalf("Walk returned error: %v\n", err)
			}
		}()
		for r := range repoC {
			printRepo(r)
		}
	}
}

func fatalf(format string, a ...any) {
	_, _ = fmt.Fprintf(os.Stderr, format, a...)
	os.Exit(1)
}
