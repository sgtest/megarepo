//! Conversions between [`SyntaxNode`] and [`tt::TokenTree`].

use rustc_hash::{FxHashMap, FxHashSet};
use stdx::{never, non_empty_vec::NonEmptyVec};
use syntax::{
    ast::{self, make::tokens::doc_comment},
    AstToken, Parse, PreorderWithTokens, SmolStr, SyntaxElement, SyntaxKind,
    SyntaxKind::*,
    SyntaxNode, SyntaxToken, SyntaxTreeBuilder, TextRange, TextSize, WalkEvent, T,
};
use tt::{
    buffer::{Cursor, TokenBuffer},
    Span, SpanData, SyntaxContext,
};

use crate::{to_parser_input::to_parser_input, tt_iter::TtIter, SpanMap};

#[cfg(test)]
mod tests;

pub trait SpanMapper<S: Span> {
    fn span_for(&self, range: TextRange) -> S;
}

impl<S: Span> SpanMapper<S> for SpanMap<S> {
    fn span_for(&self, range: TextRange) -> S {
        self.span_at(range.start())
    }
}

impl<S: Span, SM: SpanMapper<S>> SpanMapper<S> for &SM {
    fn span_for(&self, range: TextRange) -> S {
        SM::span_for(self, range)
    }
}

/// Dummy things for testing where spans don't matter.
pub(crate) mod dummy_test_span_utils {
    use super::*;

    pub type DummyTestSpanData = tt::SpanData<DummyTestSpanAnchor, DummyTestSyntaxContext>;
    pub const DUMMY: DummyTestSpanData = DummyTestSpanData::DUMMY;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct DummyTestSpanAnchor;
    impl tt::SpanAnchor for DummyTestSpanAnchor {
        const DUMMY: Self = DummyTestSpanAnchor;
    }
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct DummyTestSyntaxContext;
    impl SyntaxContext for DummyTestSyntaxContext {
        const DUMMY: Self = DummyTestSyntaxContext;
    }

    pub struct DummyTestSpanMap;

    impl SpanMapper<tt::SpanData<DummyTestSpanAnchor, DummyTestSyntaxContext>> for DummyTestSpanMap {
        fn span_for(
            &self,
            range: syntax::TextRange,
        ) -> tt::SpanData<DummyTestSpanAnchor, DummyTestSyntaxContext> {
            tt::SpanData { range, anchor: DummyTestSpanAnchor, ctx: DummyTestSyntaxContext }
        }
    }
}

/// Converts a syntax tree to a [`tt::Subtree`] using the provided span map to populate the
/// subtree's spans.
pub fn syntax_node_to_token_tree<Anchor, Ctx, SpanMap>(
    node: &SyntaxNode,
    map: SpanMap,
) -> tt::Subtree<SpanData<Anchor, Ctx>>
where
    SpanData<Anchor, Ctx>: Span,
    Anchor: Copy,
    Ctx: SyntaxContext,
    SpanMap: SpanMapper<SpanData<Anchor, Ctx>>,
{
    let mut c = Converter::new(node, map, Default::default(), Default::default());
    convert_tokens(&mut c)
}

/// Converts a syntax tree to a [`tt::Subtree`] using the provided span map to populate the
/// subtree's spans. Additionally using the append and remove parameters, the additional tokens can
/// be injected or hidden from the output.
pub fn syntax_node_to_token_tree_modified<Anchor, Ctx, SpanMap>(
    node: &SyntaxNode,
    map: SpanMap,
    append: FxHashMap<SyntaxElement, Vec<tt::Leaf<SpanData<Anchor, Ctx>>>>,
    remove: FxHashSet<SyntaxNode>,
) -> tt::Subtree<SpanData<Anchor, Ctx>>
where
    SpanMap: SpanMapper<SpanData<Anchor, Ctx>>,
    SpanData<Anchor, Ctx>: Span,
    Anchor: Copy,
    Ctx: SyntaxContext,
{
    let mut c = Converter::new(node, map, append, remove);
    convert_tokens(&mut c)
}

