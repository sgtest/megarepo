#[doc = "Prunes items of the document tree that contain no documentation"];

export mk_pass;

fn mk_pass() -> pass {
    run
}

type ctxt = {
    mutable have_docs: bool
};

fn run(
    _srv: astsrv::srv,
    doc: doc::cratedoc
) -> doc::cratedoc {
    let ctxt = {
        mutable have_docs: true
    };
    let fold = fold::fold({
        fold_mod: fold_mod,
        fold_fn: fold_fn,
        fold_const: fold_const,
        fold_enum: fold_enum,
        fold_res: fold_res,
        fold_iface: fold_iface,
        fold_impl: fold_impl,
        fold_type: fold_type
        with *fold::default_any_fold(ctxt)
    });
    fold.fold_crate(fold, doc)
}

fn fold_mod(
    fold: fold::fold<ctxt>,
    doc: doc::moddoc
) -> doc::moddoc {
    let doc = {
        items: ~vec::filter_map(*doc.items) {|itemtag|
            alt itemtag {
              doc::modtag(moddoc) {
                let doc = fold.fold_mod(fold, moddoc);
                if fold.ctxt.have_docs {
                    some(doc::modtag(doc))
                } else {
                    none
                }
              }
              doc::fntag(fndoc) {
                let doc = fold.fold_fn(fold, fndoc);
                if fold.ctxt.have_docs {
                    some(doc::fntag(doc))
                } else {
                    none
                }
              }
              doc::consttag(constdoc) {
                let doc = fold.fold_const(fold, constdoc);
                if fold.ctxt.have_docs {
                    some(doc::consttag(doc))
                } else {
                    none
                }
              }
              doc::enumtag(enumdoc) {
                let doc = fold.fold_enum(fold, enumdoc);
                if fold.ctxt.have_docs {
                    some(doc::enumtag(doc))
                } else {
                    none
                }
              }
              doc::restag(resdoc) {
                let doc = fold.fold_res(fold, resdoc);
                if fold.ctxt.have_docs {
                    some(doc::restag(doc))
                } else {
                    none
                }
              }
              doc::ifacetag(ifacedoc) {
                let doc = fold.fold_iface(fold, ifacedoc);
                if fold.ctxt.have_docs {
                    some(doc::ifacetag(doc))
                } else {
                    none
                }
              }
              doc::impltag(impldoc) {
                let doc = fold.fold_impl(fold, impldoc);
                if fold.ctxt.have_docs {
                    some(doc::impltag(doc))
                } else {
                    none
                }
              }
              doc::tytag(tydoc) {
                let doc = fold.fold_type(fold, tydoc);
                if fold.ctxt.have_docs {
                    some(doc::tytag(doc))
                } else {
                    none
                }
              }
              _ { some(itemtag) }
            }
        }
        with fold::default_any_fold_mod(fold, doc)
    };
    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none
        || vec::is_not_empty(*doc.items);
    ret doc;
}

fn fold_fn(
    fold: fold::fold<ctxt>,
    doc: doc::fndoc
) -> doc::fndoc {
    let doc = fold::default_seq_fold_fn(fold, doc);

    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none
        || args_have_docs(doc.args)
        || doc.return.desc != none
        || doc.failure != none;
    ret doc;
}

fn args_have_docs(docs: [doc::argdoc]) -> bool {
    vec::foldl(false, docs) {|accum, doc|
        accum || doc.desc != none
    }
}

#[test]
fn should_elide_fns_with_undocumented_arguments() {
    let doc = test::mk_doc("fn a(a: int) { }");
    assert vec::is_empty(doc.topmod.fns());
}

#[test]
fn should_not_elide_fns_with_documented_arguments() {
    let doc = test::mk_doc("#[doc(args(a = \"b\"))] fn a(a: int) { }");
    assert vec::is_not_empty(doc.topmod.fns());
}

#[test]
fn should_not_elide_fns_with_documented_failure_conditions() {
    let doc = test::mk_doc("#[doc(failure = \"yup\")] fn a() { }");
    assert vec::is_not_empty(doc.topmod.fns());
}

#[test]
fn should_elide_undocumented_mods() {
    let doc = test::mk_doc("mod a { }");
    assert vec::is_empty(doc.topmod.mods());
}

#[test]
fn should_not_elide_undocument_mods_with_documented_mods() {
    let doc = test::mk_doc("mod a { #[doc = \"b\"] mod b { } }");
    assert vec::is_not_empty(doc.topmod.mods());
}

#[test]
fn should_not_elide_undocument_mods_with_documented_fns() {
    let doc = test::mk_doc("mod a { #[doc = \"b\"] fn b() { } }");
    assert vec::is_not_empty(doc.topmod.mods());
}

#[test]
fn should_elide_undocumented_fns() {
    let doc = test::mk_doc("fn a() { }");
    assert vec::is_empty(doc.topmod.fns());
}

fn fold_const(
    fold: fold::fold<ctxt>,
    doc: doc::constdoc
) -> doc::constdoc {
    let doc = fold::default_seq_fold_const(fold, doc);
    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none;
    ret doc;
}

#[test]
fn should_elide_undocumented_consts() {
    let doc = test::mk_doc("const a: bool = true;");
    assert vec::is_empty(doc.topmod.consts());
}

