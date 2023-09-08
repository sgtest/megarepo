package stack

import (
	"github.com/hashicorp/terraform-cdk-go/cdktf"

	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

// Stack encapsulates a CDKTF stack and the name the stack was originally
// created with.
type Stack struct {
	Name  string
	Stack cdktf.TerraformStack
	// Metadata is arbitrary metadata that can be attached by stack options.
	Metadata map[string]string
}

// Set collects the stacks that comprise a CDKTF application.
type Set struct {
	// app represents a CDKTF application that is comprised of the stacks in
	// this set.
	//
	// The App can be extracted with stack.ExtractApp(*Set)
	app cdktf.App
	// opts are applied to all the stacks created from (*Set).New()
	opts []NewStackOption
	// stacks is all the stacks created from (*Set).New()
	//
	// Names of created stacks can be extracted with stack.ExtractStacks(*Set)
	stacks []Stack
}

// NewStackOption applies modifications to cdktf.TerraformStacks when they are
// created.
type NewStackOption func(s Stack)

// NewSet creates a new stack.Set, which collects the stacks that comprise a
// CDKTF application.
func NewSet(renderDir string, opts ...NewStackOption) *Set {
	return &Set{
		app: cdktf.NewApp(&cdktf.AppConfig{
			Outdir: pointers.Ptr(renderDir),
		}),
		opts:   opts,
		stacks: []Stack{},
	}
}

// New creates a new stack belonging to this set.
func (s *Set) New(name string, opts ...NewStackOption) cdktf.TerraformStack {
	stack := Stack{
		Name:     name,
		Stack:    cdktf.NewTerraformStack(s.app, &name),
		Metadata: make(map[string]string),
	}
	for _, opt := range append(s.opts, opts...) {
		opt(stack)
	}
	s.stacks = append(s.stacks, stack)
	return stack.Stack
}

// ExtractApp returns the underlying CDKTF application of this stack.Set for
// synthesizing resources.
//
// It is intentionally not part of the stack.Set interface as it should not
// generally be needed.
func ExtractApp(set *Set) cdktf.App { return set.app }

// ExtractStacks returns all the stacks created so far in this stack.Set.
//
// It is intentionally not part of the stack.Set interface as it should not
// generally be needed.
func ExtractStacks(set *Set) []string {
	var stackNames []string
	for _, s := range set.stacks {
		stackNames = append(stackNames, s.Name)
	}
	return stackNames
}
