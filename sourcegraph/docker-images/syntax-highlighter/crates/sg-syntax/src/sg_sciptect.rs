use std::{collections::HashSet, fmt::Debug};

use once_cell::sync::OnceCell;
use protobuf::{EnumOrUnknown, SpecialFields};
use scip::types::{Document, Occurrence, SyntaxKind};
use syntect::{
    parsing::{BasicScopeStackOp, ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

static EMPTY_SCOPE: OnceCell<Scope> = OnceCell::new();
fn empty_scope() -> Scope {
    *EMPTY_SCOPE.get_or_init(|| Scope::new("").unwrap())
}
// Whenever a scope matches any of the scopes in IGNORED_SCOPES,
// it will not emit an occurrence for that range. The most specific
// scope that overlaps this region will instead emit an occurrence for
// that range (if applicable).
static IGNORED_SCOPES: OnceCell<Vec<Scope>> = OnceCell::new();
fn should_skip_scope(scope: &Scope) -> bool {
    IGNORED_SCOPES
        .get_or_init(|| {
            // See match_scope_to_kind
            let scope = |s| Scope::new(s).unwrap();
            vec![
                scope("source"),
                scope("meta.tag.tsx"),
                scope("meta.tag.without-attributes.tsx"),
                scope("punctuation.definition.tag.begin.tsx"),
                scope("punctuation.definition.tag.end.tsx"),
                scope("entity.other.document.begin.yaml"),
                scope("punctuation.definition.heading.begin"),
                scope("punctuation.definition.heading.end"),
                scope("punctuation.definition.bold.begin"),
                scope("punctuation.definition.bold.end"),
                scope("punctuation.definition.italic.begin"),
                scope("punctuation.definition.italic.end"),
                scope("punctuation.definition.strikethrough.begin"),
                scope("punctuation.definition.strikethrough.end"),
                scope("punctuation.definition.raw.begin"),
                scope("punctuation.definition.raw.end"),
                scope("punctuation.definition.link.begin"),
                scope("punctuation.definition.link.end"),
                scope("punctuation.definition.metadata.begin"),
                scope("punctuation.definition.metadata.end"),
                scope("punctuation.definition.string.begin"),
                scope("punctuation.definition.string.end"),
                scope("punctuation.definition.comment"),
            ]
        })
        .iter()
        .any(|ignored| ignored.is_prefix_of(*scope))
}

// Maps scopes to SyntaxKind. Runs after checking if a scope is in IGNORED_SCOPES
static SCOPE_MATCHES: OnceCell<Vec<(Scope, SyntaxKind)>> = OnceCell::new();

#[rustfmt::skip]
fn match_scope_to_kind(scope: &Scope) -> Option<SyntaxKind> {
    let scope_matches: &Vec<(Scope, SyntaxKind)> = SCOPE_MATCHES.get_or_init(|| {
        use SyntaxKind::*;

        // TODO: We should probably make sure that we can't even ship syntax-highlighter if this
        // doesn't work (which should happen because it won't be able to pass tests or do anything
        // without this)
        //
        // The only way (as far as I can tell) this can fail is if you pass in a Scope with >=8
        // atoms (so we just won't do that here). This only runs once, so we don't have to worry
        // about subsequent failures for any of these unwraps.
        let scope = |s| Scope::new(s).unwrap();

        // These are IN ORDER.
        //  If you want something to resolve to something more specifically or as a higher priority
        //  make sure to place the scope(...) at the beginning of the list.
        vec![
            (scope("comment"), Comment),
            (scope("meta.documentation"), Comment),
            // TODO: How does this play with this: keyword.control.import.include
            (scope("meta.preprocessor.include"), IdentifierNamespace),
            (scope("storage.type.keyword"), IdentifierKeyword),
            (scope("entity.name.function"), IdentifierFunction),
            (scope("entity.name.type"), IdentifierType),
            // TODO: optimization opportunity, skip testing language-specific scopes.
            (
                scope("keyword.operator.expression.keyof.ts"),
                IdentifierKeyword,
            ),
            (
                scope("keyword.operator.expression.keyof.tsx"),
                IdentifierKeyword,
            ),
            (
                scope("keyword.operator.expression.typeof.ts"),
                IdentifierKeyword,
            ),
            (
                scope("keyword.operator.expression.typeof.tsx"),
                IdentifierKeyword,
            ),
            (scope("storage.type.namespace.ts"), IdentifierKeyword),
            (scope("storage.type.namespace.tsx"), IdentifierKeyword),
            (scope("storage.type.module.js"), IdentifierKeyword),
            (scope("storage.type.module.jsx"), IdentifierKeyword),
            (scope("storage.type.module.ts"), IdentifierKeyword),
            (scope("storage.type.module.tsx"), IdentifierKeyword),
            (scope("storage.type.interface.ts"), IdentifierKeyword),
            (scope("storage.type.interface.tsx"), IdentifierKeyword),
            (scope("storage.type.class.ts"), IdentifierKeyword),
            (scope("storage.type.class.tsx"), IdentifierKeyword),
            (scope("storage.type.class.js"), IdentifierKeyword),
            (scope("storage.type.class.jsx"), IdentifierKeyword),
            (scope("storage.type.type.tsx"), IdentifierKeyword),
            (scope("storage.type.type.ts"), IdentifierKeyword),
            (scope("storage.type.enum.tsx"), IdentifierKeyword),
            (scope("storage.type.enum.ts"), IdentifierKeyword),
            (scope("storage.type.function.tsx"), IdentifierKeyword),
            (scope("storage.type.function.ts"), IdentifierKeyword),
            (scope("storage.type.function.js"), IdentifierKeyword),
            (scope("storage.type.function.jsx"), IdentifierKeyword),
            (scope("storage.type.ts"), IdentifierKeyword),
            (scope("storage.type.tsx"), IdentifierKeyword),
            (scope("keyword.operator.logical.sql"), IdentifierKeyword),
            (
                scope("keyword.operator.assignment.alias.sql"),
                IdentifierKeyword,
            ),
            (scope("meta.mapping.key.json"), StringLiteralKey),
            (scope("entity.name.tag.yaml"), StringLiteralKey),
            (scope("entity.other.attribute-name.class.css"), Identifier),
            (scope("meta.link.inline.markdown"), StringLiteral),
            (scope("storage.type.scala"), IdentifierKeyword),
            (scope("storage.type.stable.scala"), IdentifierKeyword),
            (scope("storage.type.class.scala"), IdentifierKeyword),
            (scope("storage.type.function.scala"), IdentifierKeyword),
            (scope("storage.type.volatile.scala"), IdentifierKeyword),
            // (scope("entity.name.section.markdown"), IdentifierType),
            (scope("meta.tag"), Identifier),
            (scope("markup.bold"), Identifier),
            (scope("markup.underline"), Identifier),
            (scope("markup.italic"), Identifier),
            (scope("markup.raw"), StringLiteral),
            (scope("markup.heading"), Identifier),
            (scope("keyword.operator"), IdentifierOperator),
            (scope("keyword"), IdentifierKeyword),
            (scope("variable.language.this"), IdentifierBuiltin),
            (scope("variable.function"), IdentifierFunction),
            (scope("meta.definition.property"), IdentifierAttribute),
            (scope("variable"), Identifier),
            (scope("constant.character.escape"), StringLiteralEscape),
            (scope("string"), StringLiteral),
            (scope("constant.numeric"), NumericLiteral),
            (scope("constant.character"), CharacterLiteral),
            (scope("constant.language"), IdentifierBuiltin),
            (scope("storage.modifier.array"), PunctuationBracket),
            (scope("storage.modifier"), IdentifierKeyword),
            (scope("storage.type.namespace"), IdentifierNamespace),
            (scope("storage.type.ts"), IdentifierKeyword),
            (scope("storage.type"), IdentifierType),
            (scope("support.type.builtin"), IdentifierBuiltinType),
            (scope("meta.object-literal.key"), IdentifierAttribute),
            (scope("meta.path"), IdentifierNamespace),
            // (scope("meta.type"), IdentifierType), Intentionally disabled in favor of more precise classes
            (scope("meta.return.type"), IdentifierType),
            (scope("support.type"), IdentifierType),
            (scope("support.class"), IdentifierType),
            (scope("support.function"), IdentifierFunction),
            (scope("support.variable"), Identifier),
            (scope("entity.other.attribute-name"), TagAttribute),
            (scope("entity.name"), Identifier),
            (scope("entity.other"), Identifier),
            //
            // Punctuation Types: while these may appear noisy, they're
            // intentionally included so that punctutation characters get
            // correctly highlighted when nested inside other occurrences like
            // interpolated string literals. Example: the braces in `a${b}`.
            (scope("punctuation.section.mapping"), PunctuationBracket),
            (scope("punctuation.section.sequence"), PunctuationBracket),
            (scope("punctuation.terminator"), PunctuationDelimiter),
            (scope("meta.brace"), PunctuationBracket),
            (scope("punctuation"), PunctuationBracket),
        ]
    });

    scope_matches
        .iter()
        .find(|&(prefix, _)| prefix.is_prefix_of(*scope))
        .map(|&(_, kind)| kind)
}

/// The DocumentGenerator generate a Document with occurrences set to the corresponding syntax kinds
///
/// If max_line_len is not None, any lines with length greater than the
/// provided number will not be highlighted.
pub struct DocumentGenerator<'a> {
    syntax_set: &'a SyntaxSet,
    parse_state: ParseState,
    code: &'a str,
    max_line_len: Option<usize>,
}

#[derive(Clone)]
struct HighlightStart {
    row: i32,
    col: i32,
    kind: Option<SyntaxKind>,
    scope: Scope,
}

impl HighlightStart {
    fn some(row: usize, col: usize, kind: SyntaxKind, scope: Scope) -> Self {
        Self {
            row: row as i32,
            col: col as i32,
            kind: Some(kind),
            scope,
        }
    }

    fn none() -> Self {
        Self {
            row: 0,
            col: 0,
            kind: None,
            scope: empty_scope(),
        }
    }
}

impl Debug for HighlightStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            Some(kind) => write!(f, "HighlightStart({}, {}, {:?})", self.row, self.col, kind),
            None => write!(f, "<None>",),
        }
    }
}

