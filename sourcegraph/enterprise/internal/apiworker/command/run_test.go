package command

import (
	"context"
	"testing"
)

func TestRunCommandEmptyCommand(t *testing.T) {
	if err := runCommand(context.Background(), nil, newCommand()); err != ErrIllegalCommand {
		t.Errorf("unexpected error. want=%q have=%q", ErrIllegalCommand, err)
	}
}

func TestRunCommandIllegalCommand(t *testing.T) {
	if err := runCommand(context.Background(), nil, newCommand("kill")); err != ErrIllegalCommand {
		t.Errorf("unexpected error. want=%q have=%q", ErrIllegalCommand, err)
	}
}