// The following items are what `rustc` macro can be parsed into :
// link: https://github.com/rust-lang/rust/blob/9ebf47851a357faa4cd97f4b1dc7835f6376e639/src/libsyntax/ext/expand.rs#L141
// * Expr(P<ast::Expr>)                     -> token_tree_to_expr
// * Pat(P<ast::Pat>)                       -> token_tree_to_pat
// * Ty(P<ast::Ty>)                         -> token_tree_to_ty
// * Stmts(SmallVec<[ast::Stmt; 1]>)        -> token_tree_to_stmts
// * Items(SmallVec<[P<ast::Item>; 1]>)     -> token_tree_to_items
//
// * TraitItems(SmallVec<[ast::TraitItem; 1]>)
// * AssocItems(SmallVec<[ast::AssocItem; 1]>)
// * ForeignItems(SmallVec<[ast::ForeignItem; 1]>

/// Converts a [`tt::Subtree`] back to a [`SyntaxNode`].
/// The produced `SpanMap` contains a mapping from the syntax nodes offsets to the subtree's spans.
pub fn token_tree_to_syntax_node<Anchor, Ctx>(
    tt: &tt::Subtree<SpanData<Anchor, Ctx>>,
    entry_point: parser::TopEntryPoint,
) -> (Parse<SyntaxNode>, SpanMap<SpanData<Anchor, Ctx>>)
where
    SpanData<Anchor, Ctx>: Span,
    Anchor: Copy,
    Ctx: SyntaxContext,
{
    let buffer = match tt {
        tt::Subtree {
            delimiter: tt::Delimiter { kind: tt::DelimiterKind::Invisible, .. },
            token_trees,
        } => TokenBuffer::from_tokens(token_trees.as_slice()),
        _ => TokenBuffer::from_subtree(tt),
    };
    let parser_input = to_parser_input(&buffer);
    let parser_output = entry_point.parse(&parser_input);
    let mut tree_sink = TtTreeSink::new(buffer.begin());
    for event in parser_output.iter() {
        match event {
            parser::Step::Token { kind, n_input_tokens: n_raw_tokens } => {
                tree_sink.token(kind, n_raw_tokens)
            }
            parser::Step::FloatSplit { ends_in_dot: has_pseudo_dot } => {
                tree_sink.float_split(has_pseudo_dot)
            }
            parser::Step::Enter { kind } => tree_sink.start_node(kind),
            parser::Step::Exit => tree_sink.finish_node(),
            parser::Step::Error { msg } => tree_sink.error(msg.to_string()),
        }
    }
    tree_sink.finish()
}

/// Convert a string to a `TokenTree`. The spans of the subtree will be anchored to the provided
/// anchor with the given context.
pub fn parse_to_token_tree<Anchor, Ctx>(
    anchor: Anchor,
    ctx: Ctx,
    text: &str,
) -> Option<tt::Subtree<SpanData<Anchor, Ctx>>>
where
    SpanData<Anchor, Ctx>: Span,
    Anchor: Copy,
    Ctx: SyntaxContext,
{
    let lexed = parser::LexedStr::new(text);
    if lexed.errors().next().is_some() {
        return None;
    }
    let mut conv = RawConverter { lexed, pos: 0, anchor, ctx };
    Some(convert_tokens(&mut conv))
}

/// Convert a string to a `TokenTree`. The passed span will be used for all spans of the produced subtree.
pub fn parse_to_token_tree_static_span<S>(span: S, text: &str) -> Option<tt::Subtree<S>>
where
    S: Span,
{
    let lexed = parser::LexedStr::new(text);
    if lexed.errors().next().is_some() {
        return None;
    }
    let mut conv = StaticRawConverter { lexed, pos: 0, span };
    Some(convert_tokens(&mut conv))
}

/// Split token tree with separate expr: $($e:expr)SEP*
pub fn parse_exprs_with_sep<S: Span>(tt: &tt::Subtree<S>, sep: char) -> Vec<tt::Subtree<S>> {
    if tt.token_trees.is_empty() {
        return Vec::new();
    }

    let mut iter = TtIter::new(tt);
    let mut res = Vec::new();

    while iter.peek_n(0).is_some() {
        let expanded = iter.expect_fragment(parser::PrefixEntryPoint::Expr);

        res.push(match expanded.value {
            None => break,
            Some(tt) => tt.subtree_or_wrap(),
        });

        let mut fork = iter.clone();
        if fork.expect_char(sep).is_err() {
            break;
        }
        iter = fork;
    }

    if iter.peek_n(0).is_some() {
        res.push(tt::Subtree {
            delimiter: tt::Delimiter::DUMMY_INVISIBLE,
            token_trees: iter.cloned().collect(),
        });
    }

    res
}

