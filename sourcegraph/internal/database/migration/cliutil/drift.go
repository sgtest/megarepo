package cliutil

import (
	"context"
	"fmt"
	"net/url"
	"sort"

	"cuelang.org/go/pkg/strings"
	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/drift"
	descriptions "github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func Drift(commandName string, factory RunnerFactory, outFactory OutputFactory, development bool, expectedSchemaFactories ...ExpectedSchemaFactory) *cli.Command {
	defaultVersion := ""
	if development {
		defaultVersion = "HEAD"
	}

	schemaNameFlag := &cli.StringFlag{
		Name:     "schema",
		Usage:    "The target `schema` to compare. Possible values are 'frontend', 'codeintel' and 'codeinsights'",
		Required: true,
		Aliases:  []string{"db"},
	}
	versionFlag := &cli.StringFlag{
		Name: "version",
		Usage: "The target schema version. Can be a version (e.g. 5.0.2) or resolvable as a git revlike on the Sourcegraph repository " +
			"(e.g. a branch, tag or commit hash).",
		Required: false,
		Value:    defaultVersion,
	}
	fileFlag := &cli.StringFlag{
		Name:     "file",
		Usage:    "The target schema description file.",
		Required: false,
	}
	skipVersionCheckFlag := &cli.BoolFlag{
		Name:     "skip-version-check",
		Usage:    "Skip validation of the instance's current version.",
		Required: false,
		Value:    development,
	}
	ignoreMigratorUpdateCheckFlag := &cli.BoolFlag{
		Name:     "ignore-migrator-update",
		Usage:    "Ignore the running migrator not being the latest version. It is recommended to use the latest migrator version.",
		Required: false,
	}
	// Only in available via `sg migration`` in development mode
	autofixFlag := &cli.BoolFlag{
		Name:     "auto-fix",
		Usage:    "Database goes brrrr.",
		Required: false,
		Aliases:  []string{"autofix"},
	}

	action := makeAction(outFactory, func(ctx context.Context, cmd *cli.Context, out *output.Output) error {
		airgapped := isAirgapped(ctx)
		if airgapped != nil {
			out.WriteLine(output.Line(output.EmojiWarningSign, output.StyleYellow, airgapped.Error()))
		}

		if airgapped == nil {
			latest, hasUpdate, err := checkForMigratorUpdate(ctx)
			if err != nil {
				out.WriteLine(output.Linef(output.EmojiWarningSign, output.StyleYellow, "Failed to check for migrator update: %s. Continuing...", err))
			} else if hasUpdate {
				noticeStr := fmt.Sprintf("A newer migrator version is available (%s), please consider using it instead", latest)
				if ignoreMigratorUpdateCheckFlag.Get(cmd) {
					out.WriteLine(output.Linef(output.EmojiWarningSign, output.StyleYellow, "%s. Continuing...", noticeStr))
				} else {
					return cli.Exit(fmt.Sprintf("%s %s%s or pass -ignore-migrator-update.%s", output.EmojiWarning, output.StyleWarning, noticeStr, output.StyleReset), 1)
				}
			}
		}

		schemaName := TranslateSchemaNames(schemaNameFlag.Get(cmd), out)
		version := versionFlag.Get(cmd)
		file := fileFlag.Get(cmd)
		skipVersionCheck := skipVersionCheckFlag.Get(cmd)

		r, err := factory([]string{schemaName})
		if err != nil {
			return err
		}
		store, err := r.Store(ctx, schemaName)
		if err != nil {
			return err
		}

		if version != "" && file != "" {
			return errors.New("the flags -version or -file are mutually exclusive")
		}

		parsedVersion, patch, ok := oobmigration.NewVersionAndPatchFromString(version)
		// if not parsable into a structured version, then it may be a revhash
		if ok && parsedVersion.GitTagWithPatch(patch) != version {
			out.WriteLine(output.Linef(output.EmojiLightbulb, output.StyleGrey, "Parsed %q from version flag value %q", parsedVersion.GitTagWithPatch(patch), version))
			version = parsedVersion.GitTagWithPatch(patch)
		}

		if !skipVersionCheck {
			inferred, patch, ok, err := GetServiceVersion(ctx, r)
			if err != nil {
				return err
			}
			if !ok {
				err := fmt.Sprintf("version assertion failed: unknown version != %q", version)
				return errors.Newf("%s. Re-invoke with --skip-version-check to ignore this check", err)
			}

			if version == "" {
				version = inferred.GitTagWithPatch(patch)
				out.WriteLine(output.Linef(output.EmojiInfo, output.StyleReset, "Checking drift against version %q", version))
			} else if version != inferred.GitTagWithPatch(patch) {
				err := fmt.Sprintf("version assertion failed: %q != %q", inferred, version)
				return errors.Newf("%s. Re-invoke with --skip-version-check to ignore this check", err)
			}
		} else if version == "" && file == "" {
			return errors.New("-skip-version-check was supplied without -version or -file")
		}

		if file != "" {
			expectedSchemaFactories = []ExpectedSchemaFactory{
				NewExplicitFileSchemaFactory(file),
			}
		}

		expectedSchema, err := FetchExpectedSchema(ctx, schemaName, version, out, expectedSchemaFactories)
		if err != nil {
			return err
		}

		schemas, err := store.Describe(ctx)
		if err != nil {
			return err
		}
		schema := schemas["public"]
		summaries := drift.CompareSchemaDescriptions(schemaName, version, Canonicalize(schema), Canonicalize(expectedSchema))

		if autofixFlag.Get(cmd) {
			summaries, err = attemptAutofix(ctx, out, store, summaries, func(schema descriptions.SchemaDescription) []drift.Summary {
				return drift.CompareSchemaDescriptions(schemaName, version, Canonicalize(schema), Canonicalize(expectedSchema))
			})
			if err != nil {
				return err
			}
		}

		return displayDriftSummaries(out, summaries)
	})

	flags := []cli.Flag{
		schemaNameFlag,
		versionFlag,
		fileFlag,
		skipVersionCheckFlag,
		ignoreMigratorUpdateCheckFlag,
	}
	if development {
		flags = append(flags, autofixFlag)
	}

	return &cli.Command{
		Name:        "drift",
		Usage:       "Detect differences between the current database schema and the expected schema",
		Description: ConstructLongHelp(),
		Action:      action,
		Flags:       flags,
	}
}

