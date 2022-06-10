package main

import (
	"bufio"
	"bytes"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"text/template"
	"time"

	"github.com/sourcegraph/sourcegraph/dev/sg/root"
)

type adr struct {
	Number int
	Title  string
	Path   string
	Date   string
}

type templateData struct {
	Adrs []adr
}

//go:generate go run .
func main() {
	repoRoot, err := root.RepositoryRoot()
	if err != nil {
		panic(err)
	}
	tmpl, err := template.ParseFiles(filepath.Join(repoRoot, "dev", "adr-docs", "index.md.tmpl"))
	if err != nil {
		panic(err)
	}
	entries, err := os.ReadDir(filepath.Join(repoRoot, "doc", "dev", "adr"))
	if err != nil {
		panic(err)
	}

	var adrs []adr
	re := regexp.MustCompile(`^(\d+)-.+\.md`)
	for _, entry := range entries {
		if !re.MatchString(entry.Name()) {
			continue
		}
		b, err := os.ReadFile(filepath.Join(repoRoot, "doc", "dev", "adr", entry.Name()))
		if err != nil {
			panic(err)
		}
		m := re.FindAllStringSubmatch(entry.Name(), 1)
		ts, _ := strconv.Atoi(m[0][1]) // We can ignore the err because we know from the regexp it's only digits.

		reHeader := regexp.MustCompile(`#\s+(\d+)\.\s+(.*)$`)
		s := bufio.NewScanner(bytes.NewReader(b))
		var title string
		var number int
		for s.Scan() {
			matches := reHeader.FindAllStringSubmatch(s.Text(), 1)
			if len(matches) > 0 {
				number, _ = strconv.Atoi(matches[0][1]) // We can ignore the err because we know from the regexp it's only digits.
				title = matches[0][2]
				adrs = append(adrs, adr{
					Title:  title,
					Number: number,
					Path:   entry.Name(),
					Date:   time.Unix(int64(ts), 0).Format("2006-01-02"),
				})
				break
			}
		}
	}

	presenter := templateData{
		Adrs: adrs,
	}

	f, err := os.Create(filepath.Join(repoRoot, "doc", "dev", "adr", "index.md"))
	if err != nil {
		panic(err)
	}
	defer f.Close()
	err = tmpl.Execute(f, &presenter)
	if err != nil {
		panic(err)
	}
}
