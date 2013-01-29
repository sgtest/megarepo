// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Converts the Rust AST to the rustdoc document model

use core::prelude::*;

use astsrv;
use doc::ItemUtils;
use doc;

use core::cast;
use core::task::local_data::local_data_get;
use core::vec;
use syntax::ast;
use syntax;

/* can't import macros yet, so this is copied from token.rs. See its comment
 * there. */
macro_rules! interner_key (
    () => (cast::transmute::<(uint, uint),
           &fn(+v: @@syntax::parse::token::ident_interner)>((-3 as uint, 0u)))
)

// Hack; rather than thread an interner through everywhere, rely on
// thread-local data
pub fn to_str(id: ast::ident) -> ~str {
    let intr = unsafe{ local_data_get(interner_key!()) };

    return *(*intr.get()).get(id);
}

pub fn interner() -> @syntax::parse::token::ident_interner {
    return *(unsafe{ local_data_get(interner_key!()) }).get();
}

pub fn from_srv(
    srv: astsrv::Srv,
    +default_name: ~str
) -> doc::Doc {

    //! Use the AST service to create a document tree

    do astsrv::exec(srv) |ctxt| {
        extract(ctxt.ast, default_name)
    }
}

pub fn extract(
    crate: @ast::crate,
    +default_name: ~str
) -> doc::Doc {
    doc::Doc_(doc::Doc_ {
        pages: ~[
            doc::CratePage(doc::CrateDoc {
                topmod: top_moddoc_from_crate(crate, default_name),
            })
        ]
    })
}

fn top_moddoc_from_crate(
    crate: @ast::crate,
    +default_name: ~str
) -> doc::ModDoc {
    moddoc_from_mod(mk_itemdoc(ast::crate_node_id, default_name),
                    crate.node.module)
}

fn mk_itemdoc(id: ast::node_id, +name: ~str) -> doc::ItemDoc {
    doc::ItemDoc {
        id: id,
        name: name,
        path: ~[],
        brief: None,
        desc: None,
        sections: ~[],
        reexport: false
    }
}

fn moddoc_from_mod(
    +itemdoc: doc::ItemDoc,
    module_: ast::_mod
) -> doc::ModDoc {
    doc::ModDoc_(doc::ModDoc_ {
        item: itemdoc,
        items: do vec::filter_map(module_.items) |item| {
            let ItemDoc = mk_itemdoc(item.id, to_str(item.ident));
            match item.node {
              ast::item_mod(m) => {
                Some(doc::ModTag(
                    moddoc_from_mod(ItemDoc, m)
                ))
              }
              ast::item_foreign_mod(nm) => {
                Some(doc::NmodTag(
                    nmoddoc_from_mod(ItemDoc, nm)
                ))
              }
              ast::item_fn(*) => {
                Some(doc::FnTag(
                    fndoc_from_fn(ItemDoc)
                ))
              }
              ast::item_const(_, _) => {
                Some(doc::ConstTag(
                    constdoc_from_const(ItemDoc)
                ))
              }
              ast::item_enum(enum_definition, _) => {
                Some(doc::EnumTag(
                    enumdoc_from_enum(ItemDoc, enum_definition.variants)
                ))
              }
              ast::item_trait(_, _, methods) => {
                Some(doc::TraitTag(
                    traitdoc_from_trait(ItemDoc, methods)
                ))
              }
              ast::item_impl(_, _, _, methods) => {
                Some(doc::ImplTag(
                    impldoc_from_impl(ItemDoc, methods)
                ))
              }
              ast::item_ty(_, _) => {
                Some(doc::TyTag(
                    tydoc_from_ty(ItemDoc)
                ))
              }
              ast::item_struct(def, _) => {
                Some(doc::StructTag(
                    structdoc_from_struct(ItemDoc, def)
                ))
              }
              _ => None
            }
        },
        index: None
    })
}

