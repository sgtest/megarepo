package graphqlbackend

import (
	"context"
	"fmt"
	"testing"

	"github.com/graph-gophers/graphql-go"
	gqlerrors "github.com/graph-gophers/graphql-go/errors"
	"github.com/stretchr/testify/require"
	"google.golang.org/protobuf/types/known/structpb"

	"github.com/sourcegraph/sourcegraph/internal/database/dbmocks"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
)

type mockTelemetryResolver struct {
	events []TelemetryEventInput
}

func (m *mockTelemetryResolver) RecordEvents(_ context.Context, args *RecordEventsArgs) (*EmptyResponse, error) {
	m.events = append(m.events, args.Events...)
	return &EmptyResponse{}, nil
}

func TestTelemetryRecordEvents(t *testing.T) {
	for _, tc := range []struct {
		name string
		// Write a raw GraphQL event because we want to test providing the raw input
		// value, as if from a client, which the Variables field in RunTest doesn't
		// seem to accept right (it wants the final type, which defeats the point)
		gqlEventsInput string
		// Assertions on received events.
		assert func(t *testing.T, gotEvents []TelemetryEventInput)
	}{
		{
			name: "object privateMetadata",
			gqlEventsInput: `
				{
					feature: "cody.fixup"
					action: "applied"
					source: {
						client: "VSCode.Cody",
						clientVersion: "0.14.1"
					}
					parameters: {
						version: 0
						metadata: [
							{
								key: "contextSelection",
								value: 1
							},
							{
								key: "chatPredictions",
								value: 0
							},
						]
						privateMetadata: {key:"value"}
					}
				}
			`,
			assert: func(t *testing.T, gotEvents []TelemetryEventInput) {
				// Check PrivateMetadata
				require.Len(t, gotEvents, 1)
				value := gotEvents[0].Parameters.PrivateMetadata.Value
				require.NotNil(t, value)
				v, ok := value.(map[string]any)
				require.True(t, ok)
				require.Equal(t, "value", v["key"])

				// Sanity check strucpb marshalling used in cmd/frontend/internal/telemetry/resolvers
				_, err := structpb.NewStruct(v)
				require.NoError(t, err)
			},
		},
		{
			name: "string privateMetadata",
			gqlEventsInput: `
				{
					feature: "cody.fixup"
					action: "applied"
					source: {
						client: "VSCode.Cody",
						clientVersion: "0.14.1"
					}
					parameters: {
						version: 0
						metadata: [
							{
								key: "contextSelection",
								value: 1
							},
							{
								key: "chatPredictions",
								value: 0
							},
						]
						privateMetadata: "some value"
					}
				}
			`,
			assert: func(t *testing.T, gotEvents []TelemetryEventInput) {
				// Check PrivateMetadata
				require.Len(t, gotEvents, 1)
				value := gotEvents[0].Parameters.PrivateMetadata.Value
				require.NotNil(t, value)
				v, ok := value.(string)
				require.True(t, ok)
				require.Equal(t, "some value", v)

				// Sanity check strucpb marshalling used in cmd/frontend/internal/telemetry/resolvers
				_, err := structpb.NewValue(value)
				require.NoError(t, err)
			},
		},
		{
			name: "numeric privateMetadata",
			gqlEventsInput: `
				{
					feature: "cody.fixup"
					action: "applied"
					source: {
						client: "VSCode.Cody",
						clientVersion: "0.14.1"
					}
					parameters: {
						version: 0
						metadata: [
							{
								key: "contextSelection",
								value: 1
							},
							{
								key: "chatPredictions",
								value: 0
							},
						]
						privateMetadata: 12
					}
				}
			`,
			assert: func(t *testing.T, gotEvents []TelemetryEventInput) {
				// Check PrivateMetadata
				require.Len(t, gotEvents, 1)
				value := gotEvents[0].Parameters.PrivateMetadata.Value
				require.NotNil(t, value)
				v, ok := value.(int32)
				require.Truef(t, ok, "got %T", value)
				require.Equal(t, int32(12), v)

				// Sanity check strucpb marshalling used in cmd/frontend/internal/telemetry/resolvers
				_, err := structpb.NewValue(value)
				require.NoError(t, err)
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			mockResolver := &mockTelemetryResolver{}
			parsedSchema, err := NewSchema(
				dbmocks.NewMockDB(),
				gitserver.NewTestClient(t),
				[]OptionalResolver{{
					TelemetryRootResolver: &TelemetryRootResolver{Resolver: mockResolver},
				}},
				graphql.PanicHandler(printStackTrace{&gqlerrors.DefaultPanicHandler{}}),
			)
			require.NoError(t, err)

			// Parallel must start here, as NewSchema is not concurrency-safe
			// (it assigns to a global variable).
			tc := tc
			t.Parallel()

			// Check all fields accepted in GraphQL resolver.
			RunTest(t, &Test{
				Schema:  parsedSchema,
				Context: context.Background(),
				Query: fmt.Sprintf(`mutation RecordTelemetryEvents() {
					telemetry {
						recordEvents(events: [%s]) {
							alwaysNil
						}
					}
				}`, tc.gqlEventsInput),
				ExpectedResult: `{
					"telemetry": {
						"recordEvents": {
							"alwaysNil": null
						}
					}
				}`,
			})

			// Run assertions defined by test case
			tc.assert(t, mockResolver.events)
		})
	}
}
