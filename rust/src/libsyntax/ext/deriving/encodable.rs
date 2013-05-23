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

The compiler code necessary to implement the #[deriving(Encodable)]
(and Decodable, in decodable.rs) extension.  The idea here is that
type-defining items may be tagged with #[deriving(Encodable,
Decodable)].

For example, a type like:

    #[deriving(Encodable, Decodable)]
    struct Node {id: uint}

would generate two implementations like:

impl<S:extra::serialize::Encoder> Encodable<S> for Node {
    fn encode(&self, s: &S) {
        do s.emit_struct("Node", 1) {
            s.emit_field("id", 0, || s.emit_uint(self.id))
        }
    }
}

impl<D:Decoder> Decodable for node_id {
    fn decode(d: &D) -> Node {
        do d.read_struct("Node", 1) {
            Node {
                id: d.read_field(~"x", 0, || decode(d))
            }
        }
    }
}

Other interesting scenarios are whe the item has type parameters or
references other non-built-in types.  A type definition like:

    #[deriving(Encodable, Decodable)]
    struct spanned<T> {node: T, span: span}

would yield functions like:

    impl<
        S: Encoder,
        T: Encodable<S>
    > spanned<T>: Encodable<S> {
        fn encode<S:Encoder>(s: &S) {
            do s.emit_rec {
                s.emit_field("node", 0, || self.node.encode(s));
                s.emit_field("span", 1, || self.span.encode(s));
            }
        }
    }

    impl<
        D: Decoder,
        T: Decodable<D>
    > spanned<T>: Decodable<D> {
        fn decode(d: &D) -> spanned<T> {
            do d.read_rec {
                {
                    node: d.read_field(~"node", 0, || decode(d)),
                    span: d.read_field(~"span", 1, || decode(d)),
                }
            }
        }
    }
*/

use core::prelude::*;

use ast;
use ast::*;
use ext::base::ExtCtxt;
use ext::build::AstBuilder;
use ext::deriving::*;
use codemap::{span, spanned};
use ast_util;
use opt_vec;

pub fn expand_deriving_encodable(
    cx: @ExtCtxt,
    span: span,
    _mitem: @meta_item,
    in_items: ~[@item]
) -> ~[@item] {
    expand_deriving(
        cx,
        span,
        in_items,
        expand_deriving_encodable_struct_def,
        expand_deriving_encodable_enum_def
    )
}

fn create_derived_encodable_impl(
    cx: @ExtCtxt,
    span: span,
    type_ident: ident,
    generics: &Generics,
    method: @method
) -> @item {
    let encoder_ty_param = cx.typaram(
        cx.ident_of("__E"),
        @opt_vec::with(
            cx.typarambound(
                cx.path_global(
                    span,
                    ~[
                        cx.ident_of("extra"),
                        cx.ident_of("serialize"),
                        cx.ident_of("Encoder"),
                    ]))));

    // All the type parameters need to bound to the trait.
    let generic_ty_params = opt_vec::with(encoder_ty_param);

    let methods = [method];
    let trait_path = cx.path_all(
        span,
        true,
        ~[
            cx.ident_of("extra"),
            cx.ident_of("serialize"),
            cx.ident_of("Encodable")
        ],
        None,
        ~[
            cx.ty_ident(span, cx.ident_of("__E"))
        ]
    );
    create_derived_impl(
        cx,
        span,
        type_ident,
        generics,
        methods,
        trait_path,
        Generics { ty_params: generic_ty_params, lifetimes: opt_vec::Empty },
        opt_vec::Empty
    )
}

// Creates a method from the given set of statements conforming to the
// signature of the `encodable` method.
fn create_encode_method(
    cx: @ExtCtxt,
    span: span,
    statements: ~[@stmt]
) -> @method {
    // Create the `e` parameter.
    let e_arg_type = cx.ty_rptr(
        span,
        cx.ty_ident(span, cx.ident_of("__E")),
        None,
        ast::m_mutbl
    );
    let e_arg = cx.arg(span, cx.ident_of("__e"), e_arg_type);

    // Create the type of the return value.
    let output_type = cx.ty_nil();

    // Create the function declaration.
    let inputs = ~[e_arg];
    let fn_decl = cx.fn_decl(inputs, output_type);

    // Create the body block.
    let body_block = cx.blk(span, statements, None);

    // Create the method.
    let explicit_self = spanned { node: sty_region(None, m_imm), span: span };
    let method_ident = cx.ident_of("encode");
    @ast::method {
        ident: method_ident,
        attrs: ~[],
        generics: ast_util::empty_generics(),
        explicit_self: explicit_self,
        purity: impure_fn,
        decl: fn_decl,
        body: body_block,
        id: cx.next_id(),
        span: span,
        self_id: cx.next_id(),
        vis: public
    }
}