func FetchExpectedSchema(
	ctx context.Context,
	schemaName string,
	version string,
	out *output.Output,
	expectedSchemaFactories []ExpectedSchemaFactory,
) (descriptions.SchemaDescription, error) {
	filename, err := getSchemaJSONFilename(schemaName)
	if err != nil {
		return descriptions.SchemaDescription{}, err
	}

	out.WriteLine(output.Line(output.EmojiInfo, output.StyleReset, "Locating schema description"))

	for i, factory := range expectedSchemaFactories {
		matches := false
		patterns := factory.VersionPatterns()
		for _, pattern := range patterns {
			if pattern.MatchString(version) {
				matches = true
				break
			}
		}
		if len(patterns) > 0 && !matches {
			continue
		}

		resourcePath := factory.ResourcePath(filename, version)
		expectedSchema, err := factory.CreateFromPath(ctx, resourcePath)
		if err != nil {
			suffix := ""
			if i < len(expectedSchemaFactories)-1 {
				suffix = " Will attempt a fallback source."
			}

			out.WriteLine(output.Linef(output.EmojiInfo, output.StyleReset, "Reading schema definition in %s (%s)... Schema not found (%s).%s", factory.Name(), resourcePath, err, suffix))
			continue
		}

		out.WriteLine(output.Linef(output.EmojiSuccess, output.StyleReset, "Schema found in %s (%s).", factory.Name(), resourcePath))
		return expectedSchema, nil
	}

	exampleMap := map[string]struct{}{}
	failedPaths := map[string]struct{}{}
	for _, factory := range expectedSchemaFactories {
		for _, pattern := range factory.VersionPatterns() {
			if !pattern.MatchString(version) {
				exampleMap[pattern.Example()] = struct{}{}
			} else {
				failedPaths[factory.ResourcePath(filename, version)] = struct{}{}
			}
		}
	}

	versionExamples := make([]string, 0, len(exampleMap))
	for pattern := range exampleMap {
		versionExamples = append(versionExamples, pattern)
	}
	sort.Strings(versionExamples)

	paths := make([]string, 0, len(exampleMap))
	for path := range failedPaths {
		if u, err := url.Parse(path); err == nil && (u.Scheme == "http" || u.Scheme == "https") {
			paths = append(paths, path)
		}
	}
	sort.Strings(paths)

	if len(paths) > 0 {
		var additionalHints string
		if len(versionExamples) > 0 {
			additionalHints = fmt.Sprintf(
				"Alternative, provide a different version that matches one of the following patterns: \n  - %s\n", strings.Join(versionExamples, "\n  - "),
			)
		}

		out.WriteLine(output.Linef(
			output.EmojiLightbulb,
			output.StyleFailure,
			"Schema not found. "+
				"Check if the following resources exist. "+
				"If they do, then the context in which this migrator is being run may not be permitted to reach the public internet."+
				"\n  - %s\n%s",
			strings.Join(paths, "\n  - "),
			additionalHints,
		))
	} else if len(versionExamples) > 0 {
		out.WriteLine(output.Linef(
			output.EmojiLightbulb,
			output.StyleFailure,
			"Schema not found. Ensure your supplied version matches one of the following patterns: \n  - %s\n", strings.Join(versionExamples, "\n  - "),
		))
	}

	return descriptions.SchemaDescription{}, errors.New("failed to locate target schema description")
}

func Canonicalize(schemaDescription descriptions.SchemaDescription) descriptions.SchemaDescription {
	descriptions.Canonicalize(schemaDescription)

	filtered := schemaDescription.Tables[:0]
	for i, table := range schemaDescription.Tables {
		if table.Name == "migration_logs" {
			continue
		}

		filtered = append(filtered, schemaDescription.Tables[i])
	}
	schemaDescription.Tables = filtered

	return schemaDescription
}