fn nmoddoc_from_mod(
    +itemdoc: doc::ItemDoc,
    module_: ast::foreign_mod
) -> doc::NmodDoc {
    let mut fns = ~[];
    for module_.items.each |item| {
        let ItemDoc = mk_itemdoc(item.id, to_str(item.ident));
        match item.node {
          ast::foreign_item_fn(*) => {
            fns.push(fndoc_from_fn(ItemDoc));
          }
          ast::foreign_item_const(*) => {} // XXX: Not implemented.
        }
    }
    doc:: NmodDoc {
        item: itemdoc,
        fns: fns,
        index: None
    }
}

fn fndoc_from_fn(+itemdoc: doc::ItemDoc) -> doc::FnDoc {
    doc::SimpleItemDoc {
        item: itemdoc,
        sig: None
    }
}

fn constdoc_from_const(+itemdoc: doc::ItemDoc) -> doc::ConstDoc {
    doc::SimpleItemDoc {
        item: itemdoc,
        sig: None
    }
}

#[test]
fn should_extract_const_name_and_id() {
    let doc = test::mk_doc(~"const a: int = 0;");
    assert doc.cratemod().consts()[0].id() != 0;
    assert doc.cratemod().consts()[0].name() == ~"a";
}

fn enumdoc_from_enum(
    +itemdoc: doc::ItemDoc,
    +variants: ~[ast::variant]
) -> doc::EnumDoc {
    doc::EnumDoc {
        item: itemdoc,
        variants: variantdocs_from_variants(variants)
    }
}

fn variantdocs_from_variants(
    +variants: ~[ast::variant]
) -> ~[doc::VariantDoc] {
    vec::map(variants, variantdoc_from_variant)
}

fn variantdoc_from_variant(variant: &ast::variant) -> doc::VariantDoc {
    doc::VariantDoc {
        name: to_str(variant.node.name),
        desc: None,
        sig: None
    }
}

#[test]
fn should_extract_enums() {
    let doc = test::mk_doc(~"enum e { v }");
    assert doc.cratemod().enums()[0].id() != 0;
    assert doc.cratemod().enums()[0].name() == ~"e";
}

#[test]
fn should_extract_enum_variants() {
    let doc = test::mk_doc(~"enum e { v }");
    assert doc.cratemod().enums()[0].variants[0].name == ~"v";
}

fn traitdoc_from_trait(
    +itemdoc: doc::ItemDoc,
    +methods: ~[ast::trait_method]
) -> doc::TraitDoc {
    doc::TraitDoc {
        item: itemdoc,
        methods: do vec::map(methods) |method| {
            match *method {
              ast::required(ty_m) => {
                doc::MethodDoc {
                    name: to_str(ty_m.ident),
                    brief: None,
                    desc: None,
                    sections: ~[],
                    sig: None,
                    implementation: doc::Required,
                }
              }
              ast::provided(m) => {
                doc::MethodDoc {
                    name: to_str(m.ident),
                    brief: None,
                    desc: None,
                    sections: ~[],
                    sig: None,
                    implementation: doc::Provided,
                }
              }
            }
        }
    }
}

#[test]
fn should_extract_traits() {
    let doc = test::mk_doc(~"trait i { fn f(); }");
    assert doc.cratemod().traits()[0].name() == ~"i";
}

#[test]
fn should_extract_trait_methods() {
    let doc = test::mk_doc(~"trait i { fn f(); }");
    assert doc.cratemod().traits()[0].methods[0].name == ~"f";
}

fn impldoc_from_impl(
    +itemdoc: doc::ItemDoc,
    methods: ~[@ast::method]
) -> doc::ImplDoc {
    doc::ImplDoc {
        item: itemdoc,
        trait_types: ~[],
        self_ty: None,
        methods: do vec::map(methods) |method| {
            doc::MethodDoc {
                name: to_str(method.ident),
                brief: None,
                desc: None,
                sections: ~[],
                sig: None,
                implementation: doc::Provided,
            }
        }
    }
}

#[test]
fn should_extract_impl_methods() {
    let doc = test::mk_doc(~"impl int { fn f() { } }");
    assert doc.cratemod().impls()[0].methods[0].name == ~"f";
}