fn call_substructure_encode_method(
    cx: @ExtCtxt,
    span: span,
    self_field: @expr
) -> @ast::expr {
    // Gather up the parameters we want to chain along.
    let e_ident = cx.ident_of("__e");
    let e_expr = cx.expr_ident(span, e_ident);

    // Call the substructure method.
    let encode_ident = cx.ident_of("encode");
    cx.expr_method_call(
        span,
        self_field,
        encode_ident,
        ~[e_expr]
    )
}

fn expand_deriving_encodable_struct_def(
    cx: @ExtCtxt,
    span: span,
    struct_def: &struct_def,
    type_ident: ident,
    generics: &Generics
) -> @item {
    // Create the method.
    let method = expand_deriving_encodable_struct_method(
        cx,
        span,
        type_ident,
        struct_def
    );

    // Create the implementation.
    create_derived_encodable_impl(
        cx,
        span,
        type_ident,
        generics,
        method
    )
}

fn expand_deriving_encodable_enum_def(
    cx: @ExtCtxt,
    span: span,
    enum_definition: &enum_def,
    type_ident: ident,
    generics: &Generics
) -> @item {
    // Create the method.
    let method = expand_deriving_encodable_enum_method(
        cx,
        span,
        type_ident,
        enum_definition
    );

    // Create the implementation.
    create_derived_encodable_impl(
        cx,
        span,
        type_ident,
        generics,
        method
    )
}

fn expand_deriving_encodable_struct_method(
    cx: @ExtCtxt,
    span: span,
    type_ident: ident,
    struct_def: &struct_def
) -> @method {
    // Create the body of the method.
    let mut idx = 0;
    let mut statements = ~[];
    for struct_def.fields.each |struct_field| {
        match struct_field.node.kind {
            named_field(ident, _) => {
                // Create the accessor for this field.
                let self_field = cx.expr_field_access(span,
                                                      cx.expr_self(span),
                                                      ident);

                // Call the substructure method.
                let encode_expr = call_substructure_encode_method(
                    cx,
                    span,
                    self_field
                );

                let e_ident = cx.ident_of("__e");

                let call_expr = cx.expr_method_call(
                    span,
                    cx.expr_ident(span, e_ident),
                    cx.ident_of("emit_struct_field"),
                    ~[
                        cx.expr_str(span, cx.str_of(ident)),
                        cx.expr_uint(span, idx),
                        cx.lambda_expr_1(span, encode_expr, e_ident)
                    ]
                );

                statements.push(cx.stmt_expr(call_expr));
            }
            unnamed_field => {
                cx.span_unimpl(
                    span,
                    "unnamed fields with `deriving(Encodable)`"
                );
            }
        }
        idx += 1;
    }

    let e_id = cx.ident_of("__e");
    let emit_struct_stmt = cx.expr_method_call(
        span,
        cx.expr_ident(span, e_id),
        cx.ident_of("emit_struct"),
        ~[
            cx.expr_str(span, cx.str_of(type_ident)),
            cx.expr_uint(span, statements.len()),
            cx.lambda_stmts_1(span, statements, e_id),
        ]
    );

    let statements = ~[cx.stmt_expr(emit_struct_stmt)];

    // Create the method itself.
    return create_encode_method(cx, span, statements);
}

