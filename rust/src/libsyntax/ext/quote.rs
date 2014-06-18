// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast;
use codemap::Span;
use ext::base::ExtCtxt;
use ext::base;
use ext::build::AstBuilder;
use parse::token::*;
use parse::token;
use parse;

use std::gc::Gc;

/**
*
* Quasiquoting works via token trees.
*
* This is registered as a set of expression syntax extension called quote!
* that lifts its argument token-tree to an AST representing the
* construction of the same token tree, with ast::TTNonterminal nodes
* interpreted as antiquotes (splices).
*
*/

pub mod rt {
    use ast;
    use ext::base::ExtCtxt;
    use parse::token;
    use parse;
    use print::pprust;

    use ast::{TokenTree, Generics, Expr};

    pub use parse::new_parser_from_tts;
    pub use codemap::{BytePos, Span, dummy_spanned};

    use std::gc::Gc;

    pub trait ToTokens {
        fn to_tokens(&self, _cx: &ExtCtxt) -> Vec<TokenTree> ;
    }

    impl ToTokens for Vec<TokenTree> {
        fn to_tokens(&self, _cx: &ExtCtxt) -> Vec<TokenTree> {
            (*self).clone()
        }
    }

    /* Should be (when bugs in default methods are fixed):

    trait ToSource : ToTokens {
        // Takes a thing and generates a string containing rust code for it.
        pub fn to_source() -> String;

        // If you can make source, you can definitely make tokens.
        pub fn to_tokens(cx: &ExtCtxt) -> ~[TokenTree] {
            cx.parse_tts(self.to_source())
        }
    }

    */

    pub trait ToSource {
        // Takes a thing and generates a string containing rust code for it.
        fn to_source(&self) -> String;
    }

    impl ToSource for ast::Ident {
        fn to_source(&self) -> String {
            token::get_ident(*self).get().to_string()
        }
    }

    impl ToSource for Gc<ast::Item> {
        fn to_source(&self) -> String {
            pprust::item_to_str(&**self)
        }
    }

    impl<'a> ToSource for &'a [Gc<ast::Item>] {
        fn to_source(&self) -> String {
            self.iter()
                .map(|i| i.to_source())
                .collect::<Vec<String>>()
                .connect("\n\n")
                .to_string()
        }
    }

    impl ToSource for ast::Ty {
        fn to_source(&self) -> String {
            pprust::ty_to_str(self)
        }
    }

    impl<'a> ToSource for &'a [ast::Ty] {
        fn to_source(&self) -> String {
            self.iter()
                .map(|i| i.to_source())
                .collect::<Vec<String>>()
                .connect(", ")
                .to_string()
        }
    }

    impl ToSource for Generics {
        fn to_source(&self) -> String {
            pprust::generics_to_str(self)
        }
    }

    impl ToSource for Gc<ast::Expr> {
        fn to_source(&self) -> String {
            pprust::expr_to_str(&**self)
        }
    }

    impl ToSource for ast::Block {
        fn to_source(&self) -> String {
            pprust::block_to_str(self)
        }
    }

    impl ToSource for ast::Arg {
        fn to_source(&self) -> String {
            pprust::arg_to_str(self)
        }
    }

    impl<'a> ToSource for &'a str {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitStr(
                    token::intern_and_get_ident(*self), ast::CookedStr));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for () {
        fn to_source(&self) -> String {
            "()".to_string()
        }
    }

