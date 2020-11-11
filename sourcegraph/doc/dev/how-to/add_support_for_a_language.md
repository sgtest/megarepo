# How to add support for a language

The following guides detail the steps necessary to add or upgrade support for a particular programming language.

## Symbols support

To support symbol search and the symbols sidebar:

1. Add or update the target language's configuration in [sourcegraph/go-ctags](https://github.com/sourcegraph/go-ctags)/[.ctags.d](https://github.com/sourcegraph/go-ctags/tree/main/ctagsdotd). The [universal-ctags/ctags](https://github.com/universal-ctags/ctags) project bundles configuration for many languages, but additional or override configuration may be necessary to support missing or correct incorrectly parsed language features. Examples:
    - [scala](https://github.com/sourcegraph/go-ctags/blob/main/ctagsdotd/scala.ctags) (new language)
    - [clojure](https://github.com/sourcegraph/go-ctags/blob/main/ctagsdotd/clojure.ctags) (additional patterns)
    - [css](https://github.com/sourcegraph/go-ctags/blob/main/ctagsdotd/css.ctags) (additional file extensions)
1. Update the [sourcegraph/go-ctags](https://github.com/sourcegraph/go-ctags) dependency in [sourcegraph/zoekt](https://github.com/sourcegraph/zoekt).
1. Update the [sourcegraph/go-ctags](https://github.com/sourcegraph/go-ctags) dependency in [sourcegraph/sourcegraph](https://github.com/sourcegraph/sourcegraph).
1. Run `./dev/zoekt/update` in [sourcegraph/sourcegraph](https://github.com/sourcegraph/sourcegraph) to pull in the new zoekt version.

#### Development

You can validate your changes to the `.ctag.d` definitions by observing the symbol search and symbol sidebar behaviors in a local Sourcegraph. Link your Sourcegraph instance and your local clone of go-ctags by adding the following to the `go.mod` file in [sourcegraph/sourcegraph](https://github.com/sourcegraph/sourcegraph).

```
replace github.com/sourcegraph/go-ctags => /path/to/my/clone/of/sourcegraph/go-ctags
```

Remember to run the code-generation step in the go-ctags repository and restart your local instance after each change to a definition file.

**Note**: Because we have not yet updated Zoekt, you will need to ensure that your symbol search and symbol sidebar operations are not on an indexed branch. These paths must hit the _unindexed_ symbols paths in order to hit the code path with updated ctags definitions.

## Code intelligence support

To support precise code intelligence, [write an LSIF indexer](../../code_intelligence/explanations/writing_an_indexer.md). To support basic code intelligence, ensure a `sourcegraph/{lang}` Sourcegraph extension is published to the official extension registry:

0. _Code intelligence extensions are powered by symbol search. If the target language is not supported by symbols, stop and follow the guide above first._
1. Add (or update) the target language's configuration in [languages.ts](https://github.com/sourcegraph/code-intel-extensions/blob/e255e3776f213b30f2c073b98e0a959cad67c19c/shared/language-specs/languages.ts#L336). See the definition of [LanguageSpec](https://github.com/sourcegraph/code-intel-extensions/blob/e255e3776f213b30f2c073b98e0a959cad67c19c/shared/language-specs/spec.ts#L7) for an available set of fields. The likely differences will be the characters that make up the identifier, the comment delimiters, and the set of file extensions to search within for definitions and references.
1. Ensure an [icon](https://github.com/sourcegraph/code-intel-extensions/tree/e255e3776f213b30f2c073b98e0a959cad67c19c/icons) exists for the target language. This ensures that the BuildKite pipeline will generate and publish an extension for the new language definition.
1. Correlate the language's file extensions and the new Sourcegraph extension by adding entries to the switch in [getModeFromExtension](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@a4517f560a1c312e5effd6d3a858b76b56936e0e/-/blob/client/shared/src/languages.ts#L40:10). This enables the providers from the Sourcegraph extension to be registered when a text document with the correlated extensions is opened. The value returned from this function and the `languageId` from the language's configuration should match exactly.

## Syntax highlighting support

To support syntax highlighting on code files, search results, diff views, and more:

1. Follow the [directions](https://github.com/sourcegraph/syntect_server#adding-languages) to add a language to [syntect server](https://github.com/sourcegraph/syntect_server).
1. Update the [SyntectLanguageMap](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@56a9eec78566499b108e1f869712865d90cc29cf/-/blob/internal/highlight/syntect_language_map.go#L5:5) in [sourcegraph/sourcegraph](https://github.com/sourcegraph/sourcegraph).

To support syntax highlighting in hovers:

1. Update the [highlight.js contributions](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@e7ffd56b10e9bae004dfbb5d7d1c1accc93072fd/-/blob/client/shared/src/highlight/contributions.ts#L21) map in [sourcegraph/sourcegraph](https://github.com/sourcegraph/sourcegraph).

## Search support

Coming soon.
