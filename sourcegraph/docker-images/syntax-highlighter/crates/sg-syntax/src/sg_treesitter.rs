use paste::paste;
use protobuf::Message;
use std::collections::{HashMap, VecDeque};
use tree_sitter_highlight::Error;
use tree_sitter_highlight::{Highlight, HighlightEvent};

use rocket::serde::json::serde_json::json;
use rocket::serde::json::Value as JsonValue;
use tree_sitter_highlight::{HighlightConfiguration, Highlighter as TSHighlighter};

use crate::SourcegraphQuery;
use sg_lsif::{Document, Occurrence, SyntaxKind};
use sg_macros::include_project_file_optional;

#[rustfmt::skip]
// Table of (@CaptureGroup, SyntaxKind) mapping.
//
// Any capture defined in a query will be mapped to the following SyntaxKind via the highlighter.
//
// To extend what types of captures are included, simply add a line below that takes a particular
// match group that you're interested in and map it to a new SyntaxKind.
//
// We can also define our own new capture types that we want to use and add to queries to provide
// particular highlights if necessary.
//
// (I can also add per-language mappings for these if we want, but you could also just do that with
//  unique match groups. For example `@rust-bracket`, or similar. That doesn't need any
//  particularly new rust code to be written. You can just modify queries for that)
const MATCHES_TO_SYNTAX_KINDS: &[(&str, SyntaxKind)] = &[
    ("attribute",               SyntaxKind::UnspecifiedSyntaxKind),
    ("boolean",                 SyntaxKind::BooleanLiteral),
    ("comment",                 SyntaxKind::Comment),
    ("conditional",             SyntaxKind::IdentifierKeyword),
    ("constant",                SyntaxKind::IdentifierConstant),
    ("constant.builtin",        SyntaxKind::IdentifierBuiltin),
    ("constant.null",           SyntaxKind::IdentifierNull),
    ("float",                   SyntaxKind::NumericLiteral),
    ("function",                SyntaxKind::IdentifierFunctionDefinition),
    ("function.builtin",        SyntaxKind::IdentifierBuiltin),
    ("identifier",              SyntaxKind::Identifier),
    ("identifier.function",     SyntaxKind::IdentifierFunctionDefinition),
    ("include",                 SyntaxKind::IdentifierKeyword),
    ("keyword",                 SyntaxKind::IdentifierKeyword),
    ("keyword.function",        SyntaxKind::IdentifierKeyword),
    ("keyword.return",          SyntaxKind::IdentifierKeyword),
    ("method",                  SyntaxKind::IdentifierFunction),
    ("number",                  SyntaxKind::NumericLiteral),
    ("operator",                SyntaxKind::IdentifierOperator),
    ("property",                SyntaxKind::Identifier),
    ("punctuation",             SyntaxKind::UnspecifiedSyntaxKind),
    ("punctuation.bracket",     SyntaxKind::UnspecifiedSyntaxKind),
    ("punctuation.delimiter",   SyntaxKind::PunctuationDelimiter),
    ("string",                  SyntaxKind::StringLiteral),
    ("string.special",          SyntaxKind::StringLiteral),
    ("tag",                     SyntaxKind::UnspecifiedSyntaxKind),
    ("type",                    SyntaxKind::IdentifierType),
    ("type.builtin",            SyntaxKind::IdentifierType),
    ("variable",                SyntaxKind::Identifier),
    ("variable.builtin",        SyntaxKind::UnspecifiedSyntaxKind),
    ("variable.parameter",      SyntaxKind::IdentifierParameter),
    ("variable.module",         SyntaxKind::IdentifierModule),
];

/// Maps a highlight to a syntax kind.
/// This only works if you've correctly used the highlight_names from MATCHES_TO_SYNTAX_KINDS
fn get_syntax_kind_for_hl(hl: Highlight) -> SyntaxKind {
    MATCHES_TO_SYNTAX_KINDS[hl.0].1
}

/// Add a language highlight configuration to the CONFIGURATIONS global.
///
/// This makes it so you don't have to understand how configurations are added,
/// just add the name of filetype that you want.
macro_rules! create_configurations {
    ( $($name: tt),* ) => {{
        let mut m = HashMap::new();
        let highlight_names = MATCHES_TO_SYNTAX_KINDS.iter().map(|hl| hl.0).collect::<Vec<&str>>();

        $(
            {
                // Create HighlightConfiguration language
                let mut lang = HighlightConfiguration::new(
                    paste! { [<tree_sitter_ $name>]::language() },
                    include_project_file_optional!("queries/", $name, "/highlights.scm"),
                    include_project_file_optional!("queries/", $name, "/injections.scm"),
                    include_project_file_optional!("queries/", $name, "/locals.scm"),
                ).expect(stringify!("parser for '{}' must be compiled", $name));

                // Associate highlights with configuration
                lang.configure(&highlight_names);

                // Insert into configurations, so we only create once at startup.
                m.insert(stringify!($name), lang);
            }
        )*

        m
    }}
}