fn convert_tokens<S, C>(conv: &mut C) -> tt::Subtree<S>
where
    C: TokenConverter<S>,
    S: Span,
{
    let entry = tt::Subtree { delimiter: tt::Delimiter::DUMMY_INVISIBLE, token_trees: vec![] };
    let mut stack = NonEmptyVec::new(entry);

    while let Some((token, abs_range)) = conv.bump() {
        let tt::Subtree { delimiter, token_trees: result } = stack.last_mut();

        let tt = match token.as_leaf() {
            Some(leaf) => tt::TokenTree::Leaf(leaf.clone()),
            None => match token.kind(conv) {
                // Desugar doc comments into doc attributes
                COMMENT => {
                    let span = conv.span_for(abs_range);
                    if let Some(tokens) = conv.convert_doc_comment(&token, span) {
                        result.extend(tokens);
                    }
                    continue;
                }
                kind if kind.is_punct() && kind != UNDERSCORE => {
                    let expected = match delimiter.kind {
                        tt::DelimiterKind::Parenthesis => Some(T![')']),
                        tt::DelimiterKind::Brace => Some(T!['}']),
                        tt::DelimiterKind::Bracket => Some(T![']']),
                        tt::DelimiterKind::Invisible => None,
                    };

                    // Current token is a closing delimiter that we expect, fix up the closing span
                    // and end the subtree here
                    if matches!(expected, Some(expected) if expected == kind) {
                        if let Some(mut subtree) = stack.pop() {
                            subtree.delimiter.close = conv.span_for(abs_range);
                            stack.last_mut().token_trees.push(subtree.into());
                        }
                        continue;
                    }

                    let delim = match kind {
                        T!['('] => Some(tt::DelimiterKind::Parenthesis),
                        T!['{'] => Some(tt::DelimiterKind::Brace),
                        T!['['] => Some(tt::DelimiterKind::Bracket),
                        _ => None,
                    };

                    // Start a new subtree
                    if let Some(kind) = delim {
                        let open = conv.span_for(abs_range);
                        stack.push(tt::Subtree {
                            delimiter: tt::Delimiter {
                                open,
                                // will be overwritten on subtree close above
                                close: open,
                                kind,
                            },
                            token_trees: vec![],
                        });
                        continue;
                    }

                    let spacing = match conv.peek().map(|next| next.kind(conv)) {
                        Some(kind) if is_single_token_op(kind) => tt::Spacing::Joint,
                        _ => tt::Spacing::Alone,
                    };
                    let Some(char) = token.to_char(conv) else {
                        panic!("Token from lexer must be single char: token = {token:#?}")
                    };
                    tt::Leaf::from(tt::Punct { char, spacing, span: conv.span_for(abs_range) })
                        .into()
                }
                kind => {
                    macro_rules! make_leaf {
                        ($i:ident) => {
                            tt::$i { span: conv.span_for(abs_range), text: token.to_text(conv) }
                                .into()
                        };
                    }
                    let leaf: tt::Leaf<_> = match kind {
                        T![true] | T![false] => make_leaf!(Ident),
                        IDENT => make_leaf!(Ident),
                        UNDERSCORE => make_leaf!(Ident),
                        k if k.is_keyword() => make_leaf!(Ident),
                        k if k.is_literal() => make_leaf!(Literal),
                        LIFETIME_IDENT => {
                            let apostrophe = tt::Leaf::from(tt::Punct {
                                char: '\'',
                                spacing: tt::Spacing::Joint,
                                span: conv
                                    .span_for(TextRange::at(abs_range.start(), TextSize::of('\''))),
                            });
                            result.push(apostrophe.into());

                            let ident = tt::Leaf::from(tt::Ident {
                                text: SmolStr::new(&token.to_text(conv)[1..]),
                                span: conv.span_for(TextRange::new(
                                    abs_range.start() + TextSize::of('\''),
                                    abs_range.end(),
                                )),
                            });
                            result.push(ident.into());
                            continue;
                        }
                        _ => continue,
                    };

                    leaf.into()
                }
            },
        };

        result.push(tt);
    }

    // If we get here, we've consumed all input tokens.
    // We might have more than one subtree in the stack, if the delimiters are improperly balanced.
    // Merge them so we're left with one.
    while let Some(entry) = stack.pop() {
        let parent = stack.last_mut();

        let leaf: tt::Leaf<_> = tt::Punct {
            span: entry.delimiter.open,
            char: match entry.delimiter.kind {
                tt::DelimiterKind::Parenthesis => '(',
                tt::DelimiterKind::Brace => '{',
                tt::DelimiterKind::Bracket => '[',
                tt::DelimiterKind::Invisible => '$',
            },
            spacing: tt::Spacing::Alone,
        }
        .into();
        parent.token_trees.push(leaf.into());
        parent.token_trees.extend(entry.token_trees);
    }

    let subtree = stack.into_last();
    if let [tt::TokenTree::Subtree(first)] = &*subtree.token_trees {
        first.clone()
    } else {
        subtree
    }
}

