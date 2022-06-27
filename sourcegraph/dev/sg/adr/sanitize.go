package adr

import (
	"regexp"
	"strings"
)

var nonAlphaNumericOrDash = regexp.MustCompile("[^a-z0-9-]+")

func sanitizeADRName(name string) string {
	return nonAlphaNumericOrDash.ReplaceAllString(
		strings.ReplaceAll(strings.ToLower(name), " ", "-"), "",
	)
}