    impl ToSource for bool {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitBool(*self));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for char {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitChar(*self));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for int {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitInt(*self as i64, ast::TyI));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for i8 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitInt(*self as i64, ast::TyI8));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for i16 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitInt(*self as i64, ast::TyI16));
            pprust::lit_to_str(&lit)
        }
    }


    impl ToSource for i32 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitInt(*self as i64, ast::TyI32));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for i64 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitInt(*self as i64, ast::TyI64));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for uint {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitUint(*self as u64, ast::TyU));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for u8 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitUint(*self as u64, ast::TyU8));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for u16 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitUint(*self as u64, ast::TyU16));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for u32 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitUint(*self as u64, ast::TyU32));
            pprust::lit_to_str(&lit)
        }
    }

    impl ToSource for u64 {
        fn to_source(&self) -> String {
            let lit = dummy_spanned(ast::LitUint(*self as u64, ast::TyU64));
            pprust::lit_to_str(&lit)
        }
    }

    // Alas ... we write these out instead. All redundant.

    macro_rules! impl_to_tokens(
        ($t:ty) => (
            impl ToTokens for $t {
                fn to_tokens(&self, cx: &ExtCtxt) -> Vec<TokenTree> {
                    cx.parse_tts(self.to_source())
                }
            }
        )
    )

    macro_rules! impl_to_tokens_self(
        ($t:ty) => (
            impl<'a> ToTokens for $t {
                fn to_tokens(&self, cx: &ExtCtxt) -> Vec<TokenTree> {
                    cx.parse_tts(self.to_source())
                }
            }
        )
    )

    impl_to_tokens!(ast::Ident)
    impl_to_tokens!(Gc<ast::Item>)
    impl_to_tokens_self!(&'a [Gc<ast::Item>])
    impl_to_tokens!(ast::Ty)
    impl_to_tokens_self!(&'a [ast::Ty])
    impl_to_tokens!(Generics)
    impl_to_tokens!(Gc<ast::Expr>)
    impl_to_tokens!(ast::Block)
    impl_to_tokens!(ast::Arg)
    impl_to_tokens_self!(&'a str)
    impl_to_tokens!(())
    impl_to_tokens!(char)
    impl_to_tokens!(bool)
    impl_to_tokens!(int)
    impl_to_tokens!(i8)
    impl_to_tokens!(i16)
    impl_to_tokens!(i32)
    impl_to_tokens!(i64)
    impl_to_tokens!(uint)
    impl_to_tokens!(u8)
    impl_to_tokens!(u16)
    impl_to_tokens!(u32)
    impl_to_tokens!(u64)

    pub trait ExtParseUtils {
        fn parse_item(&self, s: String) -> Gc<ast::Item>;
        fn parse_expr(&self, s: String) -> Gc<ast::Expr>;
        fn parse_stmt(&self, s: String) -> Gc<ast::Stmt>;
        fn parse_tts(&self, s: String) -> Vec<ast::TokenTree> ;
    }

    impl<'a> ExtParseUtils for ExtCtxt<'a> {

        fn parse_item(&self, s: String) -> Gc<ast::Item> {
            let res = parse::parse_item_from_source_str(
                "<quote expansion>".to_string(),
                s,
                self.cfg(),
                self.parse_sess());
            match res {
                Some(ast) => ast,
                None => {
                    error!("parse error");
                    fail!()
                }
            }
        }

        fn parse_stmt(&self, s: String) -> Gc<ast::Stmt> {
            parse::parse_stmt_from_source_str("<quote expansion>".to_string(),
                                              s,
                                              self.cfg(),
                                              Vec::new(),
                                              self.parse_sess())
        }

        fn parse_expr(&self, s: String) -> Gc<ast::Expr> {
            parse::parse_expr_from_source_str("<quote expansion>".to_string(),
                                              s,
                                              self.cfg(),
                                              self.parse_sess())
        }

        fn parse_tts(&self, s: String) -> Vec<ast::TokenTree> {
            parse::parse_tts_from_source_str("<quote expansion>".to_string(),
                                             s,
                                             self.cfg(),
                                             self.parse_sess())
        }
    }

}

pub fn expand_quote_tokens(cx: &mut ExtCtxt,
                           sp: Span,
                           tts: &[ast::TokenTree])
                           -> Box<base::MacResult> {
    let (cx_expr, expr) = expand_tts(cx, sp, tts);
    let expanded = expand_wrapper(cx, sp, cx_expr, expr);
    base::MacExpr::new(expanded)
}

pub fn expand_quote_expr(cx: &mut ExtCtxt,
                         sp: Span,
                         tts: &[ast::TokenTree]) -> Box<base::MacResult> {
    let expanded = expand_parse_call(cx, sp, "parse_expr", Vec::new(), tts);
    base::MacExpr::new(expanded)
}

pub fn expand_quote_item(cx: &mut ExtCtxt,
                         sp: Span,
                         tts: &[ast::TokenTree])
                         -> Box<base::MacResult> {
    let expanded = expand_parse_call(cx, sp, "parse_item_with_outer_attributes",
                                    vec!(), tts);
    base::MacExpr::new(expanded)
}

pub fn expand_quote_pat(cx: &mut ExtCtxt,
                        sp: Span,
                        tts: &[ast::TokenTree])
                        -> Box<base::MacResult> {
    let expanded = expand_parse_call(cx, sp, "parse_pat", vec!(), tts);
    base::MacExpr::new(expanded)
}

pub fn expand_quote_ty(cx: &mut ExtCtxt,
                       sp: Span,
                       tts: &[ast::TokenTree])
                       -> Box<base::MacResult> {
    let e_param_colons = cx.expr_lit(sp, ast::LitBool(false));
    let expanded = expand_parse_call(cx, sp, "parse_ty",
                                     vec!(e_param_colons), tts);
    base::MacExpr::new(expanded)
}

pub fn expand_quote_stmt(cx: &mut ExtCtxt,
                         sp: Span,
                         tts: &[ast::TokenTree])
                         -> Box<base::MacResult> {
    let e_attrs = cx.expr_vec_ng(sp);
    let expanded = expand_parse_call(cx, sp, "parse_stmt",
                                    vec!(e_attrs), tts);
    base::MacExpr::new(expanded)
}

fn ids_ext(strs: Vec<String> ) -> Vec<ast::Ident> {
    strs.iter().map(|str| str_to_ident((*str).as_slice())).collect()
}

fn id_ext(str: &str) -> ast::Ident {
    str_to_ident(str)
}

// Lift an ident to the expr that evaluates to that ident.
fn mk_ident(cx: &ExtCtxt, sp: Span, ident: ast::Ident) -> Gc<ast::Expr> {
    let e_str = cx.expr_str(sp, token::get_ident(ident));
    cx.expr_method_call(sp,
                        cx.expr_ident(sp, id_ext("ext_cx")),
                        id_ext("ident_of"),
                        vec!(e_str))
}

fn mk_ast_path(cx: &ExtCtxt, sp: Span, name: &str) -> Gc<ast::Expr> {
    let idents = vec!(id_ext("syntax"), id_ext("ast"), id_ext(name));
    cx.expr_path(cx.path_global(sp, idents))
}

fn mk_token_path(cx: &ExtCtxt, sp: Span, name: &str) -> Gc<ast::Expr> {
    let idents = vec!(id_ext("syntax"), id_ext("parse"), id_ext("token"), id_ext(name));
    cx.expr_path(cx.path_global(sp, idents))
}

fn mk_binop(cx: &ExtCtxt, sp: Span, bop: token::BinOp) -> Gc<ast::Expr> {
    let name = match bop {
        PLUS => "PLUS",
        MINUS => "MINUS",
        STAR => "STAR",
        SLASH => "SLASH",
        PERCENT => "PERCENT",
        CARET => "CARET",
        AND => "AND",
        OR => "OR",
        SHL => "SHL",
        SHR => "SHR"
    };
    mk_token_path(cx, sp, name)
}

fn mk_token(cx: &ExtCtxt, sp: Span, tok: &token::Token) -> Gc<ast::Expr> {

    match *tok {
        BINOP(binop) => {
            return cx.expr_call(sp, mk_token_path(cx, sp, "BINOP"), vec!(mk_binop(cx, sp, binop)));
        }
        BINOPEQ(binop) => {
            return cx.expr_call(sp, mk_token_path(cx, sp, "BINOPEQ"),
                                vec!(mk_binop(cx, sp, binop)));
        }

        LIT_BYTE(i) => {
            let e_byte = cx.expr_lit(sp, ast::LitByte(i));

            return cx.expr_call(sp, mk_token_path(cx, sp, "LIT_BYTE"), vec!(e_byte));
        }

        LIT_CHAR(i) => {
            let e_char = cx.expr_lit(sp, ast::LitChar(i));

            return cx.expr_call(sp, mk_token_path(cx, sp, "LIT_CHAR"), vec!(e_char));
        }

        LIT_INT(i, ity) => {
            let s_ity = match ity {
                ast::TyI => "TyI",
                ast::TyI8 => "TyI8",
                ast::TyI16 => "TyI16",
                ast::TyI32 => "TyI32",
                ast::TyI64 => "TyI64"
            };
            let e_ity = mk_ast_path(cx, sp, s_ity);
            let e_i64 = cx.expr_lit(sp, ast::LitInt(i, ast::TyI64));
            return cx.expr_call(sp, mk_token_path(cx, sp, "LIT_INT"), vec!(e_i64, e_ity));
        }

        LIT_UINT(u, uty) => {
            let s_uty = match uty {
                ast::TyU => "TyU",
                ast::TyU8 => "TyU8",
                ast::TyU16 => "TyU16",
                ast::TyU32 => "TyU32",
                ast::TyU64 => "TyU64"
            };
            let e_uty = mk_ast_path(cx, sp, s_uty);
            let e_u64 = cx.expr_lit(sp, ast::LitUint(u, ast::TyU64));
            return cx.expr_call(sp, mk_token_path(cx, sp, "LIT_UINT"), vec!(e_u64, e_uty));
        }

        LIT_INT_UNSUFFIXED(i) => {
            let e_i64 = cx.expr_lit(sp, ast::LitInt(i, ast::TyI64));
            return cx.expr_call(sp, mk_token_path(cx, sp, "LIT_INT_UNSUFFIXED"), vec!(e_i64));
        }

        LIT_FLOAT(fident, fty) => {
            let s_fty = match fty {
                ast::TyF32 => "TyF32",
                ast::TyF64 => "TyF64",
                ast::TyF128 => "TyF128"
            };
            let e_fty = mk_ast_path(cx, sp, s_fty);
            let e_fident = mk_ident(cx, sp, fident);
            return cx.expr_call(sp, mk_token_path(cx, sp, "LIT_FLOAT"), vec!(e_fident, e_fty));
        }

        LIT_STR(ident) => {
            return cx.expr_call(sp,
                                mk_token_path(cx, sp, "LIT_STR"),
                                vec!(mk_ident(cx, sp, ident)));
        }

        LIT_STR_RAW(ident, n) => {
            return cx.expr_call(sp,
                                mk_token_path(cx, sp, "LIT_STR_RAW"),
                                vec!(mk_ident(cx, sp, ident), cx.expr_uint(sp, n)));
        }

        IDENT(ident, b) => {
            return cx.expr_call(sp,
                                mk_token_path(cx, sp, "IDENT"),
                                vec!(mk_ident(cx, sp, ident), cx.expr_bool(sp, b)));
        }

        LIFETIME(ident) => {
            return cx.expr_call(sp,
                                mk_token_path(cx, sp, "LIFETIME"),
                                vec!(mk_ident(cx, sp, ident)));
        }

        DOC_COMMENT(ident) => {
            return cx.expr_call(sp,
                                mk_token_path(cx, sp, "DOC_COMMENT"),
                                vec!(mk_ident(cx, sp, ident)));
        }

        INTERPOLATED(_) => fail!("quote! with interpolated token"),

        _ => ()
    }

    let name = match *tok {
        EQ => "EQ",
        LT => "LT",
        LE => "LE",
        EQEQ => "EQEQ",
        NE => "NE",
        GE => "GE",
        GT => "GT",
        ANDAND => "ANDAND",
        OROR => "OROR",
        NOT => "NOT",
        TILDE => "TILDE",
        AT => "AT",
        DOT => "DOT",
        DOTDOT => "DOTDOT",
        COMMA => "COMMA",
        SEMI => "SEMI",
        COLON => "COLON",
        MOD_SEP => "MOD_SEP",
        RARROW => "RARROW",
        LARROW => "LARROW",
        FAT_ARROW => "FAT_ARROW",
        LPAREN => "LPAREN",
        RPAREN => "RPAREN",
        LBRACKET => "LBRACKET",
        RBRACKET => "RBRACKET",
        LBRACE => "LBRACE",
        RBRACE => "RBRACE",
        POUND => "POUND",
        DOLLAR => "DOLLAR",
        UNDERSCORE => "UNDERSCORE",
        EOF => "EOF",
        _ => fail!()
    };
    mk_token_path(cx, sp, name)
}


fn mk_tt(cx: &ExtCtxt, sp: Span, tt: &ast::TokenTree) -> Vec<Gc<ast::Stmt>> {
    match *tt {
        ast::TTTok(sp, ref tok) => {
            let e_sp = cx.expr_ident(sp, id_ext("_sp"));
            let e_tok = cx.expr_call(sp,
                                     mk_ast_path(cx, sp, "TTTok"),
                                     vec!(e_sp, mk_token(cx, sp, tok)));
            let e_push =
                cx.expr_method_call(sp,
                                    cx.expr_ident(sp, id_ext("tt")),
                                    id_ext("push"),
                                    vec!(e_tok));
            vec!(cx.stmt_expr(e_push))
        }

        ast::TTDelim(ref tts) => mk_tts(cx, sp, tts.as_slice()),
        ast::TTSeq(..) => fail!("TTSeq in quote!"),

        ast::TTNonterminal(sp, ident) => {

            // tt.push_all_move($ident.to_tokens(ext_cx))

            let e_to_toks =
                cx.expr_method_call(sp,
                                    cx.expr_ident(sp, ident),
                                    id_ext("to_tokens"),
                                    vec!(cx.expr_ident(sp, id_ext("ext_cx"))));

            let e_push =
                cx.expr_method_call(sp,
                                    cx.expr_ident(sp, id_ext("tt")),
                                    id_ext("push_all_move"),
                                    vec!(e_to_toks));

            vec!(cx.stmt_expr(e_push))
        }
    }
}

fn mk_tts(cx: &ExtCtxt, sp: Span, tts: &[ast::TokenTree])
    -> Vec<Gc<ast::Stmt>> {
    let mut ss = Vec::new();
    for tt in tts.iter() {
        ss.push_all_move(mk_tt(cx, sp, tt));
    }
    ss
}

fn expand_tts(cx: &ExtCtxt, sp: Span, tts: &[ast::TokenTree])
              -> (Gc<ast::Expr>, Gc<ast::Expr>) {
    // NB: It appears that the main parser loses its mind if we consider
    // $foo as a TTNonterminal during the main parse, so we have to re-parse
    // under quote_depth > 0. This is silly and should go away; the _guess_ is
    // it has to do with transition away from supporting old-style macros, so
    // try removing it when enough of them are gone.

    let mut p = parse::new_parser_from_tts(cx.parse_sess(),
                                           cx.cfg(),
                                           tts.iter()
                                              .map(|x| (*x).clone())
                                              .collect());
    p.quote_depth += 1u;

    let cx_expr = p.parse_expr();
    if !p.eat(&token::COMMA) {
        p.fatal("expected token `,`");
    }

    let tts = p.parse_all_token_trees();
    p.abort_if_errors();

    // We also bind a single value, sp, to ext_cx.call_site()
    //
    // This causes every span in a token-tree quote to be attributed to the
    // call site of the extension using the quote. We can't really do much
    // better since the source of the quote may well be in a library that
    // was not even parsed by this compilation run, that the user has no
    // source code for (eg. in libsyntax, which they're just _using_).
    //
    // The old quasiquoter had an elaborate mechanism for denoting input
    // file locations from which quotes originated; unfortunately this
    // relied on feeding the source string of the quote back into the
    // compiler (which we don't really want to do) and, in any case, only
    // pushed the problem a very small step further back: an error
    // resulting from a parse of the resulting quote is still attributed to
    // the site the string literal occurred, which was in a source file
    // _other_ than the one the user has control over. For example, an
    // error in a quote from the protocol compiler, invoked in user code
    // using macro_rules! for example, will be attributed to the macro_rules.rs
    // file in libsyntax, which the user might not even have source to (unless
    // they happen to have a compiler on hand). Over all, the phase distinction
    // just makes quotes "hard to attribute". Possibly this could be fixed
    // by recreating some of the original qq machinery in the tt regime
    // (pushing fake FileMaps onto the parser to account for original sites
    // of quotes, for example) but at this point it seems not likely to be
    // worth the hassle.

    let e_sp = cx.expr_method_call(sp,
                                   cx.expr_ident(sp, id_ext("ext_cx")),
                                   id_ext("call_site"),
                                   Vec::new());

    let stmt_let_sp = cx.stmt_let(sp, false,
                                  id_ext("_sp"),
                                  e_sp);

    let stmt_let_tt = cx.stmt_let(sp, true, id_ext("tt"), cx.expr_vec_ng(sp));

    let mut vector = vec!(stmt_let_sp, stmt_let_tt);
    vector.push_all_move(mk_tts(cx, sp, tts.as_slice()));
    let block = cx.expr_block(
        cx.block_all(sp,
                     Vec::new(),
                     vector,
                     Some(cx.expr_ident(sp, id_ext("tt")))));

    (cx_expr, block)
}

fn expand_wrapper(cx: &ExtCtxt,
                  sp: Span,
                  cx_expr: Gc<ast::Expr>,
                  expr: Gc<ast::Expr>) -> Gc<ast::Expr> {
    let uses = [
        &["syntax", "ext", "quote", "rt"],
    ].iter().map(|path| {
        let path = path.iter().map(|s| s.to_string()).collect();
        cx.view_use_glob(sp, ast::Inherited, ids_ext(path))
    }).collect();

    let stmt_let_ext_cx = cx.stmt_let(sp, false, id_ext("ext_cx"), cx_expr);

    cx.expr_block(cx.block_all(sp, uses, vec!(stmt_let_ext_cx), Some(expr)))
}

fn expand_parse_call(cx: &ExtCtxt,
                     sp: Span,
                     parse_method: &str,
                     arg_exprs: Vec<Gc<ast::Expr>>,
                     tts: &[ast::TokenTree]) -> Gc<ast::Expr> {
    let (cx_expr, tts_expr) = expand_tts(cx, sp, tts);

    let cfg_call = || cx.expr_method_call(
        sp, cx.expr_ident(sp, id_ext("ext_cx")),
        id_ext("cfg"), Vec::new());

    let parse_sess_call = || cx.expr_method_call(
        sp, cx.expr_ident(sp, id_ext("ext_cx")),
        id_ext("parse_sess"), Vec::new());

    let new_parser_call =
        cx.expr_call(sp,
                     cx.expr_ident(sp, id_ext("new_parser_from_tts")),
                     vec!(parse_sess_call(), cfg_call(), tts_expr));

    let expr = cx.expr_method_call(sp, new_parser_call, id_ext(parse_method),
                                   arg_exprs);

    expand_wrapper(cx, sp, cx_expr, expr)
}