lazy_static::lazy_static! {
    static ref CONFIGURATIONS: HashMap<&'static str, HighlightConfiguration> = {
        create_configurations!( go, sql, c_sharp )
    };
}

pub fn jsonify_err(e: impl ToString) -> JsonValue {
    json!({"error": e.to_string()})
}

pub fn lsif_highlight(q: SourcegraphQuery) -> Result<JsonValue, JsonValue> {
    let filetype = q
        .filetype
        .ok_or_else(|| json!({"error": "Must pass a filetype for /lsif" }))?
        .to_lowercase();

    if !CONFIGURATIONS.contains_key(filetype.as_str()) {
        Err(json!({
            "error": format!("{} is not a valid filetype for treesitter", filetype)
        }))
    } else {
        let data = index_language(&filetype, &q.code).map_err(jsonify_err)?;
        let encoded = data.write_to_bytes().map_err(jsonify_err)?;

        Ok(json!({"data": base64::encode(&encoded), "plaintext": false}))
    }
}

pub fn index_language(filetype: &str, code: &str) -> Result<Document, Error> {
    match CONFIGURATIONS.get(filetype) {
        Some(lang_config) => index_language_with_config(code, lang_config),
        None => Err(Error::InvalidLanguage),
    }
}

pub fn make_highlight_config(name: &str, highlights: &str) -> Option<HighlightConfiguration> {
    let config = CONFIGURATIONS.get(name)?;

    // Create HighlightConfiguration language
    let mut lang = match HighlightConfiguration::new(config.language, highlights, "", "") {
        Ok(lang) => lang,
        Err(_) => return None,
    };

    // Associate highlights with configuration
    let highlight_names = MATCHES_TO_SYNTAX_KINDS
        .iter()
        .map(|hl| hl.0)
        .collect::<Vec<&str>>();
    lang.configure(&highlight_names);

    Some(lang)
}

pub fn index_language_with_config(
    code: &str,
    lang_config: &HighlightConfiguration,
) -> Result<Document, Error> {
    // TODO: We should automatically apply no highlights when we are
    // in an injected piece of code.
    //
    // Unfortunately, that information isn't currently available when
    // we are iterating in the higlighter.
    let mut highlighter = TSHighlighter::new();
    let highlights = highlighter.highlight(lang_config, code.as_bytes(), None, |l| {
        CONFIGURATIONS.get(l)
    })?;

    let mut emitter = LsifEmitter::new();
    emitter.render(highlights, code, &get_syntax_kind_for_hl)
}

struct LineManager {
    offsets: Vec<usize>,
}

impl LineManager {
    fn new(s: &str) -> Result<Self, Error> {
        if s.is_empty() {
            // TODO: Make an error here
            // Error(
        }

        let mut offsets = Vec::new();
        let mut pos = 0;
        for line in s.lines() {
            offsets.push(pos);
            pos += line.len() + 1;
        }

        Ok(Self { offsets })
    }

    fn line_and_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 0;
        for window in self.offsets.windows(2) {
            let curr = window[0];
            let next = window[1];
            if next > offset {
                return (line, offset - curr);
            }

            line += 1;
        }

        (line, offset - self.offsets.last().unwrap())
    }

    fn range(&self, start: usize, end: usize) -> Vec<i32> {
        let start_line = self.line_and_col(start);
        let end_line = self.line_and_col(end);

        if start_line.0 == end_line.0 {
            vec![start_line.0 as i32, start_line.1 as i32, end_line.1 as i32]
        } else {
            vec![
                start_line.0 as i32,
                start_line.1 as i32,
                end_line.0 as i32,
                end_line.1 as i32,
            ]
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PackedRange {
    pub start_line: i32,
    pub start_col: i32,
    pub end_line: i32,
    pub end_col: i32,
}

impl PackedRange {
    pub fn from_vec(v: &[i32]) -> Self {
        match v.len() {
            3 => Self {
                start_line: v[0],
                start_col: v[1],
                end_line: v[0],
                end_col: v[2],
            },
            4 => Self {
                start_line: v[0],
                start_col: v[1],
                end_line: v[2],
                end_col: v[3],
            },
            _ => {
                panic!("Unexpected vector length: {:?}", v);
            }
        }
    }
}

impl PartialOrd for PackedRange {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (self.start_line, self.end_line, self.start_col).partial_cmp(&(
            other.start_line,
            other.end_line,
            other.start_col,
        ))
    }
}

impl Ord for PackedRange {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.start_line, self.end_line, self.start_col).cmp(&(
            other.start_line,
            other.end_line,
            other.start_col,
        ))
    }
}

/// Converts a general-purpose syntax highlighting iterator into a sequence of lines of HTML.
pub struct LsifEmitter {}

/// Our version of `tree_sitter_highlight::HtmlRenderer`, which emits stuff as a table.
///
/// You can see the original version in the tree_sitter_highlight crate.
impl LsifEmitter {
    pub fn new() -> Self {
        LsifEmitter {}
    }

