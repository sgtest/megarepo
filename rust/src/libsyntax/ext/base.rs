// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast;
use ast::Name;
use codemap;
use codemap::{CodeMap, span, ExpnInfo};
use diagnostic::span_handler;
use ext;
use parse;
use parse::token;
use parse::token::{ident_to_str, intern, str_to_ident};

use std::hashmap::HashMap;

// new-style macro! tt code:
//
//    SyntaxExpanderTT, SyntaxExpanderTTItem, MacResult,
//    NormalTT, IdentTT
//
// also note that ast::mac used to have a bunch of extraneous cases and
// is now probably a redundant AST node, can be merged with
// ast::mac_invoc_tt.

pub struct MacroDef {
    name: @str,
    ext: SyntaxExtension
}

pub type ItemDecorator = @fn(@ExtCtxt,
                             span,
                             @ast::meta_item,
                             ~[@ast::item])
                          -> ~[@ast::item];

pub struct SyntaxExpanderTT {
    expander: SyntaxExpanderTTFun,
    span: Option<span>
}

pub type SyntaxExpanderTTFun = @fn(@ExtCtxt,
                                   span,
                                   &[ast::token_tree])
                                -> MacResult;

pub struct SyntaxExpanderTTItem {
    expander: SyntaxExpanderTTItemFun,
    span: Option<span>
}

pub type SyntaxExpanderTTItemFun = @fn(@ExtCtxt,
                                       span,
                                       ast::ident,
                                       ~[ast::token_tree])
                                    -> MacResult;

pub enum MacResult {
    MRExpr(@ast::expr),
    MRItem(@ast::item),
    MRAny(@fn() -> @ast::expr,
          @fn() -> Option<@ast::item>,
          @fn() -> @ast::stmt),
    MRDef(MacroDef)
}

pub enum SyntaxExtension {

    // #[auto_encode] and such
    ItemDecorator(ItemDecorator),

    // Token-tree expanders
    NormalTT(SyntaxExpanderTT),

    // An IdentTT is a macro that has an
    // identifier in between the name of the
    // macro and the argument. Currently,
    // the only examples of this are
    // macro_rules! and proto!

    // perhaps macro_rules! will lose its odd special identifier argument,
    // and this can go away also
    IdentTT(SyntaxExpanderTTItem),
}

// The SyntaxEnv is the environment that's threaded through the expansion
// of macros. It contains bindings for macros, and also a special binding
// for " block" (not a legal identifier) that maps to a BlockInfo
pub type SyntaxEnv = @mut MapChain<Name, Transformer>;

// Transformer : the codomain of SyntaxEnvs

pub enum Transformer {
    // this identifier maps to a syntax extension or macro
    SE(SyntaxExtension),
    // blockinfo : this is ... well, it's simpler than threading
    // another whole data stack-structured data structure through
    // expansion. Basically, there's an invariant that every
    // map must contain a binding for " block".
    BlockInfo(BlockInfo)
}

pub struct BlockInfo {
    // should macros escape from this scope?
    macros_escape : bool,
    // what are the pending renames?
    pending_renames : @mut RenameList
}

// a list of ident->name renamings
type RenameList = ~[(ast::ident,Name)];