fn is_single_token_op(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        EQ | L_ANGLE
            | R_ANGLE
            | BANG
            | AMP
            | PIPE
            | TILDE
            | AT
            | DOT
            | COMMA
            | SEMICOLON
            | COLON
            | POUND
            | DOLLAR
            | QUESTION
            | PLUS
            | MINUS
            | STAR
            | SLASH
            | PERCENT
            | CARET
            // LIFETIME_IDENT will be split into a sequence of `'` (a single quote) and an
            // identifier.
            | LIFETIME_IDENT
    )
}

/// Returns the textual content of a doc comment block as a quoted string
/// That is, strips leading `///` (or `/**`, etc)
/// and strips the ending `*/`
/// And then quote the string, which is needed to convert to `tt::Literal`
fn doc_comment_text(comment: &ast::Comment) -> SmolStr {
    let prefix_len = comment.prefix().len();
    let mut text = &comment.text()[prefix_len..];

    // Remove ending "*/"
    if comment.kind().shape == ast::CommentShape::Block {
        text = &text[0..text.len() - 2];
    }

    // Quote the string
    // Note that `tt::Literal` expect an escaped string
    let text = format!("\"{}\"", text.escape_debug());
    text.into()
}

fn convert_doc_comment<S: Copy>(
    token: &syntax::SyntaxToken,
    span: S,
) -> Option<Vec<tt::TokenTree<S>>> {
    cov_mark::hit!(test_meta_doc_comments);
    let comment = ast::Comment::cast(token.clone())?;
    let doc = comment.kind().doc?;

    let mk_ident =
        |s: &str| tt::TokenTree::from(tt::Leaf::from(tt::Ident { text: s.into(), span }));

    let mk_punct = |c: char| {
        tt::TokenTree::from(tt::Leaf::from(tt::Punct {
            char: c,
            spacing: tt::Spacing::Alone,
            span,
        }))
    };

    let mk_doc_literal = |comment: &ast::Comment| {
        let lit = tt::Literal { text: doc_comment_text(comment), span };

        tt::TokenTree::from(tt::Leaf::from(lit))
    };

    // Make `doc="\" Comments\""
    let meta_tkns = vec![mk_ident("doc"), mk_punct('='), mk_doc_literal(&comment)];

    // Make `#![]`
    let mut token_trees = Vec::with_capacity(3);
    token_trees.push(mk_punct('#'));
    if let ast::CommentPlacement::Inner = doc {
        token_trees.push(mk_punct('!'));
    }
    token_trees.push(tt::TokenTree::from(tt::Subtree {
        delimiter: tt::Delimiter { open: span, close: span, kind: tt::DelimiterKind::Bracket },
        token_trees: meta_tkns,
    }));

    Some(token_trees)
}

/// A raw token (straight from lexer) converter
struct RawConverter<'a, Anchor, Ctx> {
    lexed: parser::LexedStr<'a>,
    pos: usize,
    anchor: Anchor,
    ctx: Ctx,
}
/// A raw token (straight from lexer) converter that gives every token the same span.
struct StaticRawConverter<'a, S> {
    lexed: parser::LexedStr<'a>,
    pos: usize,
    span: S,
}

