// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use ast::{RegionTyParamBound, TraitTyParamBound, required, provided};
use ast;
use ast_util;
use ast_util::{operator_prec};
use attr;
use codemap::{CodeMap, BytePos};
use codemap;
use diagnostic;
use parse::classify::{expr_is_simple_block, expr_requires_semi_to_be_stmt};
use parse::classify::{stmt_ends_with_semi};
use parse::token::ident_interner;
use parse::{comments, lexer, token};
use parse;
use print::pp::{break_offset, word, printer, space, zerobreak, hardbreak};
use print::pp::{breaks, consistent, inconsistent, eof};
use print::pp;
use print::pprust;

use core::char;
use core::dvec::DVec;
use core::io;
use core::option;
use core::str::{push_str, push_char};
use core::str;
use core::u64;
use core::vec;

// The ps is stored here to prevent recursive type.
enum ann_node {
    node_block(ps, ast::blk),
    node_item(ps, @ast::item),
    node_expr(ps, @ast::expr),
    node_pat(ps, @ast::pat),
}
struct pp_ann {
    pre: fn@(ann_node),
    post: fn@(ann_node)
}

fn no_ann() -> pp_ann {
    fn ignore(_node: ann_node) { }
    return pp_ann {pre: ignore, post: ignore};
}

type ps =
    @{s: pp::printer,
      cm: Option<@CodeMap>,
      intr: @token::ident_interner,
      comments: Option<~[comments::cmnt]>,
      literals: Option<~[comments::lit]>,
      mut cur_cmnt: uint,
      mut cur_lit: uint,
      boxes: DVec<pp::breaks>,
      ann: pp_ann};

fn ibox(s: ps, u: uint) {
    s.boxes.push(pp::inconsistent);
    pp::ibox(s.s, u);
}

fn end(s: ps) {
    s.boxes.pop();
    pp::end(s.s);
}

fn rust_printer(writer: io::Writer, intr: @ident_interner) -> ps {
    return @{s: pp::mk_printer(writer, default_columns),
             cm: None::<@CodeMap>,
             intr: intr,
             comments: None::<~[comments::cmnt]>,
             literals: None::<~[comments::lit]>,
             mut cur_cmnt: 0u,
             mut cur_lit: 0u,
             boxes: DVec(),
             ann: no_ann()};
}

const indent_unit: uint = 4u;
const match_indent_unit: uint = 2u;

const default_columns: uint = 78u;

// Requires you to pass an input filename and reader so that
// it can scan the input text for comments and literals to
// copy forward.
fn print_crate(cm: @CodeMap, intr: @ident_interner,
               span_diagnostic: diagnostic::span_handler,
               crate: @ast::crate, filename: ~str, in: io::Reader,
               out: io::Writer, ann: pp_ann, is_expanded: bool) {
    let r = comments::gather_comments_and_literals(span_diagnostic,
                                                   filename, in);
    let s =
        @{s: pp::mk_printer(out, default_columns),
          cm: Some(cm),
          intr: intr,
          comments: Some(r.cmnts),
          // If the code is post expansion, don't use the table of
          // literals, since it doesn't correspond with the literals
          // in the AST anymore.
          literals: if is_expanded { None } else { Some(r.lits) },
          mut cur_cmnt: 0u,
          mut cur_lit: 0u,
          boxes: DVec(),
          ann: ann};
    print_crate_(s, crate);
}

fn print_crate_(s: ps, &&crate: @ast::crate) {
    print_mod(s, crate.node.module, crate.node.attrs);
    print_remaining_comments(s);
    eof(s.s);
}

fn ty_to_str(ty: @ast::Ty, intr: @ident_interner) -> ~str {
    to_str(ty, print_type, intr)
}

fn pat_to_str(pat: @ast::pat, intr: @ident_interner) -> ~str {
    to_str(pat, print_irrefutable_pat, intr)
}

fn expr_to_str(e: @ast::expr, intr: @ident_interner) -> ~str {
    to_str(e, print_expr, intr)
}

fn tt_to_str(tt: ast::token_tree, intr: @ident_interner) -> ~str {
    to_str(tt, print_tt, intr)
}

fn tts_to_str(tts: &[ast::token_tree], intr: @ident_interner) -> ~str {
    to_str(tts, print_tts, intr)
}

fn stmt_to_str(s: ast::stmt, intr: @ident_interner) -> ~str {
    to_str(s, print_stmt, intr)
}

fn item_to_str(i: @ast::item, intr: @ident_interner) -> ~str {
    to_str(i, print_item, intr)
}

fn typarams_to_str(tps: ~[ast::ty_param], intr: @ident_interner) -> ~str {
    to_str(tps, print_type_params, intr)
}

fn path_to_str(&&p: @ast::path, intr: @ident_interner) -> ~str {
    to_str(p, |a,b| print_path(a, b, false), intr)
}

fn fun_to_str(decl: ast::fn_decl, name: ast::ident,
              params: ~[ast::ty_param], intr: @ident_interner) -> ~str {
    do io::with_str_writer |wr| {
        let s = rust_printer(wr, intr);
        print_fn(s, decl, None, name, params, None, ast::inherited);
        end(s); // Close the head box
        end(s); // Close the outer box
        eof(s.s);
    }
}

fn block_to_str(blk: ast::blk, intr: @ident_interner) -> ~str {
    do io::with_str_writer |wr| {
        let s = rust_printer(wr, intr);
        // containing cbox, will be closed by print-block at }
        cbox(s, indent_unit);
        // head-ibox, will be closed by print-block after {
        ibox(s, 0u);
        print_block(s, blk);
        eof(s.s);
    }
}

fn meta_item_to_str(mi: @ast::meta_item, intr: @ident_interner) -> ~str {
    to_str(mi, print_meta_item, intr)
}

fn attribute_to_str(attr: ast::attribute, intr: @ident_interner) -> ~str {
    to_str(attr, print_attribute, intr)
}

fn variant_to_str(var: ast::variant, intr: @ident_interner) -> ~str {
    to_str(var, print_variant, intr)
}

fn cbox(s: ps, u: uint) {
    s.boxes.push(pp::consistent);
    pp::cbox(s.s, u);
}

fn box(s: ps, u: uint, b: pp::breaks) {
    s.boxes.push(b);
    pp::box(s.s, u, b);
}

fn nbsp(s: ps) { word(s.s, ~" "); }

fn word_nbsp(s: ps, w: ~str) { word(s.s, w); nbsp(s); }

fn word_space(s: ps, w: ~str) { word(s.s, w); space(s.s); }

fn popen(s: ps) { word(s.s, ~"("); }

fn pclose(s: ps) { word(s.s, ~")"); }

fn head(s: ps, w: ~str) {
    // outer-box is consistent
    cbox(s, indent_unit);
    // head-box is inconsistent
    ibox(s, str::len(w) + 1);
    // keyword that starts the head
    if !w.is_empty() {
        word_nbsp(s, w);
    }
}

fn bopen(s: ps) {
    word(s.s, ~"{");
    end(s); // close the head-box
}

fn bclose_(s: ps, span: codemap::span, indented: uint) {
    bclose_maybe_open(s, span, indented, true);
}
fn bclose_maybe_open (s: ps, span: codemap::span, indented: uint,
                     close_box: bool) {
    maybe_print_comment(s, span.hi);
    break_offset_if_not_bol(s, 1u, -(indented as int));
    word(s.s, ~"}");
    if close_box {
        end(s); // close the outer-box
    }
}
fn bclose(s: ps, span: codemap::span) { bclose_(s, span, indent_unit); }

fn is_begin(s: ps) -> bool {
    match s.s.last_token() { pp::BEGIN(_) => true, _ => false }
}

fn is_end(s: ps) -> bool {
    match s.s.last_token() { pp::END => true, _ => false }
}

fn is_bol(s: ps) -> bool {
    return s.s.last_token().is_eof() || s.s.last_token().is_hardbreak_tok();
}

fn in_cbox(s: ps) -> bool {
    let len = s.boxes.len();
    if len == 0u { return false; }
    return s.boxes[len - 1u] == pp::consistent;
}

fn hardbreak_if_not_bol(s: ps) { if !is_bol(s) { hardbreak(s.s); } }
fn space_if_not_bol(s: ps) { if !is_bol(s) { space(s.s); } }
fn break_offset_if_not_bol(s: ps, n: uint, off: int) {
    if !is_bol(s) {
        break_offset(s.s, n, off);
    } else {
        if off != 0 && s.s.last_token().is_hardbreak_tok() {
            // We do something pretty sketchy here: tuck the nonzero
            // offset-adjustment we were going to deposit along with the
            // break into the previous hardbreak.
            s.s.replace_last_token(pp::hardbreak_tok_offset(off));
        }
    }
}

// Synthesizes a comment that was not textually present in the original source
// file.
fn synth_comment(s: ps, text: ~str) {
    word(s.s, ~"/*");
    space(s.s);
    word(s.s, text);
    space(s.s);
    word(s.s, ~"*/");
}