#[derive(Default)]
struct HighlightManager {
    highlights: Vec<HighlightStart>,
}

// HighlightManager is used to keep track of the scope of highlights that we have and make sure
// that we never push overlapping ranges and that we always have ranges sorted by start offset
// (that part we should get for free).
//
// So given a situation like this:
// "asdf"
// ^        Punctuation
// ^^^^^^   String
//      ^   Punctuation
//
// HighlightManager will transform this to:
//
// "asdf"
// ^        Punctuation
//  ^^^^    String
//      ^   Punctuation
//
// Note: The parts where string previous overlapped the punctuation
// is no longer the case.
impl HighlightManager {
    fn push_hl(&mut self, hl: HighlightStart) -> Option<HighlightStart> {
        let mut existing_hl = None;

        // If there was an existing highlight, we need to modify it
        // so that the range is smaller than it would be otherwise.
        // This prevents overlapping ranges.
        //
        // (see the documentation above for HighlightManager)
        if let Some(last_hl) = self.highlights.last_mut() {
            // TODO: Avoid this hack to get string literal keys to take priority over strings for JSON.
            if last_hl.kind == Some(SyntaxKind::StringLiteralKey)
                && hl.kind == Some(SyntaxKind::StringLiteral)
            {
                return Some(last_hl.clone());
            }
            if let Some(_kind) = last_hl.kind {
                existing_hl = Some(last_hl.clone());
                last_hl.row = hl.row;
                last_hl.col = hl.col;
            }
        }

        self.highlights.push(hl);

        existing_hl
    }

