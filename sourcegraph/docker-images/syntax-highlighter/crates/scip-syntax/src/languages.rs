use scip_macros::include_scip_query;
use scip_treesitter_languages::parsers::BundledParser;
use tree_sitter::{Language, Parser, Query};

pub struct TagConfiguration {
    pub language: Language,
    pub query: Query,
    pub parser: Parser,
}

pub fn rust() -> TagConfiguration {
    let language = BundledParser::Rust.get_language();
    let query = include_scip_query!("rust", "scip-tags");

    let mut parser = Parser::new();
    parser.set_language(language).unwrap();

    TagConfiguration {
        language,
        parser,
        query: Query::new(language, query).unwrap(),
    }
}

pub fn go() -> TagConfiguration {
    let language = BundledParser::Go.get_language();
    let query = include_scip_query!("go", "scip-tags");

    let mut parser = Parser::new();
    parser.set_language(language).unwrap();

    TagConfiguration {
        language,
        parser,
        query: Query::new(language, query).unwrap(),
    }
}

pub struct LocalConfiguration {
    pub language: Language,
    pub query: Query,
    pub parser: Parser,
}

fn go_locals() -> LocalConfiguration {
    let language = BundledParser::Go.get_language();
    let query = include_scip_query!("go", "scip-locals");

    let mut parser = Parser::new();
    parser.set_language(language).unwrap();

    LocalConfiguration {
        language,
        parser,
        query: Query::new(language, query).unwrap(),
    }
}

fn perl_locals() -> LocalConfiguration {
    let language = BundledParser::Perl.get_language();
    let query = include_scip_query!("perl", "scip-locals");

    let mut parser = Parser::new();
    parser.set_language(language).unwrap();

    LocalConfiguration {
        language,
        parser,
        query: Query::new(language, query).unwrap(),
    }
}

pub fn get_local_configuration(parser: BundledParser) -> Option<LocalConfiguration> {
    match parser {
        BundledParser::Go => Some(go_locals()),
        BundledParser::Perl => Some(perl_locals()),
        _ => None,
    }
}