fn commasep<IN>(s: ps, b: breaks, elts: ~[IN], op: fn(ps, IN)) {
    box(s, 0u, b);
    let mut first = true;
    for elts.each |elt| {
        if first { first = false; } else { word_space(s, ~","); }
        op(s, *elt);
    }
    end(s);
}


fn commasep_cmnt<IN>(s: ps, b: breaks, elts: ~[IN], op: fn(ps, IN),
                     get_span: fn(IN) -> codemap::span) {
    box(s, 0u, b);
    let len = vec::len::<IN>(elts);
    let mut i = 0u;
    for elts.each |elt| {
        maybe_print_comment(s, get_span(*elt).hi);
        op(s, *elt);
        i += 1u;
        if i < len {
            word(s.s, ~",");
            maybe_print_trailing_comment(s, get_span(*elt),
                                         Some(get_span(elts[i]).hi));
            space_if_not_bol(s);
        }
    }
    end(s);
}

fn commasep_exprs(s: ps, b: breaks, exprs: ~[@ast::expr]) {
    fn expr_span(&&expr: @ast::expr) -> codemap::span { return expr.span; }
    commasep_cmnt(s, b, exprs, print_expr, expr_span);
}

fn print_mod(s: ps, _mod: ast::_mod, attrs: ~[ast::attribute]) {
    print_inner_attributes(s, attrs);
    for _mod.view_items.each |vitem| {
        print_view_item(s, *vitem);
    }
    for _mod.items.each |item| { print_item(s, *item); }
}

fn print_foreign_mod(s: ps, nmod: ast::foreign_mod,
                     attrs: ~[ast::attribute]) {
    print_inner_attributes(s, attrs);
    for nmod.view_items.each |vitem| {
        print_view_item(s, *vitem);
    }
    for nmod.items.each |item| { print_foreign_item(s, *item); }
}

fn print_region(s: ps, prefix: ~str, region: @ast::region, sep: ~str) {
    word(s.s, prefix);
    match region.node {
        ast::re_anon => {
            return;
        }
        ast::re_static => {
            word_space(s, ~"static")
        }
        ast::re_self => {
            word_space(s, ~"self")
        }
        ast::re_named(name) => {
            print_ident(s, name);
        }
    }
    word(s.s, sep);
}

fn print_type(s: ps, &&ty: @ast::Ty) {
    print_type_ex(s, ty, false);
}

fn print_type_ex(s: ps, &&ty: @ast::Ty, print_colons: bool) {
    maybe_print_comment(s, ty.span.lo);
    ibox(s, 0u);
    match ty.node {
      ast::ty_nil => word(s.s, ~"()"),
      ast::ty_bot => word(s.s, ~"!"),
      ast::ty_box(mt) => { word(s.s, ~"@"); print_mt(s, mt); }
      ast::ty_uniq(mt) => { word(s.s, ~"~"); print_mt(s, mt); }
      ast::ty_vec(mt) => {
        word(s.s, ~"[");
        match mt.mutbl {
          ast::m_mutbl => word_space(s, ~"mut"),
          ast::m_const => word_space(s, ~"const"),
          ast::m_imm => ()
        }
        print_type(s, mt.ty);
        word(s.s, ~"]");
      }
      ast::ty_ptr(mt) => { word(s.s, ~"*"); print_mt(s, mt); }
      ast::ty_rptr(region, mt) => {
          print_region(s, ~"&", region, ~"/");
          print_mt(s, mt);
      }
      ast::ty_rec(ref fields) => {
        word(s.s, ~"{");
        fn print_field(s: ps, f: ast::ty_field) {
            cbox(s, indent_unit);
            print_mutability(s, f.node.mt.mutbl);
            print_ident(s, f.node.ident);
            word_space(s, ~":");
            print_type(s, f.node.mt.ty);
            end(s);
        }
        fn get_span(f: ast::ty_field) -> codemap::span { return f.span; }
        commasep_cmnt(s, consistent, (*fields), print_field, get_span);
        word(s.s, ~",}");
      }
      ast::ty_tup(elts) => {
        popen(s);
        commasep(s, inconsistent, elts, print_type);
        pclose(s);
      }
      ast::ty_fn(f) => {
        print_ty_fn(s, Some(f.proto), f.region, f.purity,
                    f.onceness, f.bounds, f.decl, None, None, None);
      }
      ast::ty_path(path, _) => print_path(s, path, print_colons),
      ast::ty_fixed_length_vec(mt, v) => {
        word(s.s, ~"[");
        match mt.mutbl {
            ast::m_mutbl => word_space(s, ~"mut"),
            ast::m_const => word_space(s, ~"const"),
            ast::m_imm => ()
        }
        print_type(s, mt.ty);
        word(s.s, ~" * ");
        word(s.s, fmt!("%u", v));
        word(s.s, ~"]");
      }
      ast::ty_mac(_) => {
          fail ~"print_type doesn't know how to print a ty_mac";
      }
      ast::ty_infer => {
          fail ~"print_type shouldn't see a ty_infer";
      }

    }
    end(s);
}

fn print_foreign_item(s: ps, item: @ast::foreign_item) {
    hardbreak_if_not_bol(s);
    maybe_print_comment(s, item.span.lo);
    print_outer_attributes(s, item.attrs);
    match item.node {
      ast::foreign_item_fn(decl, purity, typarams) => {
        print_fn(s, decl, Some(purity), item.ident, typarams, None,
                 ast::inherited);
        end(s); // end head-ibox
        word(s.s, ~";");
        end(s); // end the outer fn box
      }
      ast::foreign_item_const(t) => {
        head(s, ~"const");
        print_ident(s, item.ident);
        word_space(s, ~":");
        print_type(s, t);
        word(s.s, ~";");
        end(s); // end the head-ibox
        end(s); // end the outer cbox
      }
    }
}

fn print_item(s: ps, &&item: @ast::item) {
    hardbreak_if_not_bol(s);
    maybe_print_comment(s, item.span.lo);
    print_outer_attributes(s, item.attrs);
    let ann_node = node_item(s, item);
    (s.ann.pre)(ann_node);
    match item.node {
      ast::item_const(ty, expr) => {
        head(s, visibility_qualified(item.vis, ~"const"));
        print_ident(s, item.ident);
        word_space(s, ~":");
        print_type(s, ty);
        space(s.s);
        end(s); // end the head-ibox

        word_space(s, ~"=");
        print_expr(s, expr);
        word(s.s, ~";");
        end(s); // end the outer cbox

      }
      ast::item_fn(decl, purity, typarams, ref body) => {
        print_fn(s, decl, Some(purity), item.ident, typarams, None,
                 item.vis);
        word(s.s, ~" ");
        print_block_with_attrs(s, (*body), item.attrs);
      }
      ast::item_mod(_mod) => {
        head(s, visibility_qualified(item.vis, ~"mod"));
        print_ident(s, item.ident);
        nbsp(s);
        bopen(s);
        print_mod(s, _mod, item.attrs);
        bclose(s, item.span);
      }
      ast::item_foreign_mod(nmod) => {
        head(s, visibility_qualified(item.vis, ~"extern"));
        print_string(s, *s.intr.get(nmod.abi));
        nbsp(s);
        match nmod.sort {
            ast::named => {
                word_nbsp(s, ~"mod");
                print_ident(s, item.ident);
                nbsp(s);
            }
            ast::anonymous => {}
        }
        bopen(s);
        print_foreign_mod(s, nmod, item.attrs);
        bclose(s, item.span);
      }
      ast::item_ty(ty, params) => {
        ibox(s, indent_unit);
        ibox(s, 0u);
        word_nbsp(s, visibility_qualified(item.vis, ~"type"));
        print_ident(s, item.ident);
        print_type_params(s, params);
        end(s); // end the inner ibox

        space(s.s);
        word_space(s, ~"=");
        print_type(s, ty);
        word(s.s, ~";");
        end(s); // end the outer ibox
      }
      ast::item_enum(ref enum_definition, params) => {
        print_enum_def(s, (*enum_definition), params, item.ident,
                       item.span, item.vis);
      }
      ast::item_struct(struct_def, tps) => {
          head(s, visibility_qualified(item.vis, ~"struct"));
          print_struct(s, struct_def, tps, item.ident, item.span);
      }

      ast::item_impl(tps, opt_trait, ty, methods) => {
        head(s, visibility_qualified(item.vis, ~"impl"));
        if tps.is_not_empty() {
            print_type_params(s, tps);
            space(s.s);
        }
        print_type(s, ty);

        match opt_trait {
            Some(t) => {
                word_space(s, ~":");
                print_path(s, t.path, false);
            }
            None => ()
        };
        space(s.s);

        if methods.len() == 0 {
            word(s.s, ~";");
        } else {
            bopen(s);
            for methods.each |meth| {
               print_method(s, *meth);
            }
            bclose(s, item.span);
        }
      }
      ast::item_trait(tps, traits, ref methods) => {
        head(s, visibility_qualified(item.vis, ~"trait"));
        print_ident(s, item.ident);
        print_type_params(s, tps);
        if vec::len(traits) != 0u {
            word(s.s, ~":");
            for vec::each(traits) |trait_| {
                nbsp(s);
                print_path(s, trait_.path, false);
            }
        }
        word(s.s, ~" ");
        bopen(s);
        for (*methods).each |meth| {
            print_trait_method(s, *meth);
        }
        bclose(s, item.span);
      }
      ast::item_mac(ast::spanned { node: ast::mac_invoc_tt(pth, ref tts),
                                   _}) => {
        print_visibility(s, item.vis);
        print_path(s, pth, false);
        word(s.s, ~"! ");
        print_ident(s, item.ident);
        cbox(s, indent_unit);
        popen(s);
        print_tts(s, *tts);
        pclose(s);
        end(s);
      }
    }
    (s.ann.post)(ann_node);
}

