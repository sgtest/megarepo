// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast::{MetaItem, Item, Expr};
use codemap::Span;
use ext::base::ExtCtxt;
use ext::build::AstBuilder;
use ext::deriving::generic::*;
use parse::token::InternedString;

pub fn expand_deriving_eq(cx: &mut ExtCtxt,
                          span: Span,
                          mitem: @MetaItem,
                          item: @Item,
                          push: |@Item|) {
    // structures are equal if all fields are equal, and non equal, if
    // any fields are not equal or if the enum variants are different
    fn cs_eq(cx: &mut ExtCtxt, span: Span, substr: &Substructure) -> @Expr {
        cs_and(|cx, span, _, _| cx.expr_bool(span, false),
                                 cx, span, substr)
    }
    fn cs_ne(cx: &mut ExtCtxt, span: Span, substr: &Substructure) -> @Expr {
        cs_or(|cx, span, _, _| cx.expr_bool(span, true),
              cx, span, substr)
    }

    macro_rules! md (
        ($name:expr, $f:ident) => { {
            let inline = cx.meta_word(span, InternedString::new("inline"));
            let attrs = vec!(cx.attribute(span, inline));
            MethodDef {
                name: $name,
                generics: LifetimeBounds::empty(),
                explicit_self: borrowed_explicit_self(),
                args: vec!(borrowed_self()),
                ret_ty: Literal(Path::new(vec!("bool"))),
                attributes: attrs,
                const_nonmatching: true,
                combine_substructure: combine_substructure(|a, b, c| {
                    $f(a, b, c)
                })
            }
        } }
    );

    let trait_def = TraitDef {
        span: span,
        attributes: Vec::new(),
        path: Path::new(vec!("std", "cmp", "PartialEq")),
        additional_bounds: Vec::new(),
        generics: LifetimeBounds::empty(),
        methods: vec!(
            md!("eq", cs_eq),
            md!("ne", cs_ne)
        )
    };
    trait_def.expand(cx, mitem, item, push)
}
