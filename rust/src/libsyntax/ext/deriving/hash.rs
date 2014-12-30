// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast::{MetaItem, Item, Expr, MutMutable};
use codemap::Span;
use ext::base::ExtCtxt;
use ext::build::AstBuilder;
use ext::deriving::generic::*;
use ext::deriving::generic::ty::*;
use parse::token::InternedString;
use ptr::P;

pub fn expand_deriving_hash<F>(cx: &mut ExtCtxt,
                               span: Span,
                               mitem: &MetaItem,
                               item: &Item,
                               push: F) where
    F: FnOnce(P<Item>),
{

    let (path, generics, args) = if cx.ecfg.deriving_hash_type_parameter {
        (Path::new_(vec!("std", "hash", "Hash"), None,
                    vec!(box Literal(Path::new_local("__S"))), true),
         LifetimeBounds {
             lifetimes: Vec::new(),
             bounds: vec!(("__S",
                           vec!(Path::new(vec!("std", "hash", "Writer"))))),
         },
         Path::new_local("__S"))
    } else {
        (Path::new(vec!("std", "hash", "Hash")),
         LifetimeBounds::empty(),
         Path::new(vec!("std", "hash", "sip", "SipState")))
    };
    let inline = cx.meta_word(span, InternedString::new("inline"));
    let attrs = vec!(cx.attribute(span, inline));
    let hash_trait_def = TraitDef {
        span: span,
        attributes: Vec::new(),
        path: path,
        additional_bounds: Vec::new(),
        generics: generics,
        methods: vec!(
            MethodDef {
                name: "hash",
                generics: LifetimeBounds::empty(),
                explicit_self: borrowed_explicit_self(),
                args: vec!(Ptr(box Literal(args), Borrowed(None, MutMutable))),
                ret_ty: nil_ty(),
                attributes: attrs,
                combine_substructure: combine_substructure(|a, b, c| {
                    hash_substructure(a, b, c)
                })
            }
        )
    };

    hash_trait_def.expand(cx, mitem, item, push);
}

fn hash_substructure(cx: &mut ExtCtxt, trait_span: Span, substr: &Substructure) -> P<Expr> {
    let state_expr = match substr.nonself_args {
        [ref state_expr] => state_expr,
        _ => cx.span_bug(trait_span, "incorrect number of arguments in `deriving(Hash)`")
    };
    let hash_ident = substr.method_ident;
    let call_hash = |span, thing_expr| {
        let expr = cx.expr_method_call(span, thing_expr, hash_ident, vec!(state_expr.clone()));
        cx.stmt_expr(expr)
    };
    let mut stmts = Vec::new();

    let fields = match *substr.fields {
        Struct(ref fs) => fs,
        EnumMatching(index, variant, ref fs) => {
            // Determine the discriminant. We will feed this value to the byte
            // iteration function.
            let discriminant = match variant.node.disr_expr {
                Some(ref d) => d.clone(),
                None => cx.expr_uint(trait_span, index)
            };

            stmts.push(call_hash(trait_span, discriminant));

            fs
        }
        _ => cx.span_bug(trait_span, "impossible substructure in `deriving(Hash)`")
    };

    for &FieldInfo { ref self_, span, .. } in fields.iter() {
        stmts.push(call_hash(span, self_.clone()));
    }

    if stmts.len() == 0 {
        cx.span_bug(trait_span, "#[deriving(Hash)] needs at least one field");
    }

    cx.expr_block(cx.block(trait_span, stmts, None))
}