fn print_enum_def(s: ps, enum_definition: ast::enum_def,
                  params: ~[ast::ty_param], ident: ast::ident,
                  span: ast::span, visibility: ast::visibility) {
    let mut newtype =
        vec::len(enum_definition.variants) == 1u &&
        ident == enum_definition.variants[0].node.name;
    if newtype {
        match enum_definition.variants[0].node.kind {
            ast::tuple_variant_kind(args) if args.len() == 1 => {}
            _ => newtype = false
        }
    }
    if newtype {
        ibox(s, indent_unit);
        word_space(s, visibility_qualified(visibility, ~"enum"));
    } else {
        head(s, visibility_qualified(visibility, ~"enum"));
    }

    print_ident(s, ident);
    print_type_params(s, params);
    space(s.s);
    if newtype {
        word_space(s, ~"=");
        match enum_definition.variants[0].node.kind {
            ast::tuple_variant_kind(args) => print_type(s, args[0].ty),
            _ => fail ~"newtype syntax with struct?"
        }
        word(s.s, ~";");
        end(s);
    } else {
        print_variants(s, enum_definition.variants, span);
    }
}

fn print_variants(s: ps, variants: ~[ast::variant], span: ast::span) {
    bopen(s);
    for variants.each |v| {
        space_if_not_bol(s);
        maybe_print_comment(s, v.span.lo);
        print_outer_attributes(s, v.node.attrs);
        ibox(s, indent_unit);
        print_variant(s, *v);
        word(s.s, ~",");
        end(s);
        maybe_print_trailing_comment(s, v.span, None);
    }
    bclose(s, span);
}

fn visibility_to_str(vis: ast::visibility) -> ~str {
    match vis {
        ast::private => ~"priv",
        ast::public => ~"pub",
        ast::inherited => ~""
    }
}

fn visibility_qualified(vis: ast::visibility, s: ~str) -> ~str {
    match vis {
        ast::private | ast::public =>
        visibility_to_str(vis) + " " + s,
        ast::inherited => s
    }
}

fn print_visibility(s: ps, vis: ast::visibility) {
    match vis {
        ast::private | ast::public =>
        word_nbsp(s, visibility_to_str(vis)),
        ast::inherited => ()
    }
}

fn print_struct(s: ps, struct_def: @ast::struct_def, tps: ~[ast::ty_param],
                ident: ast::ident, span: ast::span) {
    print_ident(s, ident);
    nbsp(s);
    print_type_params(s, tps);
    if ast_util::struct_def_is_tuple_like(struct_def) {
        popen(s);
        let mut first = true;
        for struct_def.fields.each |field| {
            if first {
                first = false;
            } else {
                word_space(s, ~",");
            }

            match field.node.kind {
                ast::named_field(*) => fail ~"unexpected named field",
                ast::unnamed_field => {
                    maybe_print_comment(s, field.span.lo);
                    print_type(s, field.node.ty);
                }
            }
        }
        pclose(s);
        word(s.s, ~";");
        end(s); // close the outer-box
    } else {
        bopen(s);
        hardbreak_if_not_bol(s);
        do struct_def.dtor.iter |dtor| {
          hardbreak_if_not_bol(s);
          maybe_print_comment(s, dtor.span.lo);
          print_outer_attributes(s, dtor.node.attrs);
          head(s, ~"drop");
          print_block(s, dtor.node.body);
        }

        for struct_def.fields.each |field| {
            match field.node.kind {
                ast::unnamed_field => fail ~"unexpected unnamed field",
                ast::named_field(ident, mutability, visibility) => {
                    hardbreak_if_not_bol(s);
                    maybe_print_comment(s, field.span.lo);
                    print_visibility(s, visibility);
                    if mutability == ast::struct_mutable {
                        word_nbsp(s, ~"mut");
                    }
                    print_ident(s, ident);
                    word_nbsp(s, ~":");
                    print_type(s, field.node.ty);
                    word(s.s, ~",");
                }
            }
        }

        bclose(s, span);
    }
}

/// This doesn't deserve to be called "pretty" printing, but it should be
/// meaning-preserving. A quick hack that might help would be to look at the
/// spans embedded in the TTs to decide where to put spaces and newlines.
/// But it'd be better to parse these according to the grammar of the
/// appropriate macro, transcribe back into the grammar we just parsed from,
/// and then pretty-print the resulting AST nodes (so, e.g., we print
/// expression arguments as expressions). It can be done! I think.
fn print_tt(s: ps, tt: ast::token_tree) {
    match tt {
      ast::tt_delim(ref tts) => print_tts(s, *tts),
      ast::tt_tok(_, ref tk) => {
          word(s.s, parse::token::to_str(s.intr, (*tk)));
      }
      ast::tt_seq(_, ref tts, ref sep, zerok) => {
        word(s.s, ~"$(");
        for (*tts).each() |tt_elt| { print_tt(s, *tt_elt); }
        word(s.s, ~")");
        match (*sep) {
          Some(ref tk) => word(s.s, parse::token::to_str(s.intr, (*tk))),
          None => ()
        }
        word(s.s, if zerok { ~"*" } else { ~"+" });
      }
      ast::tt_nonterminal(_, name) => {
        word(s.s, ~"$");
        print_ident(s, name);
      }
    }
}

fn print_tts(s: ps, &&tts: &[ast::token_tree]) {
    ibox(s, 0);
    for tts.eachi |i, tt| {
        if i != 0 {
            space(s.s);
        }
        print_tt(s, *tt);
    }
    end(s);
}

fn print_variant(s: ps, v: ast::variant) {
    print_visibility(s, v.node.vis);
    match v.node.kind {
        ast::tuple_variant_kind(args) => {
            print_ident(s, v.node.name);
            if args.is_not_empty() {
                popen(s);
                fn print_variant_arg(s: ps, arg: ast::variant_arg) {
                    print_type(s, arg.ty);
                }
                commasep(s, consistent, args, print_variant_arg);
                pclose(s);
            }
        }
        ast::struct_variant_kind(struct_def) => {
            head(s, ~"");
            print_struct(s, struct_def, ~[], v.node.name, v.span);
        }
        ast::enum_variant_kind(ref enum_definition) => {
            print_variants(s, (*enum_definition).variants, v.span);
        }
    }
    match v.node.disr_expr {
      Some(d) => {
        space(s.s);
        word_space(s, ~"=");
        print_expr(s, d);
      }
      _ => ()
    }
}

fn print_ty_method(s: ps, m: ast::ty_method) {
    hardbreak_if_not_bol(s);
    maybe_print_comment(s, m.span.lo);
    print_outer_attributes(s, m.attrs);
    print_ty_fn(s, None, None, m.purity, ast::Many,
                @~[], m.decl, Some(m.ident), Some(m.tps),
                Some(m.self_ty.node));
    word(s.s, ~";");
}

fn print_trait_method(s: ps, m: ast::trait_method) {
    match m {
      required(ref ty_m) => print_ty_method(s, (*ty_m)),
      provided(m)    => print_method(s, m)
    }
}

fn print_method(s: ps, meth: @ast::method) {
    hardbreak_if_not_bol(s);
    maybe_print_comment(s, meth.span.lo);
    print_outer_attributes(s, meth.attrs);
    print_fn(s, meth.decl, Some(meth.purity),
             meth.ident, meth.tps, Some(meth.self_ty.node),
             meth.vis);
    word(s.s, ~" ");
    print_block_with_attrs(s, meth.body, meth.attrs);
}

fn print_outer_attributes(s: ps, attrs: ~[ast::attribute]) {
    let mut count = 0;
    for attrs.each |attr| {
        match attr.node.style {
          ast::attr_outer => { print_attribute(s, *attr); count += 1; }
          _ => {/* fallthrough */ }
        }
    }
    if count > 0 { hardbreak_if_not_bol(s); }
}

fn print_inner_attributes(s: ps, attrs: ~[ast::attribute]) {
    let mut count = 0;
    for attrs.each |attr| {
        match attr.node.style {
          ast::attr_inner => {
            print_attribute(s, *attr);
            if !attr.node.is_sugared_doc {
                word(s.s, ~";");
            }
            count += 1;
          }
          _ => {/* fallthrough */ }
        }
    }
    if count > 0 { hardbreak_if_not_bol(s); }
}