// The base map of methods for expanding syntax extension
// AST nodes into full ASTs
pub fn syntax_expander_table() -> SyntaxEnv {
    // utility function to simplify creating NormalTT syntax extensions
    fn builtin_normal_tt(f: SyntaxExpanderTTFun) -> @Transformer {
        @SE(NormalTT(SyntaxExpanderTT{expander: f, span: None}))
    }
    // utility function to simplify creating IdentTT syntax extensions
    fn builtin_item_tt(f: SyntaxExpanderTTItemFun) -> @Transformer {
        @SE(IdentTT(SyntaxExpanderTTItem{expander: f, span: None}))
    }
    let mut syntax_expanders = HashMap::new();
    // NB identifier starts with space, and can't conflict with legal idents
    syntax_expanders.insert(intern(&" block"),
                            @BlockInfo(BlockInfo{
                                macros_escape : false,
                                pending_renames : @mut ~[]
                            }));
    syntax_expanders.insert(intern(&"macro_rules"),
                            builtin_item_tt(
                                ext::tt::macro_rules::add_new_extension));
    syntax_expanders.insert(intern(&"fmt"),
                            builtin_normal_tt(ext::fmt::expand_syntax_ext));
    syntax_expanders.insert(
        intern(&"auto_encode"),
        @SE(ItemDecorator(ext::auto_encode::expand_auto_encode)));
    syntax_expanders.insert(
        intern(&"auto_decode"),
        @SE(ItemDecorator(ext::auto_encode::expand_auto_decode)));
    syntax_expanders.insert(intern(&"env"),
                            builtin_normal_tt(ext::env::expand_syntax_ext));
    syntax_expanders.insert(intern("bytes"),
                            builtin_normal_tt(ext::bytes::expand_syntax_ext));
    syntax_expanders.insert(intern("concat_idents"),
                            builtin_normal_tt(
                                ext::concat_idents::expand_syntax_ext));
    syntax_expanders.insert(intern(&"log_syntax"),
                            builtin_normal_tt(
                                ext::log_syntax::expand_syntax_ext));
    syntax_expanders.insert(intern(&"deriving"),
                            @SE(ItemDecorator(
                                ext::deriving::expand_meta_deriving)));

    // Quasi-quoting expanders
    syntax_expanders.insert(intern(&"quote_tokens"),
                       builtin_normal_tt(ext::quote::expand_quote_tokens));
    syntax_expanders.insert(intern(&"quote_expr"),
                       builtin_normal_tt(ext::quote::expand_quote_expr));
    syntax_expanders.insert(intern(&"quote_ty"),
                       builtin_normal_tt(ext::quote::expand_quote_ty));
    syntax_expanders.insert(intern(&"quote_item"),
                       builtin_normal_tt(ext::quote::expand_quote_item));
    syntax_expanders.insert(intern(&"quote_pat"),
                       builtin_normal_tt(ext::quote::expand_quote_pat));
    syntax_expanders.insert(intern(&"quote_stmt"),
                       builtin_normal_tt(ext::quote::expand_quote_stmt));

    syntax_expanders.insert(intern(&"line"),
                            builtin_normal_tt(
                                ext::source_util::expand_line));
    syntax_expanders.insert(intern(&"col"),
                            builtin_normal_tt(
                                ext::source_util::expand_col));
    syntax_expanders.insert(intern(&"file"),
                            builtin_normal_tt(
                                ext::source_util::expand_file));
    syntax_expanders.insert(intern(&"stringify"),
                            builtin_normal_tt(
                                ext::source_util::expand_stringify));
    syntax_expanders.insert(intern(&"include"),
                            builtin_normal_tt(
                                ext::source_util::expand_include));
    syntax_expanders.insert(intern(&"include_str"),
                            builtin_normal_tt(
                                ext::source_util::expand_include_str));
    syntax_expanders.insert(intern(&"include_bin"),
                            builtin_normal_tt(
                                ext::source_util::expand_include_bin));
    syntax_expanders.insert(intern(&"module_path"),
                            builtin_normal_tt(
                                ext::source_util::expand_mod));
    syntax_expanders.insert(intern(&"proto"),
                            builtin_item_tt(ext::pipes::expand_proto));
    syntax_expanders.insert(intern(&"asm"),
                            builtin_normal_tt(ext::asm::expand_asm));
    syntax_expanders.insert(
        intern(&"trace_macros"),
        builtin_normal_tt(ext::trace_macros::expand_trace_macros));
    MapChain::new(~syntax_expanders)
}

// One of these is made during expansion and incrementally updated as we go;
// when a macro expansion occurs, the resulting nodes have the backtrace()
// -> expn_info of their expansion context stored into their span.
pub struct ExtCtxt {
    parse_sess: @mut parse::ParseSess,
    cfg: ast::crate_cfg,
    backtrace: @mut Option<@ExpnInfo>,