    fn push_empty(&mut self) {
        self.highlights.push(HighlightStart::none());
    }

    fn pop_hl(&mut self, row: usize, character: usize) -> Option<HighlightStart> {
        let row = row as i32;
        let character = character as i32;

        let hl = self.highlights.pop();
        if let Some(hl) = &hl {
            // Modify all previous highlights that started at this location.
            //  Make sure that we set their start row and column to whatever this partial
            //  highlight is ending at. This makes sure that we don't have any overlapping
            //  highlights.
            for prev_hl in self.highlights.iter_mut().rev() {
                if prev_hl.row != hl.row || prev_hl.col != hl.col {
                    break;
                }

                prev_hl.row = row;
                prev_hl.col = character;
            }
        }

        hl
    }
}

impl Debug for HighlightManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.highlights.is_empty() {
            return write!(f, "HighlightManager(None)");
        }

        writeln!(f, "HighlightManager {{")?;
        for hl in self.highlights.iter() {
            writeln!(f, "  {:?}", hl)?;
        }

        write!(f, "}}")
    }
}

impl<'a> DocumentGenerator<'a> {
    pub fn new(
        ss: &'a SyntaxSet,
        sr: &SyntaxReference,
        code: &'a str,
        max_line_len: Option<usize>,
    ) -> Self {
        Self {
            code,
            syntax_set: ss,
            parse_state: ParseState::new(sr),
            max_line_len,
        }
    }