fn print_attribute(s: ps, attr: ast::attribute) {
    hardbreak_if_not_bol(s);
    maybe_print_comment(s, attr.span.lo);
    if attr.node.is_sugared_doc {
        let meta = attr::attr_meta(attr);
        let comment = attr::get_meta_item_value_str(meta).get();
        word(s.s, comment);
    } else {
        word(s.s, ~"#[");
        print_meta_item(s, @attr.node.value);
        word(s.s, ~"]");
    }
}


fn print_stmt(s: ps, st: ast::stmt) {
    maybe_print_comment(s, st.span.lo);
    match st.node {
      ast::stmt_decl(decl, _) => {
        print_decl(s, decl);
      }
      ast::stmt_expr(expr, _) => {
        space_if_not_bol(s);
        print_expr(s, expr);
      }
      ast::stmt_semi(expr, _) => {
        space_if_not_bol(s);
        print_expr(s, expr);
        word(s.s, ~";");
      }
      ast::stmt_mac(ref mac, semi) => {
        space_if_not_bol(s);
        print_mac(s, (*mac));
        if semi { word(s.s, ~";"); }
      }
    }
    if parse::classify::stmt_ends_with_semi(st) { word(s.s, ~";"); }
    maybe_print_trailing_comment(s, st.span, None);
}

fn print_block(s: ps, blk: ast::blk) {
    print_possibly_embedded_block(s, blk, block_normal, indent_unit);
}

fn print_block_unclosed(s: ps, blk: ast::blk) {
    print_possibly_embedded_block_(s, blk, block_normal, indent_unit, ~[],
                                 false);
}

fn print_block_unclosed_indent(s: ps, blk: ast::blk, indented: uint) {
    print_possibly_embedded_block_(s, blk, block_normal, indented, ~[],
                                   false);
}

fn print_block_with_attrs(s: ps, blk: ast::blk, attrs: ~[ast::attribute]) {
    print_possibly_embedded_block_(s, blk, block_normal, indent_unit, attrs,
                                  true);
}

enum embed_type { block_block_fn, block_normal, }

fn print_possibly_embedded_block(s: ps, blk: ast::blk, embedded: embed_type,
                                 indented: uint) {
    print_possibly_embedded_block_(
        s, blk, embedded, indented, ~[], true);
}

fn print_possibly_embedded_block_(s: ps, blk: ast::blk, embedded: embed_type,
                                  indented: uint, attrs: ~[ast::attribute],
                                  close_box: bool) {
    match blk.node.rules {
      ast::unsafe_blk => word_space(s, ~"unsafe"),
      ast::default_blk => ()
    }
    maybe_print_comment(s, blk.span.lo);
    let ann_node = node_block(s, blk);
    (s.ann.pre)(ann_node);
    match embedded {
      block_block_fn => end(s),
      block_normal => bopen(s)
    }

    print_inner_attributes(s, attrs);

    for blk.node.view_items.each |vi| { print_view_item(s, *vi); }
    for blk.node.stmts.each |st| {
        print_stmt(s, **st);
    }
    match blk.node.expr {
      Some(expr) => {
        space_if_not_bol(s);
        print_expr(s, expr);
        maybe_print_trailing_comment(s, expr.span, Some(blk.span.hi));
      }
      _ => ()
    }
    bclose_maybe_open(s, blk.span, indented, close_box);
    (s.ann.post)(ann_node);
}

fn print_if(s: ps, test: @ast::expr, blk: ast::blk,
            elseopt: Option<@ast::expr>, chk: bool) {
    head(s, ~"if");
    if chk { word_nbsp(s, ~"check"); }
    print_expr(s, test);
    space(s.s);
    print_block(s, blk);
    fn do_else(s: ps, els: Option<@ast::expr>) {
        match els {
          Some(_else) => {
            match _else.node {
              // "another else-if"
              ast::expr_if(i, ref t, e) => {
                cbox(s, indent_unit - 1u);
                ibox(s, 0u);
                word(s.s, ~" else if ");
                print_expr(s, i);
                space(s.s);
                print_block(s, (*t));
                do_else(s, e);
              }
              // "final else"
              ast::expr_block(ref b) => {
                cbox(s, indent_unit - 1u);
                ibox(s, 0u);
                word(s.s, ~" else ");
                print_block(s, (*b));
              }
              // BLEAH, constraints would be great here
              _ => {
                  fail ~"print_if saw if with weird alternative";
              }
            }
          }
          _ => {/* fall through */ }
        }
    }
    do_else(s, elseopt);
}

fn print_mac(s: ps, m: ast::mac) {
    match m.node {
      ast::mac_invoc_tt(pth, ref tts) => {
        print_path(s, pth, false);
        word(s.s, ~"!");
        popen(s);
        print_tts(s, *tts);
        pclose(s);
      }
    }
}

fn print_vstore(s: ps, t: ast::vstore) {
    match t {
        ast::vstore_fixed(Some(i)) => word(s.s, fmt!("%u", i)),
        ast::vstore_fixed(None) => word(s.s, ~"_"),
        ast::vstore_uniq => word(s.s, ~"~"),
        ast::vstore_box => word(s.s, ~"@"),
        ast::vstore_slice(r) => print_region(s, ~"&", r, ~"/")
    }
}

fn print_expr_vstore(s: ps, t: ast::expr_vstore) {
    match t {
      ast::expr_vstore_fixed(Some(i)) => word(s.s, fmt!("%u", i)),
      ast::expr_vstore_fixed(None) => word(s.s, ~"_"),
      ast::expr_vstore_uniq => word(s.s, ~"~"),
      ast::expr_vstore_box => word(s.s, ~"@"),
      ast::expr_vstore_mut_box => {
        word(s.s, ~"@");
        word(s.s, ~"mut");
      }
      ast::expr_vstore_slice => word(s.s, ~"&"),
      ast::expr_vstore_mut_slice => {
        word(s.s, ~"&");
        word(s.s, ~"mut");
      }
    }
}

fn print_call_pre(s: ps,
                  has_block: bool,
                  base_args: &mut ~[@ast::expr])
               -> Option<@ast::expr> {
    if has_block {
        let blk_arg = base_args.pop();
        match blk_arg.node {
          ast::expr_loop_body(_) => { head(s, ~"for"); }
          ast::expr_do_body(_) => { head(s, ~"do"); }
          _ => {}
        }
        Some(blk_arg)
    } else {
        None
    }
}

fn print_call_post(s: ps,
                   has_block: bool,
                   blk: &Option<@ast::expr>,
                   base_args: &mut ~[@ast::expr]) {
    if !has_block || base_args.is_not_empty() {
        popen(s);
        commasep_exprs(s, inconsistent, *base_args);
        pclose(s);
    }
    if has_block {
        nbsp(s);
        match blk.get().node {
          // need to handle closures specifically
          ast::expr_do_body(e) | ast::expr_loop_body(e) => {
            end(s); // we close our head box; closure
                    // will create it's own.
            print_expr(s, e);
            end(s); // close outer box, as closures don't
          }
          _ => {
            // not sure if this can happen.
            print_expr(s, blk.get());
          }
        }
    }
}

