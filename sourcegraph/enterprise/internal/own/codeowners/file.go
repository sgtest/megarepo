package codeowners

import (
	"sync"

	codeownerspb "github.com/sourcegraph/sourcegraph/enterprise/internal/own/codeowners/v1"
)

type Ruleset struct {
	proto *codeownerspb.File
	rules []*CompiledRule
}

func NewRuleset(proto *codeownerspb.File) *Ruleset {
	f := &Ruleset{
		proto: proto,
	}
	for _, r := range proto.GetRule() {
		f.rules = append(f.rules, &CompiledRule{proto: r})
	}
	return f
}

// FindOwners returns the Owners associated with given path as per this CODEOWNERS file.
// Rules are evaluated in order: Returned owners come from the rule which pattern matches
// given path, that is the furthest down the file.
func (x *Ruleset) FindOwners(path string) []*codeownerspb.Owner {
	for i := len(x.rules) - 1; i >= 0; i-- {
		rule := x.rules[i]
		if rule.match(path) {
			return rule.GetOwner()
		}
	}
	return nil
}

type CompiledRule struct {
	proto       *codeownerspb.Rule
	glob        *globPattern
	compileOnce sync.Once
}

func (r *CompiledRule) match(filePath string) bool {
	r.compileOnce.Do(func() {
		// For now, we ignore errors.
		r.glob, _ = compile(r.proto.GetPattern())
	})
	return r.glob.match(filePath)
}

func (r *CompiledRule) GetOwner() []*codeownerspb.Owner {
	return r.proto.Owner
}