    // generate takes ownership of self so that it can't be re-used
    pub fn generate(mut self) -> Document {
        let mut document = Document::default();

        let mut stack = ScopeStack::new();
        let mut unhandled_scopes = HashSet::new();
        let mut highlight_manager = HighlightManager::default();
        for (row, line_contents) in LinesWithEndings::from(self.code).enumerate() {
            // Do not attempt to parse very long lines
            if self.max_line_len.map_or(false, |n| line_contents.len() > n) {
                continue;
            }

            let ops = self.parse_state.parse_line(line_contents, self.syntax_set);

            for &(byte_offset, ref op) in ops.as_slice() {
                // Character represents the nth character in a line.
                // This can be roughly thought of as column, but non-single-width
                // characters confuse this situation.
                let character = match line_contents
                    .char_indices()
                    .enumerate()
                    .find(|(_, (offset, _))| *offset == byte_offset)
                {
                    Some(char) => char.0,
                    None => line_contents.chars().count() - 1,
                };

                stack.apply_with_hook(op, |basic_op, _stack| {
                    match basic_op {
                        BasicScopeStackOp::Push(scope) => {
                            // We have to push PartialHighight to the stack
                            // so that when we come to `pop` these highlights they still pop.
                            if should_skip_scope(&scope) {
                                highlight_manager.push_empty();
                                return;
                            }

                            match match_scope_to_kind(&scope) {
                                Some(kind) => {
                                    // Uncomment to debug what scopes are picked up
                                    // println!("SCOPE {row:>3}:{character:<3} {:50} {kind:?}", format!("{}", scope));
                                    let partial_hl =
                                        HighlightStart::some(row, character, kind, scope);
                                    if let Some(partial_hl) = highlight_manager.push_hl(partial_hl)
                                    {
                                        push_document_occurence(
                                            &mut document,
                                            scope,
                                            &partial_hl,
                                            row,
                                            character,
                                        );
                                    };
                                }
                                None => {
                                    // println!("SCOPE {row:>3}:{character:<3} {:50}", format!("{}", scope));
                                    unhandled_scopes.insert(scope);
                                    highlight_manager.push_empty();
                                }
                            }
                        }
                        BasicScopeStackOp::Pop => {
                            // TODO: Consider that we should return Result<Option<hl>>
                            //  This way we can assert that we _always_ have a balanced scope stack
                            //  (never pop past what we've pushed) and still easily skip the
                            //  highlights that are useless.
                            if let Some(partial_hl) = highlight_manager.pop_hl(row, character) {
                                push_document_occurence(
                                    &mut document,
                                    partial_hl.scope,
                                    &partial_hl,
                                    row,
                                    character,
                                );
                            }
                        }
                    }
                });
            }
        }

        // Only panic in test code, this condition should only result
        // in one line not being highlighted correctly, so we can just
        // continue on in production.
        if cfg!(test) {
            if highlight_manager
                .highlights
                .iter()
                .filter(|hl| hl.kind.is_some())
                .count()
                > 0
            {
                panic!("unhandled highlights in: {:?}", highlight_manager);
            }

            if !unhandled_scopes.is_empty() {
                // TODO: We can use this to reduce unhandled scopes to 0 in test cases
                //       I will leave it up to the later reader or me :)
                // panic!("Unhandled Scopes: {:?}", unhandled_scopes);
            }
        }

        // If we have some highlights that are still open when we end the file,
        // then we need to close them with the range that is the very end of the contents
        if let Some(end_of_line) = LinesWithEndings::from(self.code)
            .enumerate()
            .last()
            .map(|(row, line)| (row, line.chars().count()))
        {
            while let Some(partial_hl) = highlight_manager.pop_hl(end_of_line.0, end_of_line.1) {
                push_document_occurence(
                    &mut document,
                    partial_hl.scope,
                    &partial_hl,
                    end_of_line.0,
                    end_of_line.1,
                );
            }
        }

        document
    }
}

