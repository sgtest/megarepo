package conf

import (
	"reflect"
	"sort"
	"testing"

	"github.com/sourcegraph/sourcegraph/schema"
)

func TestDiff(t *testing.T) {
	tests := []struct {
		name          string
		before, after *Unified
		want          []string
	}{
		{
			name:   "diff",
			before: &Unified{Critical: schema.CriticalConfiguration{ExternalURL: "a"}},
			after:  &Unified{Critical: schema.CriticalConfiguration{ExternalURL: "b"}},
			want:   []string{"critical::externalURL"},
		},
		{
			name:   "nodiff",
			before: &Unified{Critical: schema.CriticalConfiguration{ExternalURL: "a"}},
			after:  &Unified{Critical: schema.CriticalConfiguration{ExternalURL: "a"}},
			want:   nil,
		},
		{
			name: "slice_diff",
			before: &Unified{
				SiteConfiguration: schema.SiteConfiguration{ReviewBoard: []*schema.ReviewBoard{{Url: "a"}}},
				Critical:          schema.CriticalConfiguration{ExternalURL: "a"},
			},
			after: &Unified{
				SiteConfiguration: schema.SiteConfiguration{ReviewBoard: []*schema.ReviewBoard{{Url: "b"}}},
				Critical:          schema.CriticalConfiguration{ExternalURL: "a"},
			},
			want: []string{"reviewBoard"},
		},
		{
			name: "slice_nodiff",
			before: &Unified{
				SiteConfiguration: schema.SiteConfiguration{ReviewBoard: []*schema.ReviewBoard{{Url: "a"}}},
				Critical:          schema.CriticalConfiguration{ExternalURL: "a"},
			},
			after: &Unified{
				SiteConfiguration: schema.SiteConfiguration{ReviewBoard: []*schema.ReviewBoard{{Url: "a"}}},
				Critical:          schema.CriticalConfiguration{ExternalURL: "a"},
			},
		},
		{
			name: "multi_diff",
			before: &Unified{
				SiteConfiguration: schema.SiteConfiguration{ReviewBoard: []*schema.ReviewBoard{{Url: "b"}}},
				Critical:          schema.CriticalConfiguration{ExternalURL: "a"},
			},
			after: &Unified{
				SiteConfiguration: schema.SiteConfiguration{ReviewBoard: []*schema.ReviewBoard{{Url: "a"}}},
				Critical:          schema.CriticalConfiguration{ExternalURL: "b"},
			},
			want: []string{"critical::externalURL", "reviewBoard"},
		},
		{
			name: "experimental_features",
			before: &Unified{SiteConfiguration: schema.SiteConfiguration{ExperimentalFeatures: &schema.ExperimentalFeatures{
				Discussions: "enabled",
			}}},
			after: &Unified{SiteConfiguration: schema.SiteConfiguration{ExperimentalFeatures: &schema.ExperimentalFeatures{
				Discussions: "disabled",
			}}},
			want: []string{"experimentalFeatures::discussions"},
		},
		{
			name:   "experimental_features_noop",
			before: &Unified{SiteConfiguration: schema.SiteConfiguration{ExperimentalFeatures: &schema.ExperimentalFeatures{}}},
			after:  &Unified{SiteConfiguration: schema.SiteConfiguration{ExperimentalFeatures: &schema.ExperimentalFeatures{}}},
			want:   nil,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got := toSlice(diff(test.before, test.after))
			sort.Strings(got)
			if !reflect.DeepEqual(got, test.want) {
				t.Fatalf("got %#v want %#v", got, test.want)
			}
		})
	}
}

func toSlice(m map[string]struct{}) []string {
	var s []string
	for v := range m {
		s = append(s, v)
	}
	return s
}
