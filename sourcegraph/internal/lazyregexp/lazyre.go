// Copyright 2018 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

// Package lazyregexp is a thin wrapper over regexp, allowing the use of global
// regexp variables without forcing them to be compiled at init.
package lazyregexp

import (
	"os"
	"regexp"
	"strings"
	"sync"
)

// Regexp is a wrapper around regexp.Regexp, where the underlying regexp will be
// compiled the first time it is needed.
type Regexp struct {
	str   string
	posix bool
	once  sync.Once
	rx    *regexp.Regexp
}

func (r *Regexp) re() *regexp.Regexp {
	r.once.Do(r.build)
	return r.rx
}

func (r *Regexp) build() {
	if r.posix {
		r.rx = regexp.MustCompilePOSIX(r.str)
	} else {
		r.rx = regexp.MustCompile(r.str)
	}
	r.str = ""
}

func (r *Regexp) FindSubmatch(s []byte) [][]byte {
	return r.re().FindSubmatch(s)
}

func (r *Regexp) FindStringSubmatch(s string) []string {
	return r.re().FindStringSubmatch(s)
}

func (r *Regexp) FindStringSubmatchIndex(s string) []int {
	return r.re().FindStringSubmatchIndex(s)
}

func (r *Regexp) ReplaceAllString(src, repl string) string {
	return r.re().ReplaceAllString(src, repl)
}

func (r *Regexp) FindString(s string) string {
	return r.re().FindString(s)
}

func (r *Regexp) FindAllString(s string, n int) []string {
	return r.re().FindAllString(s, n)
}

func (r *Regexp) MatchString(s string) bool {
	return r.re().MatchString(s)
}

func (r *Regexp) SubexpNames() []string {
	return r.re().SubexpNames()
}

func (r *Regexp) FindAllStringSubmatch(s string, n int) [][]string {
	return r.re().FindAllStringSubmatch(s, n)
}

func (r *Regexp) Split(s string, n int) []string {
	return r.re().Split(s, n)
}

func (r *Regexp) ReplaceAllLiteralString(src, repl string) string {
	return r.re().ReplaceAllLiteralString(src, repl)
}

func (r *Regexp) FindAllIndex(b []byte, n int) [][]int {
	return r.re().FindAllIndex(b, n)
}

func (r *Regexp) Match(b []byte) bool {
	return r.re().Match(b)
}

func (r *Regexp) ReplaceAllStringFunc(src string, repl func(string) string) string {
	return r.re().ReplaceAllStringFunc(src, repl)
}

func (r *Regexp) ReplaceAll(src, repl []byte) []byte {
	return r.re().ReplaceAll(src, repl)
}

var inTest = len(os.Args) > 0 && strings.HasSuffix(strings.TrimSuffix(os.Args[0], ".exe"), ".test")

// New creates a new lazy regexp, delaying the compiling work until it is first
// needed. If the code is being run as part of tests, the regexp compiling will
// happen immediately.
func New(str string) *Regexp {
	lr := &Regexp{str: str}
	if inTest {
		// In tests, always compile the regexps early.
		lr.re()
	}
	return lr
}

// NewPOSIX creates a new lazy regexp, delaying the compiling work until it is
// first needed. If the code is being run as part of tests, the regexp
// compiling will happen immediately.
func NewPOSIX(str string) *Regexp {
	lr := &Regexp{str: str, posix: true}
	if inTest {
		// In tests, always compile the regexps early.
		lr.re()
	}
	return lr
}
