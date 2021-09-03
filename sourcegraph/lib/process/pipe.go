package process

import (
	"bufio"
	"fmt"
	"io"
	"os/exec"
	"sync"
)

// PipeOutput reads stdout/stderr output of the given command into the two
// io.Writers.
//
// It returns a sync.WaitGroup. The caller *must* call the Wait() method of the
// WaitGroup after waiting for the *exec.Cmd to finish.
//
// See this issue for more details: https://github.com/golang/go/issues/21922
func PipeOutput(c *exec.Cmd, stdoutWriter, stderrWriter io.Writer) (*sync.WaitGroup, error) {
	stdoutPipe, err := c.StdoutPipe()
	if err != nil {
		return nil, err
	}

	stderrPipe, err := c.StderrPipe()
	if err != nil {
		return nil, err
	}

	wg := &sync.WaitGroup{}

	readIntoBuf := func(w io.Writer, r io.Reader) {
		defer wg.Done()

		scanner := bufio.NewScanner(r)
		for scanner.Scan() {
			fmt.Fprintln(w, scanner.Text())
		}
	}

	wg.Add(2)
	go readIntoBuf(stdoutWriter, stdoutPipe)
	go readIntoBuf(stderrWriter, stderrPipe)

	return wg, nil
}
