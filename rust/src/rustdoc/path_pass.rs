//! Records the full path to items

import doc::item_utils;
import syntax::ast;

export mk_pass;

fn mk_pass() -> pass {
    {
        name: ~"path",
        f: run
    }
}

type ctxt = {
    srv: astsrv::srv,
    mut path: ~[~str]
};

#[allow(non_implicitly_copyable_typarams)]
fn run(srv: astsrv::srv, doc: doc::doc) -> doc::doc {
    let ctxt = {
        srv: srv,
        mut path: ~[]
    };
    let fold = fold::fold({
        fold_item: fold_item,
        fold_mod: fold_mod,
        fold_nmod: fold_nmod,
        .. *fold::default_any_fold(ctxt)
    });
    fold.fold_doc(fold, doc)
}

fn fold_item(fold: fold::fold<ctxt>, doc: doc::itemdoc) -> doc::itemdoc {
    {
        path: fold.ctxt.path,
        .. doc
    }
}

#[allow(non_implicitly_copyable_typarams)]
fn fold_mod(fold: fold::fold<ctxt>, doc: doc::moddoc) -> doc::moddoc {
    let is_topmod = doc.id() == ast::crate_node_id;

    if !is_topmod { vec::push(fold.ctxt.path, doc.name()); }
    let doc = fold::default_any_fold_mod(fold, doc);
    if !is_topmod { vec::pop(fold.ctxt.path); }

    doc::moddoc_({
        item: fold.fold_item(fold, doc.item),
        .. *doc
    })
}

fn fold_nmod(fold: fold::fold<ctxt>, doc: doc::nmoddoc) -> doc::nmoddoc {
    vec::push(fold.ctxt.path, doc.name());
    let doc = fold::default_seq_fold_nmod(fold, doc);
    vec::pop(fold.ctxt.path);

    {
        item: fold.fold_item(fold, doc.item),
        .. doc
    }
}

#[test]
fn should_record_mod_paths() {
    let source = ~"mod a { mod b { mod c { } } mod d { mod e { } } }";
    do astsrv::from_str(source) |srv| {
        let doc = extract::from_srv(srv, ~"");
        let doc = run(srv, doc);
        assert doc.cratemod().mods()[0].mods()[0].mods()[0].path()
            == ~[~"a", ~"b"];
        assert doc.cratemod().mods()[0].mods()[1].mods()[0].path()
            == ~[~"a", ~"d"];
    }
}

#[test]
fn should_record_fn_paths() {
    let source = ~"mod a { fn b() { } }";
    do astsrv::from_str(source) |srv| {
        let doc = extract::from_srv(srv, ~"");
        let doc = run(srv, doc);
        assert doc.cratemod().mods()[0].fns()[0].path() == ~[~"a"];
    }
}

#[test]
fn should_record_foreign_mod_paths() {
    let source = ~"mod a { extern mod b { } }";
    do astsrv::from_str(source) |srv| {
        let doc = extract::from_srv(srv, ~"");
        let doc = run(srv, doc);
        assert doc.cratemod().mods()[0].nmods()[0].path() == ~[~"a"];
    }
}

#[test]
fn should_record_foreign_fn_paths() {
    let source = ~"extern mod a { fn b(); }";
    do astsrv::from_str(source) |srv| {
        let doc = extract::from_srv(srv, ~"");
        let doc = run(srv, doc);
        assert doc.cratemod().nmods()[0].fns[0].path() == ~[~"a"];
    }
}
