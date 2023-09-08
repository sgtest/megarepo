// Package jsonitere wraps json-iterator/go's Iterator methods with error returns
// so linting can catch unchecked errors.
// The underlying iterator's Error property is returned and not reset.
// See json-iterator/go for method documentation and additional methods that
// can be added to this library.
package jsonitere

import (
	j "github.com/json-iterator/go"
)

type Iterator struct {
	// named property instead of embedded so there is no
	// confusion about which method or property is called
	i *j.Iterator
}

func NewIterator(i *j.Iterator) *Iterator {
	return &Iterator{i}
}

func (iter *Iterator) Read() (any, error) {
	return iter.i.Read(), iter.i.Error
}

func (iter *Iterator) ReadAny() (j.Any, error) {
	return iter.i.ReadAny(), iter.i.Error
}

func (iter *Iterator) ReadArray() (bool, error) {
	return iter.i.ReadArray(), iter.i.Error
}

func (iter *Iterator) ReadObject() (string, error) {
	return iter.i.ReadObject(), iter.i.Error
}

func (iter *Iterator) ReadString() (string, error) {
	return iter.i.ReadString(), iter.i.Error
}

func (iter *Iterator) WhatIsNext() (j.ValueType, error) {
	return iter.i.WhatIsNext(), iter.i.Error
}

func (iter *Iterator) Skip() error {
	iter.i.Skip()
	return iter.i.Error
}

func (iter *Iterator) SkipAndReturnBytes() []byte {
	return iter.i.SkipAndReturnBytes()
}

func (iter *Iterator) ReadVal(obj any) error {
	iter.i.ReadVal(obj)
	return iter.i.Error
}

func (iter *Iterator) ReadFloat64() (float64, error) {
	return iter.i.ReadFloat64(), iter.i.Error
}

func (iter *Iterator) ReadInt8() (int8, error) {
	return iter.i.ReadInt8(), iter.i.Error
}