fn fold_enum(fold: fold::fold<ctxt>, doc: doc::enumdoc) -> doc::enumdoc {
    let doc = {
        variants: vec::filter_map(doc.variants) {|variant|
            if variant.desc != none {
                some(variant)
            } else {
                none
            }
        }
        with fold::default_seq_fold_enum(fold, doc)
    };
    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none
        || vec::is_not_empty(doc.variants);
    ret doc;
}

#[test]
fn should_elide_undocumented_enums() {
    let doc = test::mk_doc("enum a { b }");
    assert vec::is_empty(doc.topmod.enums());
}

#[test]
fn should_elide_undocumented_variants() {
    let doc = test::mk_doc("#[doc = \"a\"] enum a { b }");
    assert vec::is_empty(doc.topmod.enums()[0].variants);
}

#[test]
fn should_not_elide_enums_with_documented_variants() {
    let doc = test::mk_doc("enum a { #[doc = \"a\"] b }");
    assert vec::is_not_empty(doc.topmod.enums());
}

fn fold_res(fold: fold::fold<ctxt>, doc: doc::resdoc) -> doc::resdoc {
    let doc = fold::default_seq_fold_res(fold, doc);

    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none
        || args_have_docs(doc.args);
    ret doc;
}

#[test]
fn should_elide_undocumented_resources() {
    let doc = test::mk_doc("resource r(a: bool) { }");
    assert vec::is_empty(doc.topmod.resources());
}

#[test]
fn should_not_elide_resources_with_documented_args() {
    let doc = test::mk_doc("#[doc(args(a = \"drunk\"))]\
                            resource r(a: bool) { }");
    assert vec::is_not_empty(doc.topmod.resources());
}

fn fold_iface(
    fold: fold::fold<ctxt>,
    doc: doc::ifacedoc
) -> doc::ifacedoc {
    let doc = fold::default_seq_fold_iface(fold, doc);

    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none
        || methods_have_docs(doc.methods);
    ret doc;
}

fn methods_have_docs(docs: [doc::methoddoc]) -> bool {
    vec::foldl(false, docs) {|accum, doc|
        accum
            || doc.brief != none
            || doc.desc != none
            || args_have_docs(doc.args)
            || doc.return.desc != none
            || doc.failure != none
    }
}

#[test]
fn should_elide_undocumented_ifaces() {
    let doc = test::mk_doc("iface i { fn a(); }");
    assert vec::is_empty(doc.topmod.ifaces());
}

#[test]
fn should_not_elide_documented_ifaces() {
    let doc = test::mk_doc("#[doc = \"hey\"] iface i { fn a(); }");
    assert vec::is_not_empty(doc.topmod.ifaces());
}

#[test]
fn should_elide_ifaces_with_undocumented_args() {
    let doc = test::mk_doc("iface i { fn a(b: bool); }");
    assert vec::is_empty(doc.topmod.ifaces());
}

#[test]
fn should_not_elide_ifaces_with_documented_methods() {
    let doc = test::mk_doc("iface i { #[doc = \"hey\"] fn a(); }");
    assert vec::is_not_empty(doc.topmod.ifaces());
}

#[test]
fn should_not_elide_undocumented_iface_methods() {
    let doc = test::mk_doc("#[doc = \"hey\"] iface i { fn a(); }");
    assert vec::is_not_empty(doc.topmod.ifaces()[0].methods);
}

fn fold_impl(
    fold: fold::fold<ctxt>,
    doc: doc::impldoc
) -> doc::impldoc {
    let doc = fold::default_seq_fold_impl(fold, doc);

    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none
        || methods_have_docs(doc.methods);
    ret doc;
}

#[test]
fn should_elide_undocumented_impls() {
    let doc = test::mk_doc("impl i for int { fn a() { } }");
    assert vec::is_empty(doc.topmod.impls());
}

#[test]
fn should_not_elide_documented_impls() {
    let doc = test::mk_doc("#[doc = \"hey\"] impl i for int { fn a() { } }");
    assert vec::is_not_empty(doc.topmod.impls());
}

#[test]
fn should_not_elide_impls_with_documented_methods() {
    let doc = test::mk_doc("impl i for int { #[doc = \"hey\"] fn a() { } }");
    assert vec::is_not_empty(doc.topmod.impls());
}

#[test]
fn should_not_elide_undocumented_impl_methods() {
    let doc = test::mk_doc("#[doc = \"hey\"] impl i for int { fn a() { } }");
    assert vec::is_not_empty(doc.topmod.impls()[0].methods);
}

fn fold_type(
    fold: fold::fold<ctxt>,
    doc: doc::tydoc
) -> doc::tydoc {
    let doc = fold::default_seq_fold_type(fold, doc);

    fold.ctxt.have_docs =
        doc.brief() != none
        || doc.desc() != none;
    ret doc;
}

#[test]
fn should_elide_undocumented_types() {
    let doc = test::mk_doc("type t = int;");
    assert vec::is_empty(doc.topmod.types());
}

#[cfg(test)]
mod test {
    fn mk_doc(source: str) -> doc::cratedoc {
        astsrv::from_str(source) {|srv|
            let doc = extract::from_srv(srv, "");
            let doc = attr_pass::mk_pass()(srv, doc);
            run(srv, doc)
        }
    }
}
