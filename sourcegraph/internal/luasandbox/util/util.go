package util

import (
	lua "github.com/yuin/gopher-lua"
	luar "layeh.com/gopher-luar"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// CreateModule wraps a map of functions into a lua.LGFunction suitable for
// use in CreateOptions.Modules.
func CreateModule(api map[string]lua.LGFunction) lua.LGFunction {
	return WrapLuaFunction(func(state *lua.LState) error {
		t := state.NewTable()
		state.SetFuncs(t, api)
		state.Push(t)
		return nil
	})
}

// WrapLuaFunction invokes the given callback and returns 1 on success. This assumes
// the underlying function pushed a single return value onto the stack. An error is
// raised on failure (and the stack is assumed to be untouched).
func WrapLuaFunction(f func(state *lua.LState) error) func(state *lua.LState) int {
	return func(state *lua.LState) int {
		if err := f(state); err != nil {
			state.RaiseError(err.Error())
			return 0
		}

		return 1
	}
}

// WrapSoftFailingLuaFunction invokes the given callback and returns 1 on success. This
// assumes the underlying function pushed a single return value onto the stack. A nil value
// and the error message is pushed to the stack on failure and 2 is returned. This allows
// the `value, err = call()` idiom.
func WrapSoftFailingLuaFunction(f func(state *lua.LState) error) func(state *lua.LState) int {
	return func(state *lua.LState) int {
		if err := f(state); err != nil {
			state.Push(lua.LNil)
			state.Push(luar.New(state, err.Error()))
			return 2
		}

		return 1
	}
}

// DecodeTable decodes the given table value into a map from string keys to Lua values.
// For each key present in the given decoders map, the associated decoder is invoked with
// the key's value. A table with non-string keys, a key absent from the given decoders map,
// or an error from the decoder invocation all result in an error from this function.
func DecodeTable(table *lua.LTable, decoders map[string]func(lua.LValue) error) error {
	return ForEach(table, func(key, value lua.LValue) error {
		fieldName, err := assertLuaString(key)
		if err != nil {
			return err
		}

		decoder, ok := decoders[fieldName]
		if !ok {
			return errors.Newf("unexpected field %s", fieldName)
		}

		return decoder(value)
	})
}

// ForEach invokes the given callback on each key/value pair in the given table. An
// error produced by the callback will skip invocation on any remaining keys.
func ForEach(value lua.LValue, f func(key, value lua.LValue) error) (err error) {
	table, ok := value.(*lua.LTable)
	if !ok {
		return NewTypeError("table", value)
	}

	table.ForEach(func(key, value lua.LValue) {
		if err == nil {
			err = f(key, value)
		}
	})

	return
}

// SetString returns a decoder function that updates the given string value on
// invocation. For use in DecodeTable.
func SetString(ptr *string) func(lua.LValue) error {
	return func(value lua.LValue) (err error) {
		*ptr, err = assertLuaString(value)
		return
	}
}

// SetStrings returns a decoder function that updates the given string slice value
// on invocation. For use in DecodeTable.
func SetStrings(ptr *[]string) func(lua.LValue) error {
	return func(value lua.LValue) (err error) {
		values, err := DecodeSlice(value)
		if err != nil {
			return err
		}

		for _, v := range values {
			str, err := assertLuaString(v)
			if err != nil {
				return err
			}

			*ptr = append(*ptr, str)
		}

		return nil
	}
}

// SetLuaFunction returns a decoder function that updates the given Lua function
// value on invocation. For use in DecodeTable.
func SetLuaFunction(ptr **lua.LFunction) func(lua.LValue) error {
	return func(value lua.LValue) (err error) {
		*ptr, err = assertLuaFunction(value)
		return
	}
}

// DecodeSlice reads the values off of the given table and collects them into a
// slice. This function returns an error if the value has an unexpected type.
func DecodeSlice(value lua.LValue) (values []lua.LValue, _ error) {
	table, ok := value.(*lua.LTable)
	if !ok {
		return nil, NewTypeError("table", value)
	}

	if err := ForEach(table, func(key, value lua.LValue) error {
		if table.Len() == 0 {
			// There are key/value pairs but they're associative, not indexed.
			// Throw here as we're decoding a table that's of an unexpected
			// shape.
			return NewTypeError("array", value)
		}

		values = append(values, value)
		return nil
	}); err != nil {
		return nil, err
	}

	return values, nil
}

// UnwrapLuaUserData invokes the given callback with the value within the given
// user data value. This function returns an error if the given type is not a
// pointer to user data.
func UnwrapLuaUserData(value lua.LValue, f func(any) error) error {
	userData, err := assertUserData(value)
	if err != nil {
		return err
	}

	return f(userData.Value)
}

// UnwrapSliceOrSingleton attempts to unwrap the given Lua value as a slice, then
// call the given callback over each element of the slice. If the given value does
// not seem to be a slice, then the callback is invoked once with the entire payload.
func UnwrapSliceOrSingleton(value lua.LValue, f func(lua.LValue) error) error {
	values, err := DecodeSlice(value)
	if err != nil {
		return f(value)
	}

	for _, value := range values {
		if err := f(value); err != nil {
			return err
		}
	}

	return nil
}

// NewTypeError creates an error with the given expected and actual value type.
func NewTypeError(expectedType string, actualValue any) error {
	return errors.Newf("wrong type: expecting %s, have %T", expectedType, actualValue)
}

// assertLuaString returns the given value as a string or an error if the value is
// of a different type.
func assertLuaString(value lua.LValue) (string, error) {
	if value.Type() != lua.LTString {
		return "", NewTypeError("string", value)
	}

	return lua.LVAsString(value), nil
}

// assertLuaFunction returns the given value as a function or an error if the value is
// of a different type.
func assertLuaFunction(value lua.LValue) (*lua.LFunction, error) {
	f, ok := value.(*lua.LFunction)
	if !ok {
		return nil, NewTypeError("function", value)
	}

	return f, nil
}

// assertUserData returns the given value as a pointer to user data or an error if the
// value is of a different type.
func assertUserData(value lua.LValue) (*lua.LUserData, error) {
	if value.Type() != lua.LTUserData {
		return nil, NewTypeError("UserData", value)
	}

	return value.(*lua.LUserData), nil
}
