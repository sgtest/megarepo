package std

import (
	"fmt"
	"strings"
)

// PromptAndScan is a helper that renders the prompt into out and scans for
// subsequent input up to a newline. The return value indicates if a value was
// provided at all.
//
//	ok, err := std.PromptAndScan(std.Out, "Prompt:", &value)
//	if err != nil {
//		return err
//	} else if !ok {
//		return errors.New("response is required")
//	}
func PromptAndScan(out *Output, prompt string, result *string) (valueProvided bool, err error) {
	out.Promptf(prompt)
	n, err := fmt.Scanln(result)
	if err != nil {
		// Ignore newline error and treat it as "no input provided, no error".
		// There is no exported error type for us to assert, so we have to check
		// the error string.
		if err.Error() == "unexpected newline" {
			return false, nil
		}
		return false, err
	}
	if n == 0 || strings.TrimSpace(*result) == "" {
		return false, nil
	}
	return true, nil
}
