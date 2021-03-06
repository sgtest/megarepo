/*
Copyright 2017 The Kubernetes Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package util

import (
	"reflect"
	"sort"
	"testing"
)

func TestBuildArgumentListFromMap(t *testing.T) {
	var tests = []struct {
		name      string
		base      map[string]string
		overrides map[string]string
		expected  []string
	}{
		{
			name: "override an argument from the base",
			base: map[string]string{
				"admission-control": "NamespaceLifecycle",
				"allow-privileged":  "true",
			},
			overrides: map[string]string{
				"admission-control": "NamespaceLifecycle,LimitRanger",
			},
			expected: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
			},
		},
		{
			name: "add an argument that is not in base",
			base: map[string]string{
				"allow-privileged": "true",
			},
			overrides: map[string]string{
				"admission-control": "NamespaceLifecycle,LimitRanger",
			},
			expected: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
			},
		},
		{
			name: "allow empty strings in base",
			base: map[string]string{
				"allow-privileged":                   "true",
				"something-that-allows-empty-string": "",
			},
			overrides: map[string]string{
				"admission-control": "NamespaceLifecycle,LimitRanger",
			},
			expected: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
				"--something-that-allows-empty-string=",
			},
		},
		{
			name: "allow empty strings in overrides",
			base: map[string]string{
				"allow-privileged":                   "true",
				"something-that-allows-empty-string": "foo",
			},
			overrides: map[string]string{
				"admission-control":                  "NamespaceLifecycle,LimitRanger",
				"something-that-allows-empty-string": "",
			},
			expected: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
				"--something-that-allows-empty-string=",
			},
		},
	}

	for _, rt := range tests {
		t.Run(rt.name, func(t *testing.T) {
			actual := BuildArgumentListFromMap(rt.base, rt.overrides)
			if !reflect.DeepEqual(actual, rt.expected) {
				t.Errorf("failed BuildArgumentListFromMap:\nexpected:\n%v\nsaw:\n%v", rt.expected, actual)
			}
		})
	}
}

func TestParseArgumentListToMap(t *testing.T) {
	var tests = []struct {
		name        string
		args        []string
		expectedMap map[string]string
	}{
		{
			name: "normal case",
			args: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
			},
			expectedMap: map[string]string{
				"admission-control": "NamespaceLifecycle,LimitRanger",
				"allow-privileged":  "true",
			},
		},
		{
			name: "test that feature-gates is working",
			args: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
				"--feature-gates=EnableFoo=true,EnableBar=false",
			},
			expectedMap: map[string]string{
				"admission-control": "NamespaceLifecycle,LimitRanger",
				"allow-privileged":  "true",
				"feature-gates":     "EnableFoo=true,EnableBar=false",
			},
		},
		{
			name: "test that a binary can be the first arg",
			args: []string{
				"kube-apiserver",
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
				"--feature-gates=EnableFoo=true,EnableBar=false",
			},
			expectedMap: map[string]string{
				"admission-control": "NamespaceLifecycle,LimitRanger",
				"allow-privileged":  "true",
				"feature-gates":     "EnableFoo=true,EnableBar=false",
			},
		},
	}

	for _, rt := range tests {
		t.Run(rt.name, func(t *testing.T) {
			actualMap := ParseArgumentListToMap(rt.args)
			if !reflect.DeepEqual(actualMap, rt.expectedMap) {
				t.Errorf("failed ParseArgumentListToMap:\nexpected:\n%v\nsaw:\n%v", rt.expectedMap, actualMap)
			}
		})
	}
}

func TestReplaceArgument(t *testing.T) {
	var tests = []struct {
		name         string
		args         []string
		mutateFunc   func(map[string]string) map[string]string
		expectedArgs []string
	}{
		{
			name: "normal case",
			args: []string{
				"kube-apiserver",
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
			},
			mutateFunc: func(argMap map[string]string) map[string]string {
				argMap["admission-control"] = "NamespaceLifecycle,LimitRanger,ResourceQuota"
				return argMap
			},
			expectedArgs: []string{
				"kube-apiserver",
				"--admission-control=NamespaceLifecycle,LimitRanger,ResourceQuota",
				"--allow-privileged=true",
			},
		},
		{
			name: "another normal case",
			args: []string{
				"kube-apiserver",
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
			},
			mutateFunc: func(argMap map[string]string) map[string]string {
				argMap["new-arg-here"] = "foo"
				return argMap
			},
			expectedArgs: []string{
				"kube-apiserver",
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
				"--new-arg-here=foo",
			},
		},
	}

	for _, rt := range tests {
		t.Run(rt.name, func(t *testing.T) {
			actualArgs := ReplaceArgument(rt.args, rt.mutateFunc)
			sort.Strings(actualArgs)
			sort.Strings(rt.expectedArgs)
			if !reflect.DeepEqual(actualArgs, rt.expectedArgs) {
				t.Errorf("failed ReplaceArgument:\nexpected:\n%v\nsaw:\n%v", rt.expectedArgs, actualArgs)
			}
		})
	}
}

func TestRoundtrip(t *testing.T) {
	var tests = []struct {
		name string
		args []string
	}{
		{
			name: "normal case",
			args: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
			},
		},
		{
			name: "test that feature-gates is working",
			args: []string{
				"--admission-control=NamespaceLifecycle,LimitRanger",
				"--allow-privileged=true",
				"--feature-gates=EnableFoo=true,EnableBar=false",
			},
		},
	}

	for _, rt := range tests {
		t.Run(rt.name, func(t *testing.T) {
			// These two methods should be each other's opposite functions, test that by chaining the methods and see if you get the same result back
			actual := BuildArgumentListFromMap(ParseArgumentListToMap(rt.args), map[string]string{})
			sort.Strings(actual)
			sort.Strings(rt.args)

			if !reflect.DeepEqual(actual, rt.args) {
				t.Errorf("failed TestRoundtrip:\nexpected:\n%v\nsaw:\n%v", rt.args, actual)
			}
		})
	}
}

func TestParseArgument(t *testing.T) {
	var tests = []struct {
		name        string
		arg         string
		expectedKey string
		expectedVal string
		expectedErr bool
	}{
		{
			name:        "arg cannot be empty",
			arg:         "",
			expectedErr: true,
		},
		{
			name:        "arg must contain -- and =",
			arg:         "a",
			expectedErr: true,
		},
		{
			name:        "arg must contain -- and =",
			arg:         "a-z",
			expectedErr: true,
		},
		{
			name:        "arg must contain --",
			arg:         "a=b",
			expectedErr: true,
		},
		{
			name:        "arg must contain a key",
			arg:         "--=b",
			expectedErr: true,
		},
		{
			name:        "arg can contain key but no value",
			arg:         "--a=",
			expectedKey: "a",
			expectedVal: "",
			expectedErr: false,
		},
		{
			name:        "simple case",
			arg:         "--a=b",
			expectedKey: "a",
			expectedVal: "b",
			expectedErr: false,
		},
		{
			name:        "keys/values with '-' should be supported",
			arg:         "--very-long-flag-name=some-value",
			expectedKey: "very-long-flag-name",
			expectedVal: "some-value",
			expectedErr: false,
		},
		{
			name:        "numbers should be handled correctly",
			arg:         "--some-number=0.2",
			expectedKey: "some-number",
			expectedVal: "0.2",
			expectedErr: false,
		},
		{
			name:        "lists should be handled correctly",
			arg:         "--admission-control=foo,bar,baz",
			expectedKey: "admission-control",
			expectedVal: "foo,bar,baz",
			expectedErr: false,
		},
		{
			name:        "more than one '=' should be allowed",
			arg:         "--feature-gates=EnableFoo=true,EnableBar=false",
			expectedKey: "feature-gates",
			expectedVal: "EnableFoo=true,EnableBar=false",
			expectedErr: false,
		},
	}

	for _, rt := range tests {
		t.Run(rt.name, func(t *testing.T) {
			key, val, actual := parseArgument(rt.arg)
			if (actual != nil) != rt.expectedErr {
				t.Errorf("failed parseArgument:\nexpected error:\n%t\nsaw error:\n%v", rt.expectedErr, actual)
			}
			if (key != rt.expectedKey) || (val != rt.expectedVal) {
				t.Errorf("failed parseArgument:\nexpected key: %s\nsaw key: %s\nexpected value: %s\nsaw value: %s", rt.expectedKey, key, rt.expectedVal, val)
			}
		})
	}
}