fn tydoc_from_ty(
    +itemdoc: doc::ItemDoc
) -> doc::TyDoc {
    doc::SimpleItemDoc {
        item: itemdoc,
        sig: None
    }
}

#[test]
fn should_extract_tys() {
    let doc = test::mk_doc(~"type a = int;");
    assert doc.cratemod().types()[0].name() == ~"a";
}

fn structdoc_from_struct(
    +itemdoc: doc::ItemDoc,
    struct_def: @ast::struct_def
) -> doc::StructDoc {
    doc::StructDoc {
        item: itemdoc,
        fields: do struct_def.fields.map |field| {
            match field.node.kind {
                ast::named_field(ident, _, _) => to_str(ident),
                ast::unnamed_field => fail ~"what is an unnamed struct field?"
            }
        },
        sig: None
    }
}

#[test]
fn should_extract_structs() {
    let doc = test::mk_doc(~"struct Foo { field: () }");
    assert doc.cratemod().structs()[0].name() == ~"Foo";
}

#[test]
fn should_extract_struct_fields() {
    let doc = test::mk_doc(~"struct Foo { field: () }");
    assert doc.cratemod().structs()[0].fields[0] == ~"field";
}

#[cfg(test)]
mod test {
    #[legacy_exports];

    use astsrv;
    use doc;
    use extract::{extract, from_srv};
    use parse;

    use core::vec;

    fn mk_doc(+source: ~str) -> doc::Doc {
        let ast = parse::from_str(source);
        extract(ast, ~"")
    }

    #[test]
    fn extract_empty_crate() {
        let doc = mk_doc(~"");
        assert vec::is_empty(doc.cratemod().mods());
        assert vec::is_empty(doc.cratemod().fns());
    }

    #[test]
    fn extract_mods() {
        let doc = mk_doc(~"mod a { mod b { } mod c { } }");
        assert doc.cratemod().mods()[0].name() == ~"a";
        assert doc.cratemod().mods()[0].mods()[0].name() == ~"b";
        assert doc.cratemod().mods()[0].mods()[1].name() == ~"c";
    }

    #[test]
    fn extract_foreign_mods() {
        let doc = mk_doc(~"extern mod a { }");
        assert doc.cratemod().nmods()[0].name() == ~"a";
    }

    #[test]
    fn extract_fns_from_foreign_mods() {
        let doc = mk_doc(~"extern mod a { fn a(); }");
        assert doc.cratemod().nmods()[0].fns[0].name() == ~"a";
    }

    #[test]
    fn extract_mods_deep() {
        let doc = mk_doc(~"mod a { mod b { mod c { } } }");
        assert doc.cratemod().mods()[0].mods()[0].mods()[0].name() == ~"c";
    }

    #[test]
    fn extract_should_set_mod_ast_id() {
        let doc = mk_doc(~"mod a { }");
        assert doc.cratemod().mods()[0].id() != 0;
    }

    #[test]
    fn extract_fns() {
        let doc = mk_doc(
            ~"fn a() { } \
             mod b {
                 #[legacy_exports]; fn c() { } }");
        assert doc.cratemod().fns()[0].name() == ~"a";
        assert doc.cratemod().mods()[0].fns()[0].name() == ~"c";
    }

    #[test]
    fn extract_should_set_fn_ast_id() {
        let doc = mk_doc(~"fn a() { }");
        assert doc.cratemod().fns()[0].id() != 0;
    }

    #[test]
    fn extract_should_use_default_crate_name() {
        let source = ~"";
        let ast = parse::from_str(source);
        let doc = extract(ast, ~"burp");
        assert doc.cratemod().name() == ~"burp";
    }

    #[test]
    fn extract_from_seq_srv() {
        let source = ~"";
        do astsrv::from_str(source) |srv| {
            let doc = from_srv(srv, ~"name");
            assert doc.cratemod().name() == ~"name";
        }
    }
}
