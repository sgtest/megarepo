// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
The compiler code necessary for `#[deriving(Decodable)]`. See
encodable.rs for more.
*/

use ast::{MetaItem, Item, Expr, MutMutable, Ident};
use codemap::Span;
use ext::base::ExtCtxt;
use ext::build::AstBuilder;
use ext::deriving::generic::*;
use ext::deriving::generic::ty::*;
use parse::token::InternedString;
use parse::token;

use std::gc::Gc;

pub fn expand_deriving_decodable(cx: &mut ExtCtxt,
                                 span: Span,
                                 mitem: Gc<MetaItem>,
                                 item: Gc<Item>,
                                 push: |Gc<Item>|) {
    let trait_def = TraitDef {
        span: span,
        attributes: Vec::new(),
        path: Path::new_(vec!("serialize", "Decodable"), None,
                         vec!(box Literal(Path::new_local("__D")),
                              box Literal(Path::new_local("__E"))), true),
        additional_bounds: Vec::new(),
        generics: LifetimeBounds {
            lifetimes: Vec::new(),
            bounds: vec!(("__D", None, vec!(Path::new_(
                            vec!("serialize", "Decoder"), None,
                            vec!(box Literal(Path::new_local("__E"))), true))),
                         ("__E", None, vec!()))
        },
        methods: vec!(
            MethodDef {
                name: "decode",
                generics: LifetimeBounds::empty(),
                explicit_self: None,
                args: vec!(Ptr(box Literal(Path::new_local("__D")),
                            Borrowed(None, MutMutable))),
                ret_ty: Literal(Path::new_(vec!("std", "result", "Result"), None,
                                          vec!(box Self,
                                               box Literal(Path::new_local("__E"))), true)),
                attributes: Vec::new(),
                const_nonmatching: true,
                combine_substructure: combine_substructure(|a, b, c| {
                    decodable_substructure(a, b, c)
                }),
            })
    };

    trait_def.expand(cx, mitem, item, push)
}

fn decodable_substructure(cx: &mut ExtCtxt, trait_span: Span,
                          substr: &Substructure) -> Gc<Expr> {
    let decoder = substr.nonself_args[0];
    let recurse = vec!(cx.ident_of("serialize"),
                    cx.ident_of("Decodable"),
                    cx.ident_of("decode"));
    // throw an underscore in front to suppress unused variable warnings
    let blkarg = cx.ident_of("_d");
    let blkdecoder = cx.expr_ident(trait_span, blkarg);
    let calldecode = cx.expr_call_global(trait_span, recurse, vec!(blkdecoder));
    let lambdadecode = cx.lambda_expr_1(trait_span, calldecode, blkarg);

    return match *substr.fields {
        StaticStruct(_, ref summary) => {
            let nfields = match *summary {
                Unnamed(ref fields) => fields.len(),
                Named(ref fields) => fields.len()
            };
            let read_struct_field = cx.ident_of("read_struct_field");

            let result = decode_static_fields(cx,
                                              trait_span,
                                              substr.type_ident,
                                              summary,
                                              |cx, span, name, field| {
                cx.expr_try(span,
                    cx.expr_method_call(span, blkdecoder, read_struct_field,
                                        vec!(cx.expr_str(span, name),
                                          cx.expr_uint(span, field),
                                          lambdadecode)))
            });
            let result = cx.expr_ok(trait_span, result);
            cx.expr_method_call(trait_span,
                                decoder,
                                cx.ident_of("read_struct"),
                                vec!(
                cx.expr_str(trait_span, token::get_ident(substr.type_ident)),
                cx.expr_uint(trait_span, nfields),
                cx.lambda_expr_1(trait_span, result, blkarg)
            ))
        }
        StaticEnum(_, ref fields) => {
            let variant = cx.ident_of("i");

            let mut arms = Vec::new();
            let mut variants = Vec::new();
            let rvariant_arg = cx.ident_of("read_enum_variant_arg");

            for (i, &(name, v_span, ref parts)) in fields.iter().enumerate() {
                variants.push(cx.expr_str(v_span, token::get_ident(name)));

                let decoded = decode_static_fields(cx,
                                                   v_span,
                                                   name,
                                                   parts,
                                                   |cx, span, _, field| {
                    let idx = cx.expr_uint(span, field);
                    cx.expr_try(span,
                        cx.expr_method_call(span, blkdecoder, rvariant_arg,
                                            vec!(idx, lambdadecode)))
                });

                arms.push(cx.arm(v_span,
                                 vec!(cx.pat_lit(v_span, cx.expr_uint(v_span, i))),
                                 decoded));
            }

            arms.push(cx.arm_unreachable(trait_span));

            let result = cx.expr_ok(trait_span,
                                    cx.expr_match(trait_span,
                                                  cx.expr_ident(trait_span, variant), arms));
            let lambda = cx.lambda_expr(trait_span, vec!(blkarg, variant), result);
            let variant_vec = cx.expr_vec(trait_span, variants);
            let result = cx.expr_method_call(trait_span, blkdecoder,
                                             cx.ident_of("read_enum_variant"),
                                             vec!(variant_vec, lambda));
            cx.expr_method_call(trait_span,
                                decoder,
                                cx.ident_of("read_enum"),
                                vec!(
                cx.expr_str(trait_span, token::get_ident(substr.type_ident)),
                cx.lambda_expr_1(trait_span, result, blkarg)
            ))
        }
        _ => cx.bug("expected StaticEnum or StaticStruct in deriving(Decodable)")
    };
}

/// Create a decoder for a single enum variant/struct:
/// - `outer_pat_ident` is the name of this enum variant/struct
/// - `getarg` should retrieve the `uint`-th field with name `@str`.
fn decode_static_fields(cx: &mut ExtCtxt,
                        trait_span: Span,
                        outer_pat_ident: Ident,
                        fields: &StaticFields,
                        getarg: |&mut ExtCtxt, Span, InternedString, uint| -> Gc<Expr>)
                        -> Gc<Expr> {
    match *fields {
        Unnamed(ref fields) => {
            if fields.is_empty() {
                cx.expr_ident(trait_span, outer_pat_ident)
            } else {
                let fields = fields.iter().enumerate().map(|(i, &span)| {
                    getarg(cx, span,
                           token::intern_and_get_ident(format!("_field{}",
                                                               i).as_slice()),
                           i)
                }).collect();

                cx.expr_call_ident(trait_span, outer_pat_ident, fields)
            }
        }
        Named(ref fields) => {
            // use the field's span to get nicer error messages.
            let fields = fields.iter().enumerate().map(|(i, &(name, span))| {
                let arg = getarg(cx, span, token::get_ident(name), i);
                cx.field_imm(span, name, arg)
            }).collect();
            cx.expr_struct_ident(trait_span, outer_pat_ident, fields)
        }
    }
}
