package redispool

import "testing"

func TestSchemeMatcher(t *testing.T) {
	tests := []struct {
		urlMaybe  string
		hasScheme bool
	}{
		{"redis://foo.com", true},
		{"https://foo.com", true},
		{"redis://:password@foo.com/0", true},
		{"redis://foo.com/0?password=foo", true},
		{"foo:1234", false},
	}
	for _, test := range tests {
		hasScheme := schemeMatcher.MatchString(test.urlMaybe)
		if hasScheme != test.hasScheme {
			t.Errorf("for string %q, exp != got: %v != %v", test.urlMaybe, test.hasScheme, hasScheme)
		}
	}
}