fn print_expr(s: ps, &&expr: @ast::expr) {
    fn print_field(s: ps, field: ast::field) {
        ibox(s, indent_unit);
        if field.node.mutbl == ast::m_mutbl { word_nbsp(s, ~"mut"); }
        print_ident(s, field.node.ident);
        word_space(s, ~":");
        print_expr(s, field.node.expr);
        end(s);
    }
    fn get_span(field: ast::field) -> codemap::span { return field.span; }

    maybe_print_comment(s, expr.span.lo);
    ibox(s, indent_unit);
    let ann_node = node_expr(s, expr);
    (s.ann.pre)(ann_node);
    match expr.node {
        ast::expr_vstore(e, v) => match v {
            ast::expr_vstore_fixed(_) => {
                print_expr(s, e);
                word(s.s, ~"/");
                print_expr_vstore(s, v);
            }
            _ => {
                print_expr_vstore(s, v);
                print_expr(s, e);
            }
        },
      ast::expr_vec(exprs, mutbl) => {
        ibox(s, indent_unit);
        word(s.s, ~"[");
        if mutbl == ast::m_mutbl {
            word(s.s, ~"mut");
            if vec::len(exprs) > 0u { nbsp(s); }
        }
        commasep_exprs(s, inconsistent, exprs);
        word(s.s, ~"]");
        end(s);
      }

      ast::expr_repeat(element, count, mutbl) => {
        ibox(s, indent_unit);
        word(s.s, ~"[");
        if mutbl == ast::m_mutbl {
            word(s.s, ~"mut");
            nbsp(s);
        }
        print_expr(s, element);
        word(s.s, ~",");
        word(s.s, ~"..");
        print_expr(s, count);
        word(s.s, ~"]");
        end(s);
      }

      ast::expr_rec(ref fields, wth) => {
        word(s.s, ~"{");
        commasep_cmnt(s, consistent, (*fields), print_field, get_span);
        match wth {
          Some(expr) => {
            ibox(s, indent_unit);
            word(s.s, ~",");
            space(s.s);
            word(s.s, ~"..");
            print_expr(s, expr);
            end(s);
          }
          _ => word(s.s, ~",")
        }
        word(s.s, ~"}");
      }
      ast::expr_struct(path, ref fields, wth) => {
        print_path(s, path, true);
        word(s.s, ~"{");
        commasep_cmnt(s, consistent, (*fields), print_field, get_span);
        match wth {
            Some(expr) => {
                if vec::len((*fields)) > 0u { space(s.s); }
                ibox(s, indent_unit);
                word(s.s, ~",");
                space(s.s);
                word(s.s, ~"..");
                print_expr(s, expr);
                end(s);
            }
            _ => word(s.s, ~",")
        }
        word(s.s, ~"}");
      }
      ast::expr_tup(exprs) => {
        popen(s);
        commasep_exprs(s, inconsistent, exprs);
        pclose(s);
      }
      ast::expr_call(func, args, has_block) => {
        let mut base_args = copy args;
        let blk = print_call_pre(s, has_block, &mut base_args);
        print_expr(s, func);
        print_call_post(s, has_block, &blk, &mut base_args);
      }
      ast::expr_method_call(func, ident, tys, args, has_block) => {
        let mut base_args = copy args;
        let blk = print_call_pre(s, has_block, &mut base_args);
        print_expr(s, func);
        word(s.s, ~".");
        print_ident(s, ident);
        if vec::len(tys) > 0u {
            word(s.s, ~"::<");
            commasep(s, inconsistent, tys, print_type);
            word(s.s, ~">");
        }
        print_call_post(s, has_block, &blk, &mut base_args);
      }
      ast::expr_binary(op, lhs, rhs) => {
        print_expr(s, lhs);
        space(s.s);
        word_space(s, ast_util::binop_to_str(op));
        print_expr(s, rhs);
      }
      ast::expr_unary(op, expr) => {
        word(s.s, ast_util::unop_to_str(op));
        print_expr(s, expr);
      }
      ast::expr_addr_of(m, expr) => {
        word(s.s, ~"&");
        print_mutability(s, m);
        print_expr(s, expr);
      }
      ast::expr_lit(lit) => print_literal(s, lit),
      ast::expr_cast(expr, ty) => {
        print_expr(s, expr);
        space(s.s);
        word_space(s, ~"as");
        print_type_ex(s, ty, true);
      }
      ast::expr_if(test, ref blk, elseopt) => {
        print_if(s, test, (*blk), elseopt, false);
      }
      ast::expr_while(test, ref blk) => {
        head(s, ~"while");
        print_expr(s, test);
        space(s.s);
        print_block(s, (*blk));
      }
      ast::expr_loop(ref blk, opt_ident) => {
        head(s, ~"loop");
        space(s.s);
        opt_ident.iter(|ident| {
            print_ident(s, *ident);
            word_space(s, ~":");
        });
        print_block(s, (*blk));
      }
      ast::expr_match(expr, ref arms) => {
        cbox(s, match_indent_unit);
        ibox(s, 4);
        word_nbsp(s, ~"match");
        print_expr(s, expr);
        space(s.s);
        bopen(s);
        let len = (*arms).len();
        for (*arms).eachi |i, arm| {
            space(s.s);
            cbox(s, match_indent_unit);
            ibox(s, 0u);
            let mut first = true;
            for arm.pats.each |p| {
                if first {
                    first = false;
                } else { space(s.s); word_space(s, ~"|"); }
                print_refutable_pat(s, *p);
            }
            space(s.s);
            match arm.guard {
              Some(e) => {
                word_space(s, ~"if");
                print_expr(s, e);
                space(s.s);
              }
              None => ()
            }
            word_space(s, ~"=>");

            // Extract the expression from the extra block the parser adds
            // in the case of foo => expr
            if arm.body.node.view_items.is_empty() &&
                arm.body.node.stmts.is_empty() &&
                arm.body.node.rules == ast::default_blk &&
                arm.body.node.expr.is_some()
            {
                match arm.body.node.expr {
                    Some(expr) => {
                        match expr.node {
                            ast::expr_block(ref blk) => {
                                // the block will close the pattern's ibox
                                print_block_unclosed_indent(
                                    s, (*blk), match_indent_unit);
                            }
                            _ => {
                                end(s); // close the ibox for the pattern
                                print_expr(s, expr);
                            }
                        }
                        if !expr_is_simple_block(expr)
                            && i < len - 1 {
                            word(s.s, ~",");
                        }
                        end(s); // close enclosing cbox
                    }
                    None => fail
                }
            } else {
                // the block will close the pattern's ibox
                print_block_unclosed_indent(s, arm.body, match_indent_unit);
            }
        }
        bclose_(s, expr.span, match_indent_unit);
      }
      ast::expr_fn(proto, decl, ref body, cap_clause) => {
        // containing cbox, will be closed by print-block at }
        cbox(s, indent_unit);
        // head-box, will be closed by print-block at start
        ibox(s, 0u);
        print_fn_header_info(s, None, None, ast::Many,
                             Some(proto), ast::inherited);
        print_fn_args_and_ret(s, decl, *cap_clause, None);
        space(s.s);
        print_block(s, (*body));
      }
      ast::expr_fn_block(decl, ref body, cap_clause) => {
        // in do/for blocks we don't want to show an empty
        // argument list, but at this point we don't know which
        // we are inside.
        //
        // if !decl.inputs.is_empty() {
        print_fn_block_args(s, decl, *cap_clause);
        space(s.s);
        // }
        assert (*body).node.stmts.is_empty();
        assert (*body).node.expr.is_some();
        // we extract the block, so as not to create another set of boxes
        match (*body).node.expr.get().node {
            ast::expr_block(ref blk) => {
                print_block_unclosed(s, (*blk));
            }
            _ => {
                // this is a bare expression
                print_expr(s, (*body).node.expr.get());
                end(s); // need to close a box
            }
        }
        // a box will be closed by print_expr, but we didn't want an overall
        // wrapper so we closed the corresponding opening. so create an
        // empty box to satisfy the close.
        ibox(s, 0);
      }
      ast::expr_loop_body(body) => {
        print_expr(s, body);
      }
      ast::expr_do_body(body) => {
        print_expr(s, body);
      }
      ast::expr_block(ref blk) => {
        // containing cbox, will be closed by print-block at }
        cbox(s, indent_unit);
        // head-box, will be closed by print-block after {
        ibox(s, 0u);
        print_block(s, (*blk));
      }
      ast::expr_copy(e) => { word_space(s, ~"copy"); print_expr(s, e); }
      ast::expr_unary_move(e) => {
          word_space(s, ~"move");
          print_expr(s, e);
      }
      ast::expr_assign(lhs, rhs) => {
        print_expr(s, lhs);
        space(s.s);
        word_space(s, ~"=");
        print_expr(s, rhs);
      }
      ast::expr_swap(lhs, rhs) => {
        print_expr(s, lhs);
        space(s.s);
        word_space(s, ~"<->");
        print_expr(s, rhs);
      }
      ast::expr_assign_op(op, lhs, rhs) => {
        print_expr(s, lhs);
        space(s.s);
        word(s.s, ast_util::binop_to_str(op));
        word_space(s, ~"=");
        print_expr(s, rhs);
      }
      ast::expr_field(expr, id, tys) => {
        print_expr(s, expr);
        word(s.s, ~".");
        print_ident(s, id);
        if vec::len(tys) > 0u {
            word(s.s, ~"::<");
            commasep(s, inconsistent, tys, print_type);
            word(s.s, ~">");
        }
      }
      ast::expr_index(expr, index) => {
        print_expr(s, expr);
        word(s.s, ~"[");
        print_expr(s, index);
        word(s.s, ~"]");
      }
      ast::expr_path(path) => print_path(s, path, true),
      ast::expr_fail(maybe_fail_val) => {
        word(s.s, ~"fail");
        match maybe_fail_val {
          Some(expr) => { word(s.s, ~" "); print_expr(s, expr); }
          _ => ()
        }
      }
      ast::expr_break(opt_ident) => {
        word(s.s, ~"break");
        space(s.s);
        opt_ident.iter(|ident| {print_ident(s, *ident); space(s.s)});
      }
      ast::expr_again(opt_ident) => {
        word(s.s, ~"loop");
        space(s.s);
        opt_ident.iter(|ident| {print_ident(s, *ident); space(s.s)});
      }
      ast::expr_ret(result) => {
        word(s.s, ~"return");
        match result {
          Some(expr) => { word(s.s, ~" "); print_expr(s, expr); }
          _ => ()
        }
      }
      ast::expr_log(lvl, lexp, expr) => {
        match lvl {
          ast::debug => { word_nbsp(s, ~"log"); print_expr(s, expr); }
          ast::error => { word_nbsp(s, ~"log_err"); print_expr(s, expr); }
          ast::log_other => {
            word_nbsp(s, ~"log");
            popen(s);
            print_expr(s, lexp);
            word(s.s, ~",");
            space_if_not_bol(s);
            print_expr(s, expr);
            pclose(s);
          }
        }
      }
      ast::expr_assert(expr) => {
        word_nbsp(s, ~"assert");
        print_expr(s, expr);
      }
      ast::expr_mac(ref m) => print_mac(s, (*m)),
      ast::expr_paren(e) => {
          popen(s);
          print_expr(s, e);
          pclose(s);
      }
    }
    (s.ann.post)(ann_node);
    end(s);
}

