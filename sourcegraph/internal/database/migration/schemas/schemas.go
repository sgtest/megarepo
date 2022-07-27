package schemas

import (
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/definition"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/migrations"
)

var (
	Frontend     = mustResolveSchema("frontend")
	CodeIntel    = mustResolveSchema("codeintel")
	CodeInsights = mustResolveSchema("codeinsights")

	Schemas = []*Schema{
		Frontend,
		CodeIntel,
		CodeInsights,
	}
)

func mustResolveSchema(name string) *Schema {
	fs, err := fs.Sub(migrations.QueryDefinitions, name)
	if err != nil {
		panic(fmt.Sprintf("malformed migration definitions %q: %s", name, err))
	}

	schema, err := ResolveSchema(fs, name)
	if err != nil {
		panic(err.Error())
	}

	return schema
}

func ResolveSchema(fs fs.FS, name string) (*Schema, error) {
	definitions, err := definition.ReadDefinitions(fs, filepath.Join("migrations", name))
	if err != nil {
		return nil, errors.Newf("malformed migration definitions %q: %s", name, err)
	}

	return &Schema{
		Name:                name,
		MigrationsTableName: strings.TrimPrefix(fmt.Sprintf("%s_schema_migrations", name), "frontend_"),
		FS:                  fs,
		Definitions:         definitions,
	}, nil
}

// FilterSchemasByName returns a copy of the given schemas slice containing only schema matching the given
// set of names.
func FilterSchemasByName(schemas []*Schema, targetNames []string) []*Schema {
	filtered := make([]*Schema, 0, len(schemas))
	for _, schema := range schemas {
		for _, targetName := range targetNames {
			if targetName == schema.Name {
				filtered = append(filtered, schema)
				break
			}
		}
	}

	return filtered
}