    // These two @mut's should really not be here,
    // but the self types for CtxtRepr are all wrong
    // and there are bugs in the code for object
    // types that make this hard to get right at the
    // moment. - nmatsakis
    mod_path: @mut ~[ast::ident],
    trace_mac: @mut bool
}

impl ExtCtxt {
    pub fn new(parse_sess: @mut parse::ParseSess, cfg: ast::crate_cfg)
               -> @ExtCtxt {
        @ExtCtxt {
            parse_sess: parse_sess,
            cfg: cfg,
            backtrace: @mut None,
            mod_path: @mut ~[],
            trace_mac: @mut false
        }
    }

    pub fn codemap(&self) -> @CodeMap { self.parse_sess.cm }
    pub fn parse_sess(&self) -> @mut parse::ParseSess { self.parse_sess }
    pub fn cfg(&self) -> ast::crate_cfg { self.cfg.clone() }
    pub fn call_site(&self) -> span {
        match *self.backtrace {
            Some(@ExpnInfo {call_site: cs, _}) => cs,
            None => self.bug("missing top span")
        }
    }
    pub fn print_backtrace(&self) { }
    pub fn backtrace(&self) -> Option<@ExpnInfo> { *self.backtrace }
    pub fn mod_push(&self, i: ast::ident) { self.mod_path.push(i); }
    pub fn mod_pop(&self) { self.mod_path.pop(); }
    pub fn mod_path(&self) -> ~[ast::ident] { (*self.mod_path).clone() }
    pub fn bt_push(&self, ei: codemap::ExpnInfo) {
        match ei {
            ExpnInfo {call_site: cs, callee: ref callee} => {
                *self.backtrace =
                    Some(@ExpnInfo {
                        call_site: span {lo: cs.lo, hi: cs.hi,
                                         expn_info: *self.backtrace},
                        callee: *callee});
            }
        }
    }
    pub fn bt_pop(&self) {
        match *self.backtrace {
            Some(@ExpnInfo {
                call_site: span {expn_info: prev, _}, _}) => {
                *self.backtrace = prev
            }
            _ => self.bug("tried to pop without a push")
        }
    }
    pub fn span_fatal(&self, sp: span, msg: &str) -> ! {
        self.print_backtrace();
        self.parse_sess.span_diagnostic.span_fatal(sp, msg);
    }
    pub fn span_err(&self, sp: span, msg: &str) {
        self.print_backtrace();
        self.parse_sess.span_diagnostic.span_err(sp, msg);
    }
    pub fn span_warn(&self, sp: span, msg: &str) {
        self.print_backtrace();
        self.parse_sess.span_diagnostic.span_warn(sp, msg);
    }
    pub fn span_unimpl(&self, sp: span, msg: &str) -> ! {
        self.print_backtrace();
        self.parse_sess.span_diagnostic.span_unimpl(sp, msg);
    }
    pub fn span_bug(&self, sp: span, msg: &str) -> ! {
        self.print_backtrace();
        self.parse_sess.span_diagnostic.span_bug(sp, msg);
    }
    pub fn bug(&self, msg: &str) -> ! {
        self.print_backtrace();
        self.parse_sess.span_diagnostic.handler().bug(msg);
    }
    pub fn next_id(&self) -> ast::node_id {
        parse::next_node_id(self.parse_sess)
    }
    pub fn trace_macros(&self) -> bool {
        *self.trace_mac
    }
    pub fn set_trace_macros(&self, x: bool) {
        *self.trace_mac = x
    }
    pub fn str_of(&self, id: ast::ident) -> @str {
        ident_to_str(&id)
    }
    pub fn ident_of(&self, st: &str) -> ast::ident {
        str_to_ident(st)
    }
}

pub fn expr_to_str(cx: @ExtCtxt, expr: @ast::expr, err_msg: ~str) -> @str {
    match expr.node {
      ast::expr_lit(l) => match l.node {
        ast::lit_str(s) => s,
        _ => cx.span_fatal(l.span, err_msg)
      },
      _ => cx.span_fatal(expr.span, err_msg)
    }
}