fn print_local_decl(s: ps, loc: @ast::local) {
    print_irrefutable_pat(s, loc.node.pat);
    match loc.node.ty.node {
      ast::ty_infer => (),
      _ => { word_space(s, ~":"); print_type(s, loc.node.ty); }
    }
}

fn print_decl(s: ps, decl: @ast::decl) {
    maybe_print_comment(s, decl.span.lo);
    match decl.node {
      ast::decl_local(locs) => {
        space_if_not_bol(s);
        ibox(s, indent_unit);
        word_nbsp(s, ~"let");

        // if any are mut, all are mut
        if vec::any(locs, |l| l.node.is_mutbl) {
            assert vec::all(locs, |l| l.node.is_mutbl);
            word_nbsp(s, ~"mut");
        }

        fn print_local(s: ps, &&loc: @ast::local) {
            ibox(s, indent_unit);
            print_local_decl(s, loc);
            end(s);
            match loc.node.init {
              Some(init) => {
                nbsp(s);
                word_space(s, ~"=");
                print_expr(s, init);
              }
              _ => ()
            }
        }
        commasep(s, consistent, locs, print_local);
        end(s);
      }
      ast::decl_item(item) => print_item(s, item)
    }
}

fn print_ident(s: ps, ident: ast::ident) { word(s.s, *s.intr.get(ident)); }

fn print_for_decl(s: ps, loc: @ast::local, coll: @ast::expr) {
    print_local_decl(s, loc);
    space(s.s);
    word_space(s, ~"in");
    print_expr(s, coll);
}

fn print_path(s: ps, &&path: @ast::path, colons_before_params: bool) {
    maybe_print_comment(s, path.span.lo);
    if path.global { word(s.s, ~"::"); }
    let mut first = true;
    for path.idents.each |id| {
        if first { first = false; } else { word(s.s, ~"::"); }
        print_ident(s, *id);
    }
    if path.rp.is_some() || !path.types.is_empty() {
        if colons_before_params { word(s.s, ~"::"); }

        match path.rp {
          None => { /* ok */ }
          Some(r) => {
            word(s.s, ~"/");
            print_region(s, ~"&", r, ~"");
          }
        }

        if !path.types.is_empty() {
            word(s.s, ~"<");
            commasep(s, inconsistent, path.types, print_type);
            word(s.s, ~">");
        }
    }
}

fn print_irrefutable_pat(s: ps, &&pat: @ast::pat) {
    print_pat(s, pat, false)
}

fn print_refutable_pat(s: ps, &&pat: @ast::pat) {
    print_pat(s, pat, true)
}

fn print_pat(s: ps, &&pat: @ast::pat, refutable: bool) {
    maybe_print_comment(s, pat.span.lo);
    let ann_node = node_pat(s, pat);
    (s.ann.pre)(ann_node);
    /* Pat isn't normalized, but the beauty of it
     is that it doesn't matter */
    match pat.node {
      ast::pat_wild => word(s.s, ~"_"),
      ast::pat_ident(binding_mode, path, sub) => {
          if refutable {
              match binding_mode {
                  ast::bind_by_ref(mutbl) => {
                      word_nbsp(s, ~"ref");
                      print_mutability(s, mutbl);
                  }
                  ast::bind_by_move => {
                      word_nbsp(s, ~"move");
                  }
                  ast::bind_by_value => {
                      word_nbsp(s, ~"copy");
                  }
                  ast::bind_infer => {}
              }
          }
          print_path(s, path, true);
          match sub {
              Some(p) => {
                  word(s.s, ~"@");
                  print_pat(s, p, refutable);
              }
              None => ()
          }
      }
      ast::pat_enum(path, args_) => {
        print_path(s, path, true);
        match args_ {
          None => word(s.s, ~"(*)"),
          Some(args) => {
            if args.is_not_empty() {
              popen(s);
              commasep(s, inconsistent, args,
                       |s, p| print_pat(s, p, refutable));
              pclose(s);
            } else { }
          }
        }
      }
      ast::pat_rec(fields, etc) => {
        word(s.s, ~"{");
        fn print_field(s: ps, f: ast::field_pat, refutable: bool) {
            cbox(s, indent_unit);
            print_ident(s, f.ident);
            word_space(s, ~":");
            print_pat(s, f.pat, refutable);
            end(s);
        }
        fn get_span(f: ast::field_pat) -> codemap::span { return f.pat.span; }
        commasep_cmnt(s, consistent, fields,
                      |s, f| print_field(s, f, refutable),
                      get_span);
        if etc {
            if vec::len(fields) != 0u { word_space(s, ~","); }
            word(s.s, ~"_");
        }
        word(s.s, ~"}");
      }
      ast::pat_struct(path, fields, etc) => {
        print_path(s, path, true);
        word(s.s, ~"{");
        fn print_field(s: ps, f: ast::field_pat, refutable: bool) {
            cbox(s, indent_unit);
            print_ident(s, f.ident);
            word_space(s, ~":");
            print_pat(s, f.pat, refutable);
            end(s);
        }
        fn get_span(f: ast::field_pat) -> codemap::span { return f.pat.span; }
        commasep_cmnt(s, consistent, fields,
                      |s, f| print_field(s,f,refutable),
                      get_span);
        if etc {
            if vec::len(fields) != 0u { word_space(s, ~","); }
            word(s.s, ~"_");
        }
        word(s.s, ~"}");
      }
      ast::pat_tup(elts) => {
        popen(s);
        commasep(s, inconsistent, elts, |s, p| print_pat(s, p, refutable));
        pclose(s);
      }
      ast::pat_box(inner) => {
          word(s.s, ~"@");
          print_pat(s, inner, refutable);
      }
      ast::pat_uniq(inner) => {
          word(s.s, ~"~");
          print_pat(s, inner, refutable);
      }
      ast::pat_region(inner) => {
          word(s.s, ~"&");
          print_pat(s, inner, refutable);
      }
      ast::pat_lit(e) => print_expr(s, e),
      ast::pat_range(begin, end) => {
        print_expr(s, begin);
        space(s.s);
        word(s.s, ~"..");
        print_expr(s, end);
      }
      ast::pat_vec(elts, tail) => {
        word(s.s, ~"[");
        commasep(s, inconsistent, elts, |s, p| print_pat(s, p, refutable));
        do option::iter(&tail) |tail| {
            if vec::len(elts) != 0u { word_space(s, ~","); }
            word(s.s, ~"..");
            print_pat(s, *tail, refutable);
        }
        word(s.s, ~"]");
      }
    }
    (s.ann.post)(ann_node);
}

// Returns whether it printed anything
fn print_self_ty(s: ps, self_ty: ast::self_ty_) -> bool {
    match self_ty {
      ast::sty_static | ast::sty_by_ref => { return false; }
      ast::sty_value => { word(s.s, ~"self"); }
      ast::sty_region(m) => {
        word(s.s, ~"&"); print_mutability(s, m); word(s.s, ~"self");
      }
      ast::sty_box(m) => {
        word(s.s, ~"@"); print_mutability(s, m); word(s.s, ~"self");
      }
      ast::sty_uniq(m) => {
        word(s.s, ~"~"); print_mutability(s, m); word(s.s, ~"self");
      }
    }
    return true;
}

fn print_fn(s: ps,
            decl: ast::fn_decl,
            purity: Option<ast::purity>,
            name: ast::ident,
            typarams: ~[ast::ty_param],
            opt_self_ty: Option<ast::self_ty_>,
            vis: ast::visibility) {
    head(s, ~"");
    print_fn_header_info(s, opt_self_ty, purity, ast::Many, None, vis);
    nbsp(s);
    print_ident(s, name);
    print_type_params(s, typarams);
    print_fn_args_and_ret(s, decl, ~[], opt_self_ty);
}

fn print_fn_args(s: ps, decl: ast::fn_decl,
                 cap_items: ~[ast::capture_item],
                 opt_self_ty: Option<ast::self_ty_>) {
    // It is unfortunate to duplicate the commasep logic, but we
    // we want the self type, the args, and the capture clauses all
    // in the same box.
    box(s, 0, inconsistent);
    let mut first = true;
    for opt_self_ty.each |self_ty| {
        first = !print_self_ty(s, *self_ty);
    }

    for decl.inputs.each |arg| {
        if first { first = false; } else { word_space(s, ~","); }
        print_arg(s, *arg);
    }

    for cap_items.each |cap_item| {
        if first { first = false; } else { word_space(s, ~","); }
        if cap_item.is_move { word_nbsp(s, ~"move") }
        else { word_nbsp(s, ~"copy") }
        print_ident(s, cap_item.name);
    }

    end(s);
}