fn expand_deriving_encodable_enum_method(
    cx: @ExtCtxt,
    span: span,
    type_ident: ast::ident,
    enum_definition: &enum_def
) -> @method {
    // Create the arms of the match in the method body.
    let arms = do enum_definition.variants.mapi |i, variant| {
        // Create the matching pattern.
        let (pat, fields) = create_enum_variant_pattern(cx, span, variant, "__self", ast::m_imm);

        // Feed the discriminant to the encode function.
        let mut stmts = ~[];

        // Feed each argument in this variant to the encode function
        // as well.
        let variant_arg_len = variant_arg_count(cx, span, variant);
        for fields.eachi |j, &(_, field)| {
            // Call the substructure method.
            let expr = call_substructure_encode_method(cx, span, field);

            let e_ident = cx.ident_of("__e");
            let call_expr = cx.expr_method_call(
                span,
                cx.expr_ident(span, e_ident),
                cx.ident_of("emit_enum_variant_arg"),
                ~[
                    cx.expr_uint(span, j),
                    cx.lambda_expr_1(span, expr, e_ident),
                ]
            );

            stmts.push(cx.stmt_expr(call_expr));
        }

        // Create the pattern body.
        let e_id = cx.ident_of("__e");

        let call_expr = cx.expr_method_call(
            span,
            cx.expr_ident(span, e_id),
            cx.ident_of("emit_enum_variant"),
            ~[
                cx.expr_str(span, cx.str_of(variant.node.name)),
                cx.expr_uint(span, i),
                cx.expr_uint(span, variant_arg_len),
                cx.lambda_stmts_1(span, stmts, e_id)
            ]
        );

        //let match_body_block = cx.blk_expr(call_expr);

        // Create the arm.
        cx.arm(span, ~[pat], call_expr) //match_body_block)
    };

    let e_ident = cx.ident_of("__e");

    // Create the method body.
    let lambda_expr = cx.lambda_expr_1(
        span,
        expand_enum_or_struct_match(cx, span, arms),
        e_ident);

    let call_expr = cx.expr_method_call(
        span,
        cx.expr_ident(span, e_ident),
        cx.ident_of("emit_enum"),
        ~[
            cx.expr_str(span, cx.str_of(type_ident)),
            lambda_expr,
        ]
    );

    let stmt = cx.stmt_expr(call_expr);

    // Create the method.
    create_encode_method(cx, span, ~[stmt])
}

#[cfg(test)]
mod test {
    extern mod extra;
    use core::option::{None, Some};
    use extra::serialize::Encodable;
    use extra::serialize::Encoder;

    // just adding the ones I want to test, for now:
    #[deriving(Eq)]
    pub enum call {
        CallToEmitEnum(~str),
        CallToEmitEnumVariant(~str, uint, uint),
        CallToEmitEnumVariantArg(uint),
        CallToEmitUint(uint),
        CallToEmitNil,
        CallToEmitStruct(~str,uint),
        CallToEmitField(~str,uint),
        CallToEmitOption,
        CallToEmitOptionNone,
        CallToEmitOptionSome,
        // all of the ones I was too lazy to handle:
        CallToOther
    }
    // using `@mut` rather than changing the
    // type of self in every method of every encoder everywhere.
    pub struct TestEncoder {call_log : @mut ~[call]}

    pub impl TestEncoder {
        // these self's should be &mut self's, as well....
        fn add_to_log (&self, c : call) {
            self.call_log.push(copy c);
        }
        fn add_unknown_to_log (&self) {
            self.add_to_log (CallToOther)
        }
    }

    impl Encoder for TestEncoder {
        fn emit_nil(&mut self) { self.add_to_log(CallToEmitNil) }

        fn emit_uint(&mut self, v: uint) {
            self.add_to_log(CallToEmitUint(v));
        }
        fn emit_u64(&mut self, _v: u64) { self.add_unknown_to_log(); }
        fn emit_u32(&mut self, _v: u32) { self.add_unknown_to_log(); }
        fn emit_u16(&mut self, _v: u16) { self.add_unknown_to_log(); }
        fn emit_u8(&mut self, _v: u8)   { self.add_unknown_to_log(); }

        fn emit_int(&mut self, _v: int) { self.add_unknown_to_log(); }
        fn emit_i64(&mut self, _v: i64) { self.add_unknown_to_log(); }
        fn emit_i32(&mut self, _v: i32) { self.add_unknown_to_log(); }
        fn emit_i16(&mut self, _v: i16) { self.add_unknown_to_log(); }
        fn emit_i8(&mut self, _v: i8)   { self.add_unknown_to_log(); }

        fn emit_bool(&mut self, _v: bool) { self.add_unknown_to_log(); }