trait SrcToken<Ctx, S>: std::fmt::Debug {
    fn kind(&self, ctx: &Ctx) -> SyntaxKind;

    fn to_char(&self, ctx: &Ctx) -> Option<char>;

    fn to_text(&self, ctx: &Ctx) -> SmolStr;

    fn as_leaf(&self) -> Option<&tt::Leaf<S>> {
        None
    }
}

trait TokenConverter<S>: Sized {
    type Token: SrcToken<Self, S>;

    fn convert_doc_comment(&self, token: &Self::Token, span: S) -> Option<Vec<tt::TokenTree<S>>>;

    fn bump(&mut self) -> Option<(Self::Token, TextRange)>;

    fn peek(&self) -> Option<Self::Token>;

    fn span_for(&self, range: TextRange) -> S;
}

impl<Anchor, S, Ctx> SrcToken<RawConverter<'_, Anchor, Ctx>, S> for usize {
    fn kind(&self, ctx: &RawConverter<'_, Anchor, Ctx>) -> SyntaxKind {
        ctx.lexed.kind(*self)
    }

    fn to_char(&self, ctx: &RawConverter<'_, Anchor, Ctx>) -> Option<char> {
        ctx.lexed.text(*self).chars().next()
    }

    fn to_text(&self, ctx: &RawConverter<'_, Anchor, Ctx>) -> SmolStr {
        ctx.lexed.text(*self).into()
    }
}

impl<S: Span> SrcToken<StaticRawConverter<'_, S>, S> for usize {
    fn kind(&self, ctx: &StaticRawConverter<'_, S>) -> SyntaxKind {
        ctx.lexed.kind(*self)
    }

    fn to_char(&self, ctx: &StaticRawConverter<'_, S>) -> Option<char> {
        ctx.lexed.text(*self).chars().next()
    }

    fn to_text(&self, ctx: &StaticRawConverter<'_, S>) -> SmolStr {
        ctx.lexed.text(*self).into()
    }
}

impl<Anchor: Copy, Ctx: SyntaxContext> TokenConverter<SpanData<Anchor, Ctx>>
    for RawConverter<'_, Anchor, Ctx>
where
    SpanData<Anchor, Ctx>: Span,
{
    type Token = usize;

    fn convert_doc_comment(
        &self,
        &token: &usize,
        span: SpanData<Anchor, Ctx>,
    ) -> Option<Vec<tt::TokenTree<SpanData<Anchor, Ctx>>>> {
        let text = self.lexed.text(token);
        convert_doc_comment(&doc_comment(text), span)
    }

    fn bump(&mut self) -> Option<(Self::Token, TextRange)> {
        if self.pos == self.lexed.len() {
            return None;
        }
        let token = self.pos;
        self.pos += 1;
        let range = self.lexed.text_range(token);
        let range = TextRange::new(range.start.try_into().ok()?, range.end.try_into().ok()?);

        Some((token, range))
    }

    fn peek(&self) -> Option<Self::Token> {
        if self.pos == self.lexed.len() {
            return None;
        }
        Some(self.pos)
    }

    fn span_for(&self, range: TextRange) -> SpanData<Anchor, Ctx> {
        SpanData { range, anchor: self.anchor, ctx: self.ctx }
    }
}

impl<S> TokenConverter<S> for StaticRawConverter<'_, S>
where
    S: Span,
{
    type Token = usize;

    fn convert_doc_comment(&self, &token: &usize, span: S) -> Option<Vec<tt::TokenTree<S>>> {
        let text = self.lexed.text(token);
        convert_doc_comment(&doc_comment(text), span)
    }

    fn bump(&mut self) -> Option<(Self::Token, TextRange)> {
        if self.pos == self.lexed.len() {
            return None;
        }
        let token = self.pos;
        self.pos += 1;
        let range = self.lexed.text_range(token);
        let range = TextRange::new(range.start.try_into().ok()?, range.end.try_into().ok()?);

        Some((token, range))
    }

    fn peek(&self) -> Option<Self::Token> {
        if self.pos == self.lexed.len() {
            return None;
        }
        Some(self.pos)
    }

    fn span_for(&self, _: TextRange) -> S {
        self.span
    }
}

