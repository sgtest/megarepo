package cloud

import (
	"encoding/json"
	"fmt"
	"io"
	"time"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

type Printer interface {
	Print(...*Instance) error
}

type rawInstancePrinter struct {
	w io.Writer
}

type terminalInstancePrinter struct {
	headingFmt string
	headings   []any
	valueFunc  func(i *Instance) []any
}

type jsonInstancePrinter struct {
	w io.Writer
}

func newDefaultTerminalInstancePrinter() *terminalInstancePrinter {
	valueFunc := func(inst *Instance) []any {
		name := inst.Name
		if len(name) > 37 {
			name = name[:37] + "..."
		}

		status := inst.Status.Status
		expiresAt := "n/a"
		if !inst.ExpiresAt.IsZero() {
			expiresAt = inst.ExpiresAt.Format(time.RFC3339)
		}

		return []any{
			name, status, expiresAt,
		}

	}
	return newTerminalInstancePrinter(valueFunc, "%-40s %-11s %s", "Name", "Status", "Expires At")
}

func newTerminalInstancePrinter(valueFunc func(i *Instance) []any, headingFmt string, headings ...string) *terminalInstancePrinter {
	anyHeadings := make([]any, 0, len(headings))
	for _, h := range headings {
		anyHeadings = append(anyHeadings, h)
	}

	return &terminalInstancePrinter{
		headingFmt: headingFmt,
		headings:   anyHeadings,
		valueFunc:  valueFunc,
	}
}

func (p *terminalInstancePrinter) Print(items ...*Instance) error {
	heading := fmt.Sprintf(p.headingFmt, p.headings...)
	std.Out.WriteLine(output.Line("", output.StyleBold, heading))
	for _, inst := range items {
		values := p.valueFunc(inst)
		line := fmt.Sprintf("%-40s %-11s %s", values...)
		std.Out.WriteLine(output.Line("", output.StyleGrey, line))
	}

	std.Out.WriteSuggestionf("Some names may be truncated. To see the full names use the --raw format")
	return nil
}

func newJSONInstancePrinter(w io.Writer) *jsonInstancePrinter {
	return &jsonInstancePrinter{w: w}
}

func (p *jsonInstancePrinter) Print(items ...*Instance) error {
	return json.NewEncoder(p.w).Encode(items)
}

func newRawInstancePrinter(w io.Writer) *rawInstancePrinter {
	return &rawInstancePrinter{w: w}
}

func (p *rawInstancePrinter) Print(items ...*Instance) error {
	for _, inst := range items {
		fmt.Fprintln(p.w, inst.String())
	}

	return nil
}