        fn emit_f64(&mut self, _v: f64) { self.add_unknown_to_log(); }
        fn emit_f32(&mut self, _v: f32) { self.add_unknown_to_log(); }
        fn emit_float(&mut self, _v: float) { self.add_unknown_to_log(); }

        fn emit_char(&mut self, _v: char) { self.add_unknown_to_log(); }
        fn emit_str(&mut self, _v: &str) { self.add_unknown_to_log(); }

        fn emit_enum(&mut self, name: &str, f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitEnum(name.to_str()));
            f(self);
        }

        fn emit_enum_variant(&mut self,
                             name: &str,
                             id: uint,
                             cnt: uint,
                             f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitEnumVariant(name.to_str(), id, cnt));
            f(self);
        }

        fn emit_enum_variant_arg(&mut self,
                                 idx: uint,
                                 f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitEnumVariantArg(idx));
            f(self);
        }

        fn emit_enum_struct_variant(&mut self,
                                    name: &str,
                                    id: uint,
                                    cnt: uint,
                                    f: &fn(&mut TestEncoder)) {
            self.emit_enum_variant(name, id, cnt, f)
        }

        fn emit_enum_struct_variant_field(&mut self,
                                          _name: &str,
                                          idx: uint,
                                          f: &fn(&mut TestEncoder)) {
            self.emit_enum_variant_arg(idx, f)
        }

        fn emit_struct(&mut self,
                       name: &str,
                       len: uint,
                       f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitStruct (name.to_str(),len));
            f(self);
        }
        fn emit_struct_field(&mut self,
                             name: &str,
                             idx: uint,
                             f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitField (name.to_str(),idx));
            f(self);
        }

        fn emit_tuple(&mut self, _len: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }
        fn emit_tuple_arg(&mut self, _idx: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }

        fn emit_tuple_struct(&mut self,
                             _name: &str,
                             _len: uint,
                             f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }

        fn emit_tuple_struct_arg(&mut self,
                                 _idx: uint,
                                 f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }

        fn emit_option(&mut self, f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitOption);
            f(self);
        }
        fn emit_option_none(&mut self) {
            self.add_to_log(CallToEmitOptionNone);
        }
        fn emit_option_some(&mut self, f: &fn(&mut TestEncoder)) {
            self.add_to_log(CallToEmitOptionSome);
            f(self);
        }

        fn emit_seq(&mut self, _len: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }
        fn emit_seq_elt(&mut self, _idx: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }

        fn emit_map(&mut self, _len: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }
        fn emit_map_elt_key(&mut self, _idx: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }
        fn emit_map_elt_val(&mut self, _idx: uint, f: &fn(&mut TestEncoder)) {
            self.add_unknown_to_log();
            f(self);
        }
    }


    fn to_call_log<E:Encodable<TestEncoder>>(val: E) -> ~[call] {
        let mut te = TestEncoder {
            call_log: @mut ~[]
        };
        val.encode(&mut te);
        copy *te.call_log
    }

    #[deriving(Encodable)]
    enum Written {
        Book(uint,uint),
        Magazine(~str)
    }

    #[test]
    fn test_encode_enum() {
        assert_eq!(
            to_call_log(Book(34,44)),
            ~[
                CallToEmitEnum(~"Written"),
                CallToEmitEnumVariant(~"Book",0,2),
                CallToEmitEnumVariantArg(0),
                CallToEmitUint(34),
                CallToEmitEnumVariantArg(1),
                CallToEmitUint(44),
            ]
        );
    }

    pub struct BPos(uint);

    #[deriving(Encodable)]
    pub struct HasPos { pos : BPos }

    #[test]
    fn test_encode_newtype() {
        assert_eq!(
            to_call_log(HasPos { pos:BPos(48) }),
            ~[
                CallToEmitStruct(~"HasPos",1),
                CallToEmitField(~"pos",0),
                CallToEmitUint(48),
            ]
        );
    }

    #[test]
    fn test_encode_option() {
        let mut v = None;

        assert_eq!(
            to_call_log(v),
            ~[
                CallToEmitOption,
                CallToEmitOptionNone,
            ]
        );

        v = Some(54u);
        assert_eq!(
            to_call_log(v),
            ~[
                CallToEmitOption,
                CallToEmitOptionSome,
                CallToEmitUint(54)
            ]
        );
    }
}