fn push_document_occurence(
    document: &mut Document,
    scope: Scope,
    partial_hl: &HighlightStart,
    row: usize,
    col: usize,
) {
    let row = row as i32;
    let col = col as i32;

    // Do not emit ranges that are empty
    if (partial_hl.row, partial_hl.col) == (row, col) {
        return;
    }

    if let Some(kind) = partial_hl.kind {
        document.occurrences.push(new_occurence(
            vec![partial_hl.row, partial_hl.col, row, col],
            kind,
            scope,
        ));
    }
}

fn new_occurence(range: Vec<i32>, syntax_kind: SyntaxKind, scope: Scope) -> Occurrence {
    let syntax_kind = EnumOrUnknown::new(syntax_kind);
    let range = match range.len() {
        4 => {
            if range[0] == range[2] {
                vec![range[0], range[1], range[3]]
            } else {
                range
            }
        }
        _ => range,
    };

    let symbol = if cfg!(test) {
        scope.to_string()
    } else {
        String::default()
    };
    Occurrence {
        range,
        syntax_kind,
        symbol_roles: 0,
        symbol,
        override_documentation: vec![],
        diagnostics: vec![],
        ..Default::default()
    }
}

#[cfg(test)]
mod test {
    use std::{
        env,
        fs::{read_dir, File},
        io::Read,
    };

    use pretty_assertions::assert_eq;
    use scip_treesitter::snapshot::{
        dump_document_with_config, EmitSymbol, EmitSyntax, SnapshotOptions,
    };

    fn snapshot_sciptect_documents(doc: &Document, source: &str) -> String {
        dump_document_with_config(
            doc,
            source,
            SnapshotOptions {
                emit_syntax: EmitSyntax::Highlighted,
                emit_symbol: EmitSymbol::Unqualified,
                ..Default::default()
            },
        )
        .expect("dump document")
    }

    use super::*;
    use crate::{determine_language, SourcegraphQuery};

    #[test]
    fn test_generates_empty_file() {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let q = crate::SourcegraphQuery {
            filetype: Some("go".to_string()),
            code: "".to_string(),
            ..Default::default()
        };

        let syntax_def = determine_language(&q, &syntax_set).unwrap();
        let output = DocumentGenerator::new(&syntax_set, syntax_def, &q.code, q.line_length_limit)
            .generate();

        assert_eq!(Document::default(), output);
    }

    #[test]
    fn test_all_files() -> Result<(), std::io::Error> {
        let ss = SyntaxSet::load_defaults_newlines();
        let mut failed = vec![];

        let crate_root: std::path::PathBuf = std::env::var("CARGO_MANIFEST_DIR").unwrap().into();
        let input_dir = crate_root
            .join("src")
            .join("snapshots")
            .join("syntect_files");

        let dir = read_dir(&input_dir)?;

        let filter = env::args()
            .last()
            .and_then(|x| x.strip_prefix("only=").map(|x| x.to_owned()))
            .unwrap_or("".to_owned()); // run everything

        for entry in dir {
            let entry = entry?;

            if !entry.file_name().to_str().unwrap().contains(&filter) {
                continue;
            }
            let filepath = entry.path();
            let mut file = File::open(&filepath)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            let q = SourcegraphQuery {
                extension: filepath.extension().unwrap().to_str().unwrap().to_string(),
                filepath: filepath.to_str().unwrap().to_string(),
                filetype: None,
                css: false,
                line_length_limit: None,
                theme: "".to_string(),
                code: contents.clone(),
            };
            let syntax_def = determine_language(&q, &ss).unwrap();
            let document = DocumentGenerator::new(&ss, syntax_def, &q.code, None).generate();

            // As far as I can tell, there is no "matches_snapshot" or similar for `insta`.
            // So we'll just catch the panic for now, push the results and then panic at the end
            // with all the failed files (if applicable)
            match std::panic::catch_unwind(|| {
                insta::assert_snapshot!(
                    filepath.strip_prefix(&input_dir).unwrap().to_str().unwrap(),
                    snapshot_sciptect_documents(&document, &contents)
                );
            }) {
                Ok(_) => {}
                Err(_) => failed.push(entry),
            }
        }

        assert!(failed.is_empty(), "Failed: {:?}", failed);

        Ok(())
    }
}
