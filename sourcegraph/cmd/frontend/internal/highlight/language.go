package highlight

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/go-enry/go-enry/v2"
	"github.com/grafana/regexp"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
)

type EngineType int64

const (
	EngineInvalid EngineType = iota
	EngineTreeSitter
	EngineSyntect
)

var engineToDisplay map[EngineType]string = map[EngineType]string{
	EngineInvalid:    "invalid",
	EngineSyntect:    "syntect",
	EngineTreeSitter: "tree-sitter",
}

type languagePattern struct {
	pattern  *regexp.Regexp
	language string
}

// TODO: Later get an exhaustive list for this, or add to documentation.
type syntaxHighlightConfig struct {
	// Order does not matter. Evaluated before Patterns
	Extensions map[string]string

	// Order matters for this. First matching pattern matches.
	// Matches against the entire string.
	Patterns []languagePattern
}

type syntaxEngineConfig struct {
	Default   EngineType
	Overrides map[string]EngineType
}

type SyntaxEngineQuery struct {
	Engine           EngineType
	Language         string
	LanguageOverride bool
}

var highlightConfig = syntaxHighlightConfig{
	Extensions: map[string]string{},
	Patterns:   []languagePattern{},
}

var engineConfig = syntaxEngineConfig{
	// This sets the default syntax engine for the sourcegraph server.
	Default:   EngineSyntect,
	Overrides: map[string]EngineType{},
}

func init() {
	conf.ContributeValidator(func(c conftypes.SiteConfigQuerier) (problems conf.Problems) {
		highlights := c.SiteConfig().SyntaxHighlighting
		if highlights == nil {
			return
		}

		if _, ok := engineNameToEngineType(highlights.Engine.Default); !ok {
			problems = append(problems, conf.NewSiteProblem(fmt.Sprintf("Not a valid highlights.Engine.Default: `%s`.", highlights.Engine.Default)))
		}

		for _, engine := range highlights.Engine.Overrides {
			if _, ok := engineNameToEngineType(engine); !ok {
				problems = append(problems, conf.NewSiteProblem(fmt.Sprintf("Not a valid highlights.Engine.Default: `%s`.", engine)))
			}
		}

		for _, pattern := range highlights.Languages.Patterns {
			if _, err := regexp.Compile(pattern.Pattern); err != nil {
				problems = append(problems, conf.NewSiteProblem(fmt.Sprintf("Not a valid regexp: `%s`. See the valid syntax: https://golang.org/pkg/regexp/", pattern.Pattern)))
			}
		}

		return
	})

	go func() {
		conf.Watch(func() {
			config := conf.Get()
			if config == nil {
				return
			}

			if config.SyntaxHighlighting == nil {
				return
			}

			if defaultEngine, ok := engineNameToEngineType(config.SyntaxHighlighting.Engine.Default); ok {
				engineConfig.Default = defaultEngine
			}

			engineConfig.Overrides = map[string]EngineType{}
			for name, engine := range config.SyntaxHighlighting.Engine.Overrides {
				if overrideEngine, ok := engineNameToEngineType(engine); ok {
					engineConfig.Overrides[strings.ToLower(name)] = overrideEngine
				}
			}

			highlightConfig.Extensions = config.SyntaxHighlighting.Languages.Extensions
			highlightConfig.Patterns = []languagePattern{}
			for _, pattern := range config.SyntaxHighlighting.Languages.Patterns {
				if re, err := regexp.Compile(pattern.Pattern); err == nil {
					highlightConfig.Patterns = append(highlightConfig.Patterns, languagePattern{pattern: re, language: pattern.Language})
				}
			}
		})
	}()
}

func engineNameToEngineType(engineName string) (engine EngineType, ok bool) {
	switch engineName {
	case "tree-sitter":
		return EngineTreeSitter, true
	case "syntect":
		return EngineSyntect, true
	default:
		return EngineInvalid, false
	}
}

// Matches against config. Only returns values if there is a match.
func getLanguageFromConfig(config syntaxHighlightConfig, path string) (string, bool) {
	extension := strings.ToLower(strings.TrimPrefix(filepath.Ext(path), "."))
	if ft, ok := config.Extensions[extension]; ok {
		return ft, true
	}

	for _, pattern := range config.Patterns {
		if pattern.pattern != nil && pattern.pattern.MatchString(path) {
			return pattern.language, true
		}
	}

	return "", false
}

// getLanguage will return the name of the language and default back to enry if
// no language could be found.
func getLanguage(path string, contents string) (string, bool) {
	ft, found := getLanguageFromConfig(highlightConfig, path)
	if found {
		return ft, true
	}

	return enry.GetLanguage(path, []byte(contents)), false
}

// DetectSyntaxHighlightingLanguage will calculate the SyntaxEngineQuery from a given
// path and contents. First it will determine if there are any configuration overrides
// and then, if none, return the 'enry' default language detection
func DetectSyntaxHighlightingLanguage(path string, contents string) SyntaxEngineQuery {
	lang, langOverride := getLanguage(path, contents)
	lang = strings.ToLower(lang)

	engine := engineConfig.Default
	if overrideEngine, ok := engineConfig.Overrides[lang]; ok {
		engine = overrideEngine
	}

	return SyntaxEngineQuery{
		Language:         lang,
		LanguageOverride: langOverride,
		Engine:           engine,
	}
}