struct Converter<SpanMap, S> {
    current: Option<SyntaxToken>,
    current_leafs: Vec<tt::Leaf<S>>,
    preorder: PreorderWithTokens,
    range: TextRange,
    punct_offset: Option<(SyntaxToken, TextSize)>,
    /// Used to make the emitted text ranges in the spans relative to the span anchor.
    map: SpanMap,
    append: FxHashMap<SyntaxElement, Vec<tt::Leaf<S>>>,
    remove: FxHashSet<SyntaxNode>,
}

impl<SpanMap, S> Converter<SpanMap, S> {
    fn new(
        node: &SyntaxNode,
        map: SpanMap,
        append: FxHashMap<SyntaxElement, Vec<tt::Leaf<S>>>,
        remove: FxHashSet<SyntaxNode>,
    ) -> Self {
        let mut this = Converter {
            current: None,
            preorder: node.preorder_with_tokens(),
            range: node.text_range(),
            punct_offset: None,
            map,
            append,
            remove,
            current_leafs: vec![],
        };
        let first = this.next_token();
        this.current = first;
        this
    }

    fn next_token(&mut self) -> Option<SyntaxToken> {
        while let Some(ev) = self.preorder.next() {
            match ev {
                WalkEvent::Enter(SyntaxElement::Token(t)) => return Some(t),
                WalkEvent::Enter(SyntaxElement::Node(n)) if self.remove.contains(&n) => {
                    self.preorder.skip_subtree();
                    if let Some(mut v) = self.append.remove(&n.into()) {
                        v.reverse();
                        self.current_leafs.extend(v);
                        return None;
                    }
                }
                WalkEvent::Enter(SyntaxElement::Node(_)) => (),
                WalkEvent::Leave(ele) => {
                    if let Some(mut v) = self.append.remove(&ele) {
                        v.reverse();
                        self.current_leafs.extend(v);
                        return None;
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug)]
enum SynToken<S> {
    Ordinary(SyntaxToken),
    Punct { token: SyntaxToken, offset: usize },
    Leaf(tt::Leaf<S>),
}

impl<S> SynToken<S> {
    fn token(&self) -> &SyntaxToken {
        match self {
            SynToken::Ordinary(it) | SynToken::Punct { token: it, offset: _ } => it,
            SynToken::Leaf(_) => unreachable!(),
        }
    }
}

impl<SpanMap, S: std::fmt::Debug> SrcToken<Converter<SpanMap, S>, S> for SynToken<S> {
    fn kind(&self, ctx: &Converter<SpanMap, S>) -> SyntaxKind {
        match self {
            SynToken::Ordinary(token) => token.kind(),
            SynToken::Punct { .. } => SyntaxKind::from_char(self.to_char(ctx).unwrap()).unwrap(),
            SynToken::Leaf(_) => {
                never!();
                SyntaxKind::ERROR
            }
        }
    }
    fn to_char(&self, _ctx: &Converter<SpanMap, S>) -> Option<char> {
        match self {
            SynToken::Ordinary(_) => None,
            SynToken::Punct { token: it, offset: i } => it.text().chars().nth(*i),
            SynToken::Leaf(_) => None,
        }
    }
    fn to_text(&self, _ctx: &Converter<SpanMap, S>) -> SmolStr {
        match self {
            SynToken::Ordinary(token) | SynToken::Punct { token, offset: _ } => token.text().into(),
            SynToken::Leaf(_) => {
                never!();
                "".into()
            }
        }
    }
    fn as_leaf(&self) -> Option<&tt::Leaf<S>> {
        match self {
            SynToken::Ordinary(_) | SynToken::Punct { .. } => None,
            SynToken::Leaf(it) => Some(it),
        }
    }
}

impl<S, SpanMap> TokenConverter<S> for Converter<SpanMap, S>
where
    S: Span,
    SpanMap: SpanMapper<S>,
{
    type Token = SynToken<S>;
    fn convert_doc_comment(&self, token: &Self::Token, span: S) -> Option<Vec<tt::TokenTree<S>>> {
        convert_doc_comment(token.token(), span)
    }

    fn bump(&mut self) -> Option<(Self::Token, TextRange)> {
        if let Some((punct, offset)) = self.punct_offset.clone() {
            if usize::from(offset) + 1 < punct.text().len() {
                let offset = offset + TextSize::of('.');
                let range = punct.text_range();
                self.punct_offset = Some((punct.clone(), offset));
                let range = TextRange::at(range.start() + offset, TextSize::of('.'));
                return Some((
                    SynToken::Punct { token: punct, offset: u32::from(offset) as usize },
                    range,
                ));
            }
        }

        if let Some(leaf) = self.current_leafs.pop() {
            if self.current_leafs.is_empty() {
                self.current = self.next_token();
            }
            return Some((SynToken::Leaf(leaf), TextRange::empty(TextSize::new(0))));
        }

        let curr = self.current.clone()?;
        if !self.range.contains_range(curr.text_range()) {
            return None;
        }

        self.current = self.next_token();
        let token = if curr.kind().is_punct() {
            self.punct_offset = Some((curr.clone(), 0.into()));
            let range = curr.text_range();
            let range = TextRange::at(range.start(), TextSize::of('.'));
            (SynToken::Punct { token: curr, offset: 0 as usize }, range)
        } else {
            self.punct_offset = None;
            let range = curr.text_range();
            (SynToken::Ordinary(curr), range)
        };

        Some(token)
    }

    fn peek(&self) -> Option<Self::Token> {
        if let Some((punct, mut offset)) = self.punct_offset.clone() {
            offset += TextSize::of('.');
            if usize::from(offset) < punct.text().len() {
                return Some(SynToken::Punct { token: punct, offset: usize::from(offset) });
            }
        }

        let curr = self.current.clone()?;
        if !self.range.contains_range(curr.text_range()) {
            return None;
        }

        let token = if curr.kind().is_punct() {
            SynToken::Punct { token: curr, offset: 0 as usize }
        } else {
            SynToken::Ordinary(curr)
        };
        Some(token)
    }

    fn span_for(&self, range: TextRange) -> S {
        self.map.span_for(range)
    }
}

struct TtTreeSink<'a, Anchor, Ctx>
where
    SpanData<Anchor, Ctx>: Span,
{
    buf: String,
    cursor: Cursor<'a, SpanData<Anchor, Ctx>>,
    text_pos: TextSize,
    inner: SyntaxTreeBuilder,
    token_map: SpanMap<SpanData<Anchor, Ctx>>,
}

impl<'a, Anchor, Ctx> TtTreeSink<'a, Anchor, Ctx>
where
    SpanData<Anchor, Ctx>: Span,
{
    fn new(cursor: Cursor<'a, SpanData<Anchor, Ctx>>) -> Self {
        TtTreeSink {
            buf: String::new(),
            cursor,
            text_pos: 0.into(),
            inner: SyntaxTreeBuilder::default(),
            token_map: SpanMap::empty(),
        }
    }

    fn finish(mut self) -> (Parse<SyntaxNode>, SpanMap<SpanData<Anchor, Ctx>>) {
        self.token_map.finish();
        (self.inner.finish(), self.token_map)
    }
}

fn delim_to_str(d: tt::DelimiterKind, closing: bool) -> Option<&'static str> {
    let texts = match d {
        tt::DelimiterKind::Parenthesis => "()",
        tt::DelimiterKind::Brace => "{}",
        tt::DelimiterKind::Bracket => "[]",
        tt::DelimiterKind::Invisible => return None,
    };

    let idx = closing as usize;
    Some(&texts[idx..texts.len() - (1 - idx)])
}

impl<Anchor, Ctx> TtTreeSink<'_, Anchor, Ctx>
where
    SpanData<Anchor, Ctx>: Span,
{
    /// Parses a float literal as if it was a one to two name ref nodes with a dot inbetween.
    /// This occurs when a float literal is used as a field access.
    fn float_split(&mut self, has_pseudo_dot: bool) {
        let (text, span) = match self.cursor.token_tree() {
            Some(tt::buffer::TokenTreeRef::Leaf(tt::Leaf::Literal(lit), _)) => {
                (lit.text.as_str(), lit.span)
            }
            _ => unreachable!(),
        };
        // FIXME: Span splitting
        match text.split_once('.') {
            Some((left, right)) => {
                assert!(!left.is_empty());

                self.inner.start_node(SyntaxKind::NAME_REF);
                self.inner.token(SyntaxKind::INT_NUMBER, left);
                self.inner.finish_node();
                self.token_map.push(self.text_pos + TextSize::of(left), span);

                // here we move the exit up, the original exit has been deleted in process
                self.inner.finish_node();

                self.inner.token(SyntaxKind::DOT, ".");
                self.token_map.push(self.text_pos + TextSize::of(left) + TextSize::of("."), span);

                if has_pseudo_dot {
                    assert!(right.is_empty(), "{left}.{right}");
                } else {
                    assert!(!right.is_empty(), "{left}.{right}");
                    self.inner.start_node(SyntaxKind::NAME_REF);
                    self.inner.token(SyntaxKind::INT_NUMBER, right);
                    self.token_map.push(self.text_pos + TextSize::of(text), span);
                    self.inner.finish_node();

                    // the parser creates an unbalanced start node, we are required to close it here
                    self.inner.finish_node();
                }
                self.text_pos += TextSize::of(text);
            }
            None => unreachable!(),
        }
        self.cursor = self.cursor.bump();
    }

    fn token(&mut self, kind: SyntaxKind, mut n_tokens: u8) {
        if kind == LIFETIME_IDENT {
            n_tokens = 2;
        }

        let mut last = self.cursor;
        for _ in 0..n_tokens {
            let tmp: u8;
            if self.cursor.eof() {
                break;
            }
            last = self.cursor;
            let (text, span) = loop {
                break match self.cursor.token_tree() {
                    Some(tt::buffer::TokenTreeRef::Leaf(leaf, _)) => {
                        // Mark the range if needed
                        let (text, span) = match leaf {
                            tt::Leaf::Ident(ident) => (ident.text.as_str(), ident.span),
                            tt::Leaf::Punct(punct) => {
                                assert!(punct.char.is_ascii());
                                tmp = punct.char as u8;
                                (
                                    std::str::from_utf8(std::slice::from_ref(&tmp)).unwrap(),
                                    punct.span,
                                )
                            }
                            tt::Leaf::Literal(lit) => (lit.text.as_str(), lit.span),
                        };
                        self.cursor = self.cursor.bump();
                        (text, span)
                    }
                    Some(tt::buffer::TokenTreeRef::Subtree(subtree, _)) => {
                        self.cursor = self.cursor.subtree().unwrap();
                        match delim_to_str(subtree.delimiter.kind, false) {
                            Some(it) => (it, subtree.delimiter.open),
                            None => continue,
                        }
                    }
                    None => {
                        let parent = self.cursor.end().unwrap();
                        self.cursor = self.cursor.bump();
                        match delim_to_str(parent.delimiter.kind, true) {
                            Some(it) => (it, parent.delimiter.close),
                            None => continue,
                        }
                    }
                };
            };
            self.buf += text;
            self.text_pos += TextSize::of(text);
            self.token_map.push(self.text_pos, span);
        }

        self.inner.token(kind, self.buf.as_str());
        self.buf.clear();
        // FIXME: Emitting whitespace for this is really just a hack, we should get rid of it.
        // Add whitespace between adjoint puncts
        let next = last.bump();
        if let (
            Some(tt::buffer::TokenTreeRef::Leaf(tt::Leaf::Punct(curr), _)),
            Some(tt::buffer::TokenTreeRef::Leaf(tt::Leaf::Punct(next), _)),
        ) = (last.token_tree(), next.token_tree())
        {
            // Note: We always assume the semi-colon would be the last token in
            // other parts of RA such that we don't add whitespace here.
            //
            // When `next` is a `Punct` of `'`, that's a part of a lifetime identifier so we don't
            // need to add whitespace either.
            if curr.spacing == tt::Spacing::Alone && curr.char != ';' && next.char != '\'' {
                self.inner.token(WHITESPACE, " ");
                self.text_pos += TextSize::of(' ');
                self.token_map.push(self.text_pos, curr.span);
            }
        }
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.inner.start_node(kind);
    }

    fn finish_node(&mut self) {
        self.inner.finish_node();
    }

    fn error(&mut self, error: String) {
        self.inner.error(error, self.text_pos)
    }
}