fn print_fn_args_and_ret(s: ps, decl: ast::fn_decl,
                         cap_items: ~[ast::capture_item],
                         opt_self_ty: Option<ast::self_ty_>) {
    popen(s);
    print_fn_args(s, decl, cap_items, opt_self_ty);
    pclose(s);

    maybe_print_comment(s, decl.output.span.lo);
    match decl.output.node {
        ast::ty_nil => {}
        _ => {
            space_if_not_bol(s);
            word_space(s, ~"->");
            print_type(s, decl.output);
        }
    }
}

fn print_fn_block_args(s: ps, decl: ast::fn_decl,
                       cap_items: ~[ast::capture_item]) {
    word(s.s, ~"|");
    print_fn_args(s, decl, cap_items, None);
    word(s.s, ~"|");

    match decl.output.node {
        ast::ty_infer => {}
        _ => {
            space_if_not_bol(s);
            word_space(s, ~"->");
            print_type(s, decl.output);
        }
    }

    maybe_print_comment(s, decl.output.span.lo);
}

fn mode_to_str(m: ast::mode) -> ~str {
    match m {
      ast::expl(ast::by_move) => ~"-",
      ast::expl(ast::by_ref) => ~"&&",
      ast::expl(ast::by_val) => ~"++",
      ast::expl(ast::by_copy) => ~"+",
      ast::infer(_) => ~""
    }
}

fn print_arg_mode(s: ps, m: ast::mode) {
    let ms = mode_to_str(m);
    if ms != ~"" { word(s.s, ms); }
}

fn print_bounds(s: ps, bounds: @~[ast::ty_param_bound]) {
    if bounds.is_not_empty() {
        word(s.s, ~":");
        let mut first = true;
        for vec::each(*bounds) |&bound| {
            nbsp(s);
            if first {
                first = false;
            } else {
                word_space(s, ~"+");
            }

            match bound {
                TraitTyParamBound(ty) => print_type(s, ty),
                RegionTyParamBound => word(s.s, ~"&static"),
            }
        }
    }
}

fn print_type_params(s: ps, &&params: ~[ast::ty_param]) {
    if vec::len(params) > 0u {
        word(s.s, ~"<");
        fn printParam(s: ps, param: ast::ty_param) {
            print_ident(s, param.ident);
            print_bounds(s, param.bounds);
        }
        commasep(s, inconsistent, params, printParam);
        word(s.s, ~">");
    }
}

fn print_meta_item(s: ps, &&item: @ast::meta_item) {
    ibox(s, indent_unit);
    match item.node {
      ast::meta_word(ref name) => word(s.s, (*name)),
      ast::meta_name_value(ref name, value) => {
        word_space(s, (*name));
        word_space(s, ~"=");
        print_literal(s, @value);
      }
      ast::meta_list(ref name, items) => {
        word(s.s, (*name));
        popen(s);
        commasep(s, consistent, items, print_meta_item);
        pclose(s);
      }
    }
    end(s);
}

fn print_view_path(s: ps, &&vp: @ast::view_path) {
    match vp.node {
      ast::view_path_simple(ident, path, namespace, _) => {
        if namespace == ast::module_ns {
            word_space(s, ~"mod");
        }
        if path.idents[vec::len(path.idents)-1u] != ident {
            print_ident(s, ident);
            space(s.s);
            word_space(s, ~"=");
        }
        print_path(s, path, false);
      }

      ast::view_path_glob(path, _) => {
        print_path(s, path, false);
        word(s.s, ~"::*");
      }

      ast::view_path_list(path, ref idents, _) => {
        print_path(s, path, false);
        word(s.s, ~"::{");
        do commasep(s, inconsistent, (*idents)) |s, w| {
            print_ident(s, w.node.name);
        }
        word(s.s, ~"}");
      }
    }
}

fn print_view_paths(s: ps, vps: ~[@ast::view_path]) {
    commasep(s, inconsistent, vps, print_view_path);
}

fn print_view_item(s: ps, item: @ast::view_item) {
    hardbreak_if_not_bol(s);
    maybe_print_comment(s, item.span.lo);
    print_outer_attributes(s, item.attrs);
    print_visibility(s, item.vis);
    match item.node {
      ast::view_item_use(id, mta, _) => {
        head(s, ~"extern mod");
        print_ident(s, id);
        if mta.is_not_empty() {
            popen(s);
            commasep(s, consistent, mta, print_meta_item);
            pclose(s);
        }
      }

      ast::view_item_import(vps) => {
        head(s, ~"use");
        print_view_paths(s, vps);
      }

      ast::view_item_export(vps) => {
        head(s, ~"export");
        print_view_paths(s, vps);
      }
    }
    word(s.s, ~";");
    end(s); // end inner head-block
    end(s); // end outer head-block
}

fn print_mutability(s: ps, mutbl: ast::mutability) {
    match mutbl {
      ast::m_mutbl => word_nbsp(s, ~"mut"),
      ast::m_const => word_nbsp(s, ~"const"),
      ast::m_imm => {/* nothing */ }
    }
}

fn print_mt(s: ps, mt: ast::mt) {
    print_mutability(s, mt.mutbl);
    print_type(s, mt.ty);
}

fn print_arg(s: ps, input: ast::arg) {
    ibox(s, indent_unit);
    print_arg_mode(s, input.mode);
    if input.is_mutbl {
        word_space(s, ~"mut");
    }
    match input.ty.node {
      ast::ty_infer => print_irrefutable_pat(s, input.pat),
      _ => {
        match input.pat.node {
            ast::pat_ident(_, path, _) if
                path.idents.len() == 1 &&
                path.idents[0] == parse::token::special_idents::invalid => {
                // Do nothing.
            }
            _ => {
                print_irrefutable_pat(s, input.pat);
                word(s.s, ~":");
                space(s.s);
            }
        }
        print_type(s, input.ty);
      }
    }
    end(s);
}

fn print_ty_fn(s: ps,
               opt_proto: Option<ast::Proto>,
               opt_region: Option<@ast::region>,
               purity: ast::purity,
               onceness: ast::Onceness,
               bounds: @~[ast::ty_param_bound],
               decl: ast::fn_decl, id: Option<ast::ident>,
               tps: Option<~[ast::ty_param]>,
               opt_self_ty: Option<ast::self_ty_>) {
    ibox(s, indent_unit);

    // Duplicates the logic in `print_fn_header_info()`.  This is because that
    // function prints the proto in the wrong place.  That should be fixed.
    print_self_ty_if_static(s, opt_self_ty);
    print_opt_proto(s, opt_proto);
    for opt_region.each |r| { print_region(s, ~"", *r, ~"/"); }
    print_purity(s, purity);
    print_onceness(s, onceness);
    word(s.s, ~"fn");
    print_bounds(s, bounds);
    match id { Some(id) => { word(s.s, ~" "); print_ident(s, id); } _ => () }
    match tps { Some(tps) => print_type_params(s, tps), _ => () }
    zerobreak(s.s);

    popen(s);
    // It is unfortunate to duplicate the commasep logic, but we
    // we want the self type, the args, and the capture clauses all
    // in the same box.
    box(s, 0u, inconsistent);
    let mut first = true;
    for opt_self_ty.each |self_ty| {
        first = !print_self_ty(s, *self_ty);
    }
    for decl.inputs.each |arg| {
        if first { first = false; } else { word_space(s, ~","); }
        print_arg(s, *arg);
    }
    end(s);
    pclose(s);

    maybe_print_comment(s, decl.output.span.lo);

    match decl.output.node {
        ast::ty_nil => {}
        _ => {
            space_if_not_bol(s);
            ibox(s, indent_unit);
            word_space(s, ~"->");
            if decl.cf == ast::noreturn { word_nbsp(s, ~"!"); }
            else { print_type(s, decl.output); }
            end(s);
        }
    }

    end(s);
}

fn maybe_print_trailing_comment(s: ps, span: codemap::span,
                                next_pos: Option<BytePos>) {
    let mut cm;
    match s.cm { Some(ccm) => cm = ccm, _ => return }
    match next_comment(s) {
      Some(ref cmnt) => {
        if (*cmnt).style != comments::trailing { return; }
        let span_line = cm.lookup_char_pos(span.hi);
        let comment_line = cm.lookup_char_pos((*cmnt).pos);
        let mut next = (*cmnt).pos + BytePos(1u);
        match next_pos { None => (), Some(p) => next = p }
        if span.hi < (*cmnt).pos && (*cmnt).pos < next &&
               span_line.line == comment_line.line {
            print_comment(s, (*cmnt));
            s.cur_cmnt += 1u;
        }
      }
      _ => ()
    }
}

