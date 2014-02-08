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

pub fn expand_deriving_totaleq(cx: &mut ExtCtxt,
                               span: Span,
                               mitem: @MetaItem,
                               in_items: ~[@Item]) -> ~[@Item] {
    fn cs_equals(cx: &mut ExtCtxt, span: Span, substr: &Substructure) -> @Expr {
        cs_and(|cx, span, _, _| cx.expr_bool(span, false),
               cx, span, substr)
    }

    let trait_def = TraitDef {
        cx: cx, span: span,

        path: Path::new(~["std", "cmp", "TotalEq"]),
        additional_bounds: ~[],
        generics: LifetimeBounds::empty(),
        methods: ~[
            MethodDef {
                name: "equals",
                generics: LifetimeBounds::empty(),
                explicit_self: borrowed_explicit_self(),
                args: ~[borrowed_self()],
                ret_ty: Literal(Path::new(~["bool"])),
                inline: true,
                const_nonmatching: true,
                combine_substructure: cs_equals
            }
        ]
    };
    trait_def.expand(mitem, in_items)
}
