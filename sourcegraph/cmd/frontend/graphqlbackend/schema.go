package graphqlbackend

import (
	_ "embed"
)

// mainSchema is the main raw graqhql schema.
//go:embed schema.graphql
var mainSchema string

// batchesSchema is the Batch Changes raw graqhql schema.
//go:embed batches.graphql
var batchesSchema string

// codeIntelSchema is the Code Intel raw graqhql schema.
//go:embed codeintel.graphql
var codeIntelSchema string

// dotcomSchema is the Dotcom schema extension raw graqhql schema.
//go:embed dotcom.graphql
var dotcomSchema string

// licenseSchema is the Licensing raw graqhql schema.
//go:embed license.graphql
var licenseSchema string

// codeMonitorsSchema is the Code Monitoring raw graqhql schema.
//go:embed code_monitors.graphql
var codeMonitorsSchema string

// insightsSchema is the Code Insights raw graqhql schema.
//go:embed insights.graphql
var insightsSchema string

// authzSchema is the Authz raw graqhql schema.
//go:embed authz.graphql
var authzSchema string

// computeSchema is an experimental graphql endpoint for computing values from search results.
//go:embed compute.graphql
var computeSchema string

// searchContextsSchema is the Search Contexts raw graqhql schema.
//go:embed search_contexts.graphql
var searchContextsSchema string

// orgSchema is the schema containing enterprise-only functionality of
// organization repositories.
//go:embed org.graphql
var orgSchema string

// notebooksSchema is the Notebooks raw graqhql schema.
//go:embed notebooks.graphql
var notebooksSchema string

// dependenciesSchema is the dependencies raw graqhql schema.
//go:embed dependencies.graphql
var dependenciesSchema string