fn print_remaining_comments(s: ps) {
    // If there aren't any remaining comments, then we need to manually
    // make sure there is a line break at the end.
    if next_comment(s).is_none() { hardbreak(s.s); }
    loop {
        match next_comment(s) {
          Some(ref cmnt) => { print_comment(s, (*cmnt)); s.cur_cmnt += 1u; }
          _ => break
        }
    }
}

fn print_literal(s: ps, &&lit: @ast::lit) {
    maybe_print_comment(s, lit.span.lo);
    match next_lit(s, lit.span.lo) {
      Some(ref ltrl) => {
        word(s.s, (*ltrl).lit);
        return;
      }
      _ => ()
    }
    match lit.node {
      ast::lit_str(st) => print_string(s, *st),
      ast::lit_int(ch, ast::ty_char) => {
        word(s.s, ~"'" + char::escape_default(ch as char) + ~"'");
      }
      ast::lit_int(i, t) => {
        if i < 0_i64 {
            word(s.s,
                 ~"-" + u64::to_str(-i as u64, 10u)
                 + ast_util::int_ty_to_str(t));
        } else {
            word(s.s,
                 u64::to_str(i as u64, 10u)
                 + ast_util::int_ty_to_str(t));
        }
      }
      ast::lit_uint(u, t) => {
        word(s.s,
             u64::to_str(u, 10u)
             + ast_util::uint_ty_to_str(t));
      }
      ast::lit_int_unsuffixed(i) => {
        if i < 0_i64 {
            word(s.s, ~"-" + u64::to_str(-i as u64, 10u));
        } else {
            word(s.s, u64::to_str(i as u64, 10u));
        }
      }
      ast::lit_float(f, t) => {
        word(s.s, *f + ast_util::float_ty_to_str(t));
      }
      ast::lit_float_unsuffixed(f) => word(s.s, *f),
      ast::lit_nil => word(s.s, ~"()"),
      ast::lit_bool(val) => {
        if val { word(s.s, ~"true"); } else { word(s.s, ~"false"); }
      }
    }
}

fn lit_to_str(l: @ast::lit) -> ~str {
    return to_str(l, print_literal, parse::token::mk_fake_ident_interner());
}

fn next_lit(s: ps, pos: BytePos) -> Option<comments::lit> {
    match s.literals {
      Some(ref lits) => {
        while s.cur_lit < vec::len((*lits)) {
            let ltrl = (*lits)[s.cur_lit];
            if ltrl.pos > pos { return None; }
            s.cur_lit += 1u;
            if ltrl.pos == pos { return Some(ltrl); }
        }
        return None;
      }
      _ => return None
    }
}

fn maybe_print_comment(s: ps, pos: BytePos) {
    loop {
        match next_comment(s) {
          Some(ref cmnt) => {
            if (*cmnt).pos < pos {
                print_comment(s, (*cmnt));
                s.cur_cmnt += 1u;
            } else { break; }
          }
          _ => break
        }
    }
}

fn print_comment(s: ps, cmnt: comments::cmnt) {
    match cmnt.style {
      comments::mixed => {
        assert (vec::len(cmnt.lines) == 1u);
        zerobreak(s.s);
        word(s.s, cmnt.lines[0]);
        zerobreak(s.s);
      }
      comments::isolated => {
        pprust::hardbreak_if_not_bol(s);
        for cmnt.lines.each |line| {
            // Don't print empty lines because they will end up as trailing
            // whitespace
            if str::is_not_empty(*line) { word(s.s, *line); }
            hardbreak(s.s);
        }
      }
      comments::trailing => {
        word(s.s, ~" ");
        if vec::len(cmnt.lines) == 1u {
            word(s.s, cmnt.lines[0]);
            hardbreak(s.s);
        } else {
            ibox(s, 0u);
            for cmnt.lines.each |line| {
                if str::is_not_empty(*line) { word(s.s, *line); }
                hardbreak(s.s);
            }
            end(s);
        }
      }
      comments::blank_line => {
        // We need to do at least one, possibly two hardbreaks.
        let is_semi =
            match s.s.last_token() {
              pp::STRING(s, _) => *s == ~";",
              _ => false
            };
        if is_semi || is_begin(s) || is_end(s) { hardbreak(s.s); }
        hardbreak(s.s);
      }
    }
}

fn print_string(s: ps, st: ~str) {
    word(s.s, ~"\"");
    word(s.s, str::escape_default(st));
    word(s.s, ~"\"");
}

fn to_str<T>(t: T, f: fn@(ps, T), intr: @ident_interner) -> ~str {
    do io::with_str_writer |wr| {
        let s = rust_printer(wr, intr);
        f(s, t);
        eof(s.s);
    }
}

fn next_comment(s: ps) -> Option<comments::cmnt> {
    match s.comments {
      Some(ref cmnts) => {
        if s.cur_cmnt < vec::len((*cmnts)) {
            return Some((*cmnts)[s.cur_cmnt]);
        } else { return None::<comments::cmnt>; }
      }
      _ => return None::<comments::cmnt>
    }
}

fn print_self_ty_if_static(s: ps,
                           opt_self_ty: Option<ast::self_ty_>) {
    match opt_self_ty {
        Some(ast::sty_static) => { word(s.s, ~"static "); }
        _ => {}
    }
}

fn print_opt_purity(s: ps, opt_purity: Option<ast::purity>) {
    match opt_purity {
        Some(ast::impure_fn) => { }
        Some(purity) => {
            word_nbsp(s, purity_to_str(purity));
        }
        None => {}
    }
}

fn print_opt_proto(s: ps, opt_proto: Option<ast::Proto>) {
    match opt_proto {
        Some(ast::ProtoBare) => { word(s.s, ~"extern "); }
        Some(ast::ProtoBorrowed) => { word(s.s, ~"&"); }
        Some(ast::ProtoUniq) => { word(s.s, ~"~"); }
        Some(ast::ProtoBox) => { word(s.s, ~"@"); }
        None => {}
    };
}

fn print_fn_header_info(s: ps,
                        opt_sty: Option<ast::self_ty_>,
                        opt_purity: Option<ast::purity>,
                        onceness: ast::Onceness,
                        opt_proto: Option<ast::Proto>,
                        vis: ast::visibility)
{
    print_self_ty_if_static(s, opt_sty);
    word(s.s, visibility_qualified(vis, ~""));
    print_opt_purity(s, opt_purity);
    print_onceness(s, onceness);
    word(s.s, ~"fn");
    print_opt_proto(s, opt_proto);
}

fn opt_proto_to_str(opt_p: Option<ast::Proto>) -> ~str {
    match opt_p {
      None => ~"fn",
      Some(p) => proto_to_str(p)
    }
}

pure fn purity_to_str(p: ast::purity) -> ~str {
    match p {
      ast::impure_fn => ~"impure",
      ast::unsafe_fn => ~"unsafe",
      ast::pure_fn => ~"pure",
      ast::extern_fn => ~"extern"
    }
}

pure fn onceness_to_str(o: ast::Onceness) -> ~str {
    match o {
        ast::Once => ~"once",
        ast::Many => ~"many"
    }
}

fn print_purity(s: ps, p: ast::purity) {
    match p {
      ast::impure_fn => (),
      _ => word_nbsp(s, purity_to_str(p))
    }
}

fn print_onceness(s: ps, o: ast::Onceness) {
    match o {
        ast::Once => { word_nbsp(s, ~"once"); }
        ast::Many => {}
    }
}

fn proto_to_str(p: ast::Proto) -> ~str {
    return match p {
      ast::ProtoBare => ~"extern fn",
      ast::ProtoBorrowed => ~"fn&",
      ast::ProtoUniq => ~"fn~",
      ast::ProtoBox => ~"fn@"
    };
}

#[cfg(test)]
mod test {
    use ast;
    use ast_util;
    use parse;
    use super::*;
    //use util;

    fn string_check<T : Eq> (given : &T, expected: &T) {
        if !(given == expected) {
            fail (fmt!("given %?, expected %?",given,expected));
        }
    }

    #[test]
    fn test_fun_to_str() {
        let mock_interner = parse::token::mk_fake_ident_interner();
        let abba_ident = mock_interner.intern(@~"abba");

        let decl: ast::fn_decl = ast::fn_decl {
            inputs: ~[],
            output: @ast::Ty {id: 0,
                              node: ast::ty_nil,
                              span: ast_util::dummy_sp()},
            cf: ast::return_val
        };
        assert fun_to_str(decl, abba_ident, ~[],mock_interner)
            == ~"fn abba()";
    }

    #[test]
    fn test_variant_to_str() {
        let mock_interner = parse::token::mk_fake_ident_interner();
        let ident = mock_interner.intern(@~"principal_skinner");

        let var = ast_util::respan(ast_util::dummy_sp(), ast::variant_ {
            name: ident,
            attrs: ~[],
            // making this up as I go.... ?
            kind: ast::tuple_variant_kind(~[]),
            id: 0,
            disr_expr: None,
            vis: ast::public,
        });

        let varstr = variant_to_str(var,mock_interner);
        string_check(&varstr,&~"pub principal_skinner");
    }
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
