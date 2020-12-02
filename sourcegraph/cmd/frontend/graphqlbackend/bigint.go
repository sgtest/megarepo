package graphqlbackend

import (
	"encoding/json"
	"fmt"
	"strconv"
)

// BigInt implements the BigInt GraphQL scalar type.
type BigInt struct{ Int uint64 }

// BigIntOrNil is a helper function that returns nil for int == nil and otherwise wraps int in
// BigInt.
func BigIntOrNil(int *uint64) *BigInt {
	if int == nil {
		return nil
	}
	return &BigInt{Int: *int}
}

func (BigInt) ImplementsGraphQLType(name string) bool {
	return name == "BigInt"
}

func (v BigInt) MarshalJSON() ([]byte, error) {
	return json.Marshal(strconv.FormatUint(v.Int, 10))
}

func (v *BigInt) UnmarshalGraphQL(input interface{}) error {
	s, ok := input.(string)
	if !ok {
		return fmt.Errorf("invalid GraphQL BigInt scalar value input (got %T, expected string)", input)
	}
	n, err := strconv.ParseUint(s, 10, 64)
	if err != nil {
		return err
	}
	*v = BigInt{Int: n}
	return nil
}
