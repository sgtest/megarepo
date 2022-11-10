package graphqlbackend

import (
	"encoding/json"
	"strconv"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// BigInt implements the BigInt GraphQL scalar type.
type BigInt int64

func (BigInt) ImplementsGraphQLType(name string) bool {
	return name == "BigInt"
}

func (v BigInt) MarshalJSON() ([]byte, error) {
	return json.Marshal(strconv.FormatInt(int64(v), 10))
}

func (v *BigInt) UnmarshalGraphQL(input any) error {
	s, ok := input.(string)
	if !ok {
		return errors.Errorf("invalid GraphQL BigInt scalar value input (got %T, expected string)", input)
	}
	n, err := strconv.ParseInt(s, 10, 64)
	if err != nil {
		return err
	}
	*v = BigInt(n)
	return nil
}