pub fn expr_to_ident(cx: @ExtCtxt,
                     expr: @ast::expr,
                     err_msg: &str) -> ast::ident {
    match expr.node {
      ast::expr_path(ref p) => {
        if p.types.len() > 0u || p.idents.len() != 1u {
            cx.span_fatal(expr.span, err_msg);
        }
        return p.idents[0];
      }
      _ => cx.span_fatal(expr.span, err_msg)
    }
}

pub fn check_zero_tts(cx: @ExtCtxt, sp: span, tts: &[ast::token_tree],
                      name: &str) {
    if tts.len() != 0 {
        cx.span_fatal(sp, fmt!("%s takes no arguments", name));
    }
}

pub fn get_single_str_from_tts(cx: @ExtCtxt,
                               sp: span,
                               tts: &[ast::token_tree],
                               name: &str) -> @str {
    if tts.len() != 1 {
        cx.span_fatal(sp, fmt!("%s takes 1 argument.", name));
    }

    match tts[0] {
        ast::tt_tok(_, token::LIT_STR(ident)) => cx.str_of(ident),
        _ =>
        cx.span_fatal(sp, fmt!("%s requires a string.", name))
    }
}

pub fn get_exprs_from_tts(cx: @ExtCtxt, tts: &[ast::token_tree])
                       -> ~[@ast::expr] {
    let p = parse::new_parser_from_tts(cx.parse_sess(),
                                       cx.cfg(),
                                       tts.to_owned());
    let mut es = ~[];
    while *p.token != token::EOF {
        if es.len() != 0 {
            p.eat(&token::COMMA);
        }
        es.push(p.parse_expr());
    }
    es
}

// in order to have some notion of scoping for macros,
// we want to implement the notion of a transformation
// environment.

// This environment maps Names to Transformers.
// Initially, this includes macro definitions and
// block directives.



// Actually, the following implementation is parameterized
// by both key and value types.

//impl question: how to implement it? Initially, the
// env will contain only macros, so it might be painful
// to add an empty frame for every context. Let's just
// get it working, first....

// NB! the mutability of the underlying maps means that
// if expansion is out-of-order, a deeper scope may be
// able to refer to a macro that was added to an enclosing
// scope lexically later than the deeper scope.

// Note on choice of representation: I've been pushed to
// use a top-level managed pointer by some difficulties
// with pushing and popping functionally, and the ownership
// issues.  As a result, the values returned by the table
// also need to be managed; the &'self ... type that Maps
// return won't work for things that need to get outside
// of that managed pointer.  The easiest way to do this
// is just to insist that the values in the tables are
// managed to begin with.

// a transformer env is either a base map or a map on top
// of another chain.
pub enum MapChain<K,V> {
    BaseMapChain(~HashMap<K,@V>),
    ConsMapChain(~HashMap<K,@V>,@mut MapChain<K,V>)
}


// get the map from an env frame
impl <K: Eq + Hash + IterBytes + 'static, V: 'static> MapChain<K,V>{
    // Constructor. I don't think we need a zero-arg one.
    fn new(init: ~HashMap<K,@V>) -> @mut MapChain<K,V> {
        @mut BaseMapChain(init)
    }

    // add a new frame to the environment (functionally)
    fn push_frame (@mut self) -> @mut MapChain<K,V> {
        @mut ConsMapChain(~HashMap::new() ,self)
    }

// no need for pop, it'll just be functional.

    // utility fn...

    // ugh: can't get this to compile with mut because of the
    // lack of flow sensitivity.
    fn get_map<'a>(&'a self) -> &'a HashMap<K,@V> {
        match *self {
            BaseMapChain (~ref map) => map,
            ConsMapChain (~ref map,_) => map
        }
    }