    pub fn render<F>(
        &mut self,
        highlighter: impl Iterator<Item = Result<HighlightEvent, Error>>,
        source: &str,
        _attribute_callback: &F,
    ) -> Result<Document, Error>
    where
        F: Fn(Highlight) -> SyntaxKind,
    {
        // let mut highlights = Vec::new();
        let mut doc = Document::new();

        let line_manager = LineManager::new(source)?;

        let mut highlights = vec![];
        for event in highlighter {
            match event {
                Ok(HighlightEvent::HighlightStart(s)) => highlights.push(s),
                Ok(HighlightEvent::HighlightEnd) => {
                    highlights.pop();
                }

                // No highlights matched
                Ok(HighlightEvent::Source { .. }) if highlights.is_empty() => {}

                // When a `start`->`end` has some highlights
                Ok(HighlightEvent::Source { start, end }) => {
                    let mut occurence = Occurrence::new();
                    occurence.range = line_manager.range(start, end);
                    occurence.syntax_kind = get_syntax_kind_for_hl(*highlights.last().unwrap());

                    doc.occurrences.push(occurence);
                }
                Err(a) => return Err(a),
            }
        }

        Ok(doc)
    }
}

pub fn dump_document(doc: &Document, source: &str) -> String {
    dump_document_range(doc, source, &None)
}

pub struct FileRange {
    pub start: usize,
    pub end: usize,
}

pub fn dump_document_range(doc: &Document, source: &str, file_range: &Option<FileRange>) -> String {
    let mut occurences = doc.get_occurrences().to_owned();
    occurences.sort_by_key(|o| PackedRange::from_vec(&o.range));
    let mut occurences = VecDeque::from(occurences);

    let mut result = String::new();

    let line_iterator: Box<dyn Iterator<Item = (usize, &str)>> = match file_range {
        Some(range) => Box::new(
            source
                .lines()
                .enumerate()
                .skip(range.start - 1)
                .take(range.end - range.start + 1),
        ),
        None => Box::new(source.lines().enumerate()),
    };

    for (idx, line) in line_iterator {
        result += "  ";
        result += &line.replace("\t", " ");
        result += "\n";

        while let Some(occ) = occurences.pop_front() {
            if occ.syntax_kind == SyntaxKind::UnspecifiedSyntaxKind {
                continue;
            }

            let range = PackedRange::from_vec(&occ.range);
            if range.start_line != range.end_line {
                continue;
            }

            if range.start_line < idx as i32 {
                continue;
            } else if range.start_line > idx as i32 {
                occurences.push_front(occ);
                break;
            }

            let length = (range.end_col - range.start_col) as usize;

            result.push_str(&format!(
                "//{}{} {:?}\n",
                " ".repeat(range.start_col as usize),
                "^".repeat(length),
                occ.syntax_kind
            ));
        }
    }

    result
}

#[cfg(test)]
mod test {
    use std::{
        fs::{read_dir, File},
        io::Read,
    };

    use crate::determine_filetype;

    use super::*;

    #[test]
    fn test_highlights_one_comment() -> Result<(), Error> {
        let src = "// Hello World";
        let document = index_language("go", src)?;
        insta::assert_snapshot!(dump_document(&document, src));

        Ok(())
    }

    #[test]
    fn test_highlights_simple_main() -> Result<(), Error> {
        let src = r#"package main
import "fmt"

func main() {
	fmt.Println("Hello, world", 5)
}
"#;

        let document = index_language("go", src)?;
        insta::assert_snapshot!(dump_document(&document, src));

        Ok(())
    }

    #[test]
    fn test_highlights_a_sql_query_within_go() -> Result<(), Error> {
        let src = r#"package main

const MySqlQuery = `
SELECT * FROM my_table
`
"#;

        let document = index_language("go", src)?;
        insta::assert_snapshot!(dump_document(&document, src));

        Ok(())
    }

    #[test]
    fn test_highlight_csharp_file() -> Result<(), Error> {
        let src = "using System;";
        let document = index_language("c_sharp", src)?;
        insta::assert_snapshot!(dump_document(&document, src));

        Ok(())
    }

    #[test]
    fn test_all_files() -> Result<(), std::io::Error> {
        let dir = read_dir("./src/snapshots/files/")?;
        for entry in dir {
            let entry = entry?;
            let filepath = entry.path();
            let mut file = File::open(&filepath)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            let filetype = &determine_filetype(&SourcegraphQuery {
                extension: filepath.extension().unwrap().to_str().unwrap().to_string(),
                filepath: filepath.to_str().unwrap().to_string(),
                filetype: None,
                css: false,
                line_length_limit: None,
                theme: "".to_string(),
                code: contents.clone(),
            });

            println!("Filetype: {filetype}");

            let document = index_language(filetype, &contents).unwrap();
            insta::assert_snapshot!(
                filepath
                    .to_str()
                    .unwrap()
                    .replace("/src/snapshots/files", ""),
                dump_document(&document, &contents)
            );
        }

        Ok(())
    }
}
