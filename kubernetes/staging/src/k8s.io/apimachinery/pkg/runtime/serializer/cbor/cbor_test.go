/*
Copyright 2024 The Kubernetes Authors.

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

// The tests in this package focus on the correctness of its implementation of
// runtime.Serializer. The specific behavior of marshaling Go values to CBOR bytes and back is
// tested in the ./internal/modes package, which is used both by the Serializer implementation and
// the package-scoped Marshal/Unmarshal functions in the ./direct package.
package cbor

import (
	"bytes"
	"encoding/hex"
	"errors"
	"io"
	"reflect"
	"testing"

	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/apimachinery/pkg/runtime/serializer/cbor/internal/modes"

	"github.com/google/go-cmp/cmp"
)

func TestRecognizesData(t *testing.T) {
	for _, tc := range []struct {
		in         []byte
		recognizes bool
	}{
		{
			in:         nil,
			recognizes: false,
		},
		{
			in:         []byte{},
			recognizes: false,
		},
		{
			in:         []byte{0xd9},
			recognizes: false,
		},
		{
			in:         []byte{0xd9, 0xd9},
			recognizes: false,
		},
		{
			in:         []byte{0xd9, 0xd9, 0xf7},
			recognizes: true,
		},
		{
			in:         []byte{0xff, 0xff, 0xff},
			recognizes: false,
		},
		{
			in:         []byte{0xd9, 0xd9, 0xf7, 0x01, 0x02, 0x03},
			recognizes: true,
		},
		{
			in:         []byte{0xff, 0xff, 0xff, 0x01, 0x02, 0x03},
			recognizes: false,
		},
	} {
		t.Run(hex.EncodeToString(tc.in), func(t *testing.T) {
			s := NewSerializer(nil, nil)
			recognizes, unknown, err := s.RecognizesData(tc.in)
			if recognizes != tc.recognizes {
				t.Errorf("expected recognized to be %t, got %t", tc.recognizes, recognizes)
			}
			if unknown {
				t.Error("expected unknown to be false, got true")
			}
			if err != nil {
				t.Errorf("expected nil error, got: %v", err)
			}
		})
	}
}

type stubWriter struct {
	n   int
	err error
}

func (w stubWriter) Write([]byte) (int, error) {
	return w.n, w.err
}

// anyObject wraps arbitrary concrete values to be encoded or decoded.
type anyObject struct {
	Value interface{}
}

func (p anyObject) GetObjectKind() schema.ObjectKind {
	return schema.EmptyObjectKind
}

func (anyObject) DeepCopyObject() runtime.Object {
	panic("unimplemented")
}

func (p anyObject) MarshalCBOR() ([]byte, error) {
	return modes.Encode.Marshal(p.Value)
}

func (p *anyObject) UnmarshalCBOR(in []byte) error {
	return modes.Decode.Unmarshal(in, &p.Value)
}

func TestEncode(t *testing.T) {
	for _, tc := range []struct {
		name           string
		in             runtime.Object
		assertOnWriter func() (io.Writer, func(*testing.T))
		assertOnError  func(*testing.T, error)
	}{
		{
			name: "io error writing self described cbor tag",
			assertOnWriter: func() (io.Writer, func(*testing.T)) {
				return stubWriter{err: io.ErrShortWrite}, func(*testing.T) {}
			},
			assertOnError: func(t *testing.T, err error) {
				if !errors.Is(err, io.ErrShortWrite) {
					t.Errorf("expected io.ErrShortWrite, got: %v", err)
				}
			},
		},
		{
			name: "output enclosed by self-described CBOR tag",
			in:   anyObject{},
			assertOnWriter: func() (io.Writer, func(*testing.T)) {
				var b bytes.Buffer
				return &b, func(t *testing.T) {
					if !bytes.HasPrefix(b.Bytes(), []byte{0xd9, 0xd9, 0xf7}) {
						t.Errorf("expected output to have prefix 0xd9d9f7: 0x%x", b.Bytes())
					}
				}
			},
			assertOnError: func(t *testing.T, err error) {
				if err != nil {
					t.Errorf("expected nil error, got: %v", err)
				}
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			s := NewSerializer(nil, nil)
			w, assertOnWriter := tc.assertOnWriter()
			err := s.Encode(tc.in, w)
			tc.assertOnError(t, err)
			assertOnWriter(t)
		})
	}
}

func TestDecode(t *testing.T) {
	for _, tc := range []struct {
		name          string
		options       []Option
		data          []byte
		gvk           *schema.GroupVersionKind
		metaFactory   metaFactory
		typer         runtime.ObjectTyper
		creater       runtime.ObjectCreater
		into          runtime.Object
		expectedObj   runtime.Object
		expectedGVK   *schema.GroupVersionKind
		assertOnError func(*testing.T, error)
	}{
		{
			name:        "error determining gvk",
			metaFactory: stubMetaFactory{err: errors.New("test")},
			assertOnError: func(t *testing.T, err error) {
				if err == nil || err.Error() != "test" {
					t.Errorf("expected error \"test\", got: %v", err)
				}
			},
		},
		{
			name:        "typer does not recognize into",
			gvk:         &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			typer:       notRegisteredTyper{},
			into:        &anyObject{},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if !runtime.IsNotRegisteredError(err) {
					t.Errorf("expected NotRegisteredError, got: %v", err)
				}
			},
		},
		{
			name:        "gvk from type of into",
			data:        []byte{0xf6},
			gvk:         &schema.GroupVersionKind{},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			typer:       stubTyper{gvks: []schema.GroupVersionKind{{Group: "x", Version: "y", Kind: "z"}}},
			into:        &anyObject{},
			expectedObj: &anyObject{},
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if err != nil {
					t.Errorf("expected nil error, got: %v", err)
				}
			},
		},
		{
			name:        "strict mode strict error",
			options:     []Option{Strict(true)},
			data:        []byte{0xa1, 0x61, 'z', 0x01}, // {'z': 1}
			gvk:         &schema.GroupVersionKind{},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			typer:       stubTyper{gvks: []schema.GroupVersionKind{{Group: "x", Version: "y", Kind: "z"}}},
			into:        &metav1.PartialObjectMetadata{},
			expectedObj: &metav1.PartialObjectMetadata{},
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if !runtime.IsStrictDecodingError(err) {
					t.Errorf("expected StrictDecodingError, got: %v", err)
				}
			},
		},
		{
			name:        "no strict mode no strict error",
			data:        []byte{0xa1, 0x61, 'z', 0x01}, // {'z': 1}
			gvk:         &schema.GroupVersionKind{},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			typer:       stubTyper{gvks: []schema.GroupVersionKind{{Group: "x", Version: "y", Kind: "z"}}},
			into:        &metav1.PartialObjectMetadata{},
			expectedObj: &metav1.PartialObjectMetadata{},
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if err != nil {
					t.Errorf("expected nil error, got: %v", err)
				}
			},
		},
		{
			name:        "unknown error from typer on into",
			gvk:         &schema.GroupVersionKind{},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			typer:       stubTyper{err: errors.New("test")},
			into:        &anyObject{},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{},
			assertOnError: func(t *testing.T, err error) {
				if err == nil || err.Error() != "test" {
					t.Errorf("expected error \"test\", got: %v", err)
				}
			},
		},
		{
			name:        "missing kind",
			gvk:         &schema.GroupVersionKind{Version: "v"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Version: "v"},
			assertOnError: func(t *testing.T, err error) {
				if !runtime.IsMissingKind(err) {
					t.Errorf("expected MissingKind, got: %v", err)
				}
			},
		},
		{
			name:        "missing version",
			gvk:         &schema.GroupVersionKind{Kind: "k"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Kind: "k"},
			assertOnError: func(t *testing.T, err error) {
				if !runtime.IsMissingVersion(err) {
					t.Errorf("expected MissingVersion, got: %v", err)
				}
			},
		},
		{
			name:        "creater error",
			gvk:         &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			creater:     stubCreater{err: errors.New("test")},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if err == nil || err.Error() != "test" {
					t.Errorf("expected error \"test\", got: %v", err)
				}
			},
		},
		{
			name:        "unmarshal error",
			data:        nil, // EOF
			gvk:         &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			creater:     stubCreater{obj: &anyObject{}},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if !errors.Is(err, io.EOF) {
					t.Errorf("expected EOF, got: %v", err)
				}
			},
		},
		{
			name:        "strict mode unmarshal error",
			options:     []Option{Strict(true)},
			data:        nil, // EOF
			gvk:         &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			creater:     stubCreater{obj: &anyObject{}},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if !errors.Is(err, io.EOF) {
					t.Errorf("expected EOF, got: %v", err)
				}
			},
		},
		{
			name:        "into unstructured unmarshal error",
			data:        nil, // EOF
			gvk:         &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			metaFactory: stubMetaFactory{gvk: &schema.GroupVersionKind{}},
			into:        &unstructured.Unstructured{},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Group: "x", Version: "y", Kind: "z"},
			assertOnError: func(t *testing.T, err error) {
				if !errors.Is(err, io.EOF) {
					t.Errorf("expected EOF, got: %v", err)
				}
			},
		},
		{
			name:        "into unstructured missing kind",
			data:        []byte("\xa1\x6aapiVersion\x61v"),
			into:        &unstructured.Unstructured{},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Version: "v"},
			assertOnError: func(t *testing.T, err error) {
				if !runtime.IsMissingKind(err) {
					t.Errorf("expected MissingKind, got: %v", err)
				}
			},
		},
		{
			name:        "into unstructured missing version",
			data:        []byte("\xa1\x64kind\x61k"),
			into:        &unstructured.Unstructured{},
			expectedObj: nil,
			expectedGVK: &schema.GroupVersionKind{Kind: "k"},
			assertOnError: func(t *testing.T, err error) {
				if !runtime.IsMissingVersion(err) {
					t.Errorf("expected MissingVersion, got: %v", err)
				}
			},
		},
		{
			name: "into unstructured",
			data: []byte("\xa2\x6aapiVersion\x61v\x64kind\x61k"),
			into: &unstructured.Unstructured{},
			expectedObj: &unstructured.Unstructured{Object: map[string]interface{}{
				"apiVersion": "v",
				"kind":       "k",
			}},
			expectedGVK: &schema.GroupVersionKind{Version: "v", Kind: "k"},
			assertOnError: func(t *testing.T, err error) {
				if err != nil {
					t.Errorf("expected nil error, got: %v", err)
				}
			},
		},
		{
			name:        "using unstructured creater",
			data:        []byte("\xa2\x6aapiVersion\x61v\x64kind\x61k"),
			metaFactory: &defaultMetaFactory{},
			creater:     stubCreater{obj: &unstructured.Unstructured{}},
			expectedObj: &unstructured.Unstructured{Object: map[string]interface{}{
				"apiVersion": "v",
				"kind":       "k",
			}},
			expectedGVK: &schema.GroupVersionKind{Version: "v", Kind: "k"},
			assertOnError: func(t *testing.T, err error) {
				if err != nil {
					t.Errorf("expected nil error, got: %v", err)
				}
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			s := newSerializer(tc.metaFactory, tc.creater, tc.typer, tc.options...)

			actualObj, actualGVK, err := s.Decode(tc.data, tc.gvk, tc.into)
			tc.assertOnError(t, err)

			if !reflect.DeepEqual(tc.expectedObj, actualObj) {
				t.Error(cmp.Diff(tc.expectedObj, actualObj))
			}

			if diff := cmp.Diff(tc.expectedGVK, actualGVK); diff != "" {
				t.Error(diff)
			}
		})
	}
}

func TestMetaFactoryInterpret(t *testing.T) {
	mf := &defaultMetaFactory{}
	_, err := mf.Interpret(nil)
	if err == nil {
		t.Error("expected non-nil error")
	}
	gvk, err := mf.Interpret([]byte("\xa2\x6aapiVersion\x63a/b\x64kind\x61c"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if diff := cmp.Diff(&schema.GroupVersionKind{Group: "a", Version: "b", Kind: "c"}, gvk); diff != "" {
		t.Error(diff)
	}
}

type stubTyper struct {
	gvks        []schema.GroupVersionKind
	unversioned bool
	err         error
}

func (t stubTyper) ObjectKinds(obj runtime.Object) ([]schema.GroupVersionKind, bool, error) {
	return t.gvks, t.unversioned, t.err
}

func (stubTyper) Recognizes(schema.GroupVersionKind) bool {
	return false
}

type stubCreater struct {
	obj runtime.Object
	err error
}

func (c stubCreater) New(gvk schema.GroupVersionKind) (runtime.Object, error) {
	return c.obj, c.err
}

type notRegisteredTyper struct{}

func (notRegisteredTyper) ObjectKinds(obj runtime.Object) ([]schema.GroupVersionKind, bool, error) {
	return nil, false, runtime.NewNotRegisteredErrForType("test", reflect.TypeOf(obj))
}

func (notRegisteredTyper) Recognizes(schema.GroupVersionKind) bool {
	return false
}

type stubMetaFactory struct {
	gvk *schema.GroupVersionKind
	err error
}

func (mf stubMetaFactory) Interpret([]byte) (*schema.GroupVersionKind, error) {
	return mf.gvk, mf.err
}
