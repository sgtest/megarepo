package main

import (
	"bytes"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/urfave/cli/v2"
)

// testSG creates a copy of the sg app for testing.
func testSG() *cli.App {
	tsg := *sg
	return &tsg
}

func TestAppRun(t *testing.T) {
	sg := testSG()

	// Capture output
	var out, err bytes.Buffer
	sg.Writer = &out
	sg.ErrWriter = &err
	// Check app starts up correctly
	assert.NoError(t, sg.Run([]string{"help"}))
	assert.Contains(t, out.String(), "The Sourcegraph developer tool!")
	// We do not want errors anywhere
	assert.NotContains(t, out.String(), "error")
	assert.NotContains(t, out.String(), "panic")
	assert.Empty(t, err.String())
}

func TestCommandFormatting(t *testing.T) {
	sg := testSG()

	sg.Setup()
	for _, cmd := range sg.Commands {
		testCommandFormatting(t, cmd)
		// for top-level commands, also require a category
		assert.NotEmptyf(t, cmd.Category, "top-level command %q Category should be set", cmd.Name)
	}
}

func testCommandFormatting(t *testing.T, cmd *cli.Command) {
	t.Run(cmd.Name, func(t *testing.T) {
		assert.NotEmpty(t, cmd.Name, "Name should be set")
		assert.NotEmpty(t, cmd.Usage, "Usage should be set")
		assert.False(t, strings.HasSuffix(cmd.Usage, "."), "Usage should not end with period")
		assert.NotNil(t, cmd.Action, "Action must be provided (for parent commands, 'suggestSubcommandsAction'")

		for _, subCmd := range cmd.Subcommands {
			testCommandFormatting(t, subCmd)
		}
	})
}