// traits just don't work anywhere...?
//impl Map<Name,SyntaxExtension> for MapChain {

    fn contains_key (&self, key: &K) -> bool {
        match *self {
            BaseMapChain (ref map) => map.contains_key(key),
            ConsMapChain (ref map,ref rest) =>
            (map.contains_key(key)
             || rest.contains_key(key))
        }
    }
    // should each_key and each_value operate on shadowed
    // names? I think not.
    // delaying implementing this....
    fn each_key (&self, _f: &fn (&K)->bool) {
        fail!("unimplemented 2013-02-15T10:01");
    }

    fn each_value (&self, _f: &fn (&V) -> bool) {
        fail!("unimplemented 2013-02-15T10:02");
    }

    // Returns a copy of the value that the name maps to.
    // Goes down the chain 'til it finds one (or bottom out).
    fn find (&self, key: &K) -> Option<@V> {
        match self.get_map().find (key) {
            Some(ref v) => Some(**v),
            None => match *self {
                BaseMapChain (_) => None,
                ConsMapChain (_,ref rest) => rest.find(key)
            }
        }
    }

    fn find_in_topmost_frame(&self, key: &K) -> Option<@V> {
        let map = match *self {
            BaseMapChain(ref map) => map,
            ConsMapChain(ref map,_) => map
        };
        // strip one layer of indirection off the pointer.
        map.find(key).map(|r| {**r})
    }

    // insert the binding into the top-level map
    fn insert (&mut self, key: K, ext: @V) -> bool {
        // can't abstract over get_map because of flow sensitivity...
        match *self {
            BaseMapChain (~ref mut map) => map.insert(key, ext),
            ConsMapChain (~ref mut map,_) => map.insert(key,ext)
        }
    }
    // insert the binding into the topmost frame for which the binding
    // associated with 'n' exists and satisfies pred
    // ... there are definitely some opportunities for abstraction
    // here that I'm ignoring. (e.g., manufacturing a predicate on
    // the maps in the chain, and using an abstract "find".
    fn insert_into_frame(&mut self, key: K, ext: @V, n: K, pred: &fn(&@V)->bool) {
        match *self {
            BaseMapChain (~ref mut map) => {
                if satisfies_pred(map,&n,pred) {
                    map.insert(key,ext);
                } else {
                    fail!(~"expected map chain containing satisfying frame")
                }
            },
            ConsMapChain (~ref mut map, rest) => {
                if satisfies_pred(map,&n,|v|pred(v)) {
                    map.insert(key,ext);
                } else {
                    rest.insert_into_frame(key,ext,n,pred)
                }
            }
        }
    }
}

// returns true if the binding for 'n' satisfies 'pred' in 'map'
fn satisfies_pred<K : Eq + Hash + IterBytes,V>(map : &mut HashMap<K,V>,
                                               n: &K,
                                               pred: &fn(&V)->bool)
    -> bool {
    match map.find(n) {
        Some(ref v) => (pred(*v)),
        None => false
    }
}

#[cfg(test)]
mod test {
    use super::MapChain;
    use std::hashmap::HashMap;

    #[test] fn testenv () {
        let mut a = HashMap::new();
        a.insert (@"abc",@15);
        let m = MapChain::new(~a);
        m.insert (@"def",@16);
        // FIXME: #4492 (ICE)  assert_eq!(m.find(&@"abc"),Some(@15));
        //  ....               assert_eq!(m.find(&@"def"),Some(@16));
        assert_eq!(*(m.find(&@"abc").get()),15);
        assert_eq!(*(m.find(&@"def").get()),16);
        let n = m.push_frame();
        // old bindings are still present:
        assert_eq!(*(n.find(&@"abc").get()),15);
        assert_eq!(*(n.find(&@"def").get()),16);
        n.insert (@"def",@17);
        // n shows the new binding
        assert_eq!(*(n.find(&@"abc").get()),15);
        assert_eq!(*(n.find(&@"def").get()),17);
        // ... but m still has the old ones
        // FIXME: #4492: assert_eq!(m.find(&@"abc"),Some(@15));
        // FIXME: #4492: assert_eq!(m.find(&@"def"),Some(@16));
        assert_eq!(*(m.find(&@"abc").get()),15);
        assert_eq!(*(m.find(&@"def").get()),16);
    }
}
