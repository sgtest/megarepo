#[doc = "Records the full path to items"];

export mk_pass;

fn mk_pass() -> pass { run }

type ctxt = {
    srv: astsrv::srv,
    mutable path: [str]
};

fn run(srv: astsrv::srv, doc: doc::cratedoc) -> doc::cratedoc {
    let ctxt = {
        srv: srv,
        mutable path: []
    };
    let fold = fold::fold({
        fold_item: fold_item,
        fold_mod: fold_mod,
        fold_nmod: fold_nmod
        with *fold::default_any_fold(ctxt)
    });
    fold.fold_crate(fold, doc)
}

fn fold_item(fold: fold::fold<ctxt>, doc: doc::itemdoc) -> doc::itemdoc {
    {
        path: fold.ctxt.path
        with doc
    }
}

fn fold_mod(fold: fold::fold<ctxt>, doc: doc::moddoc) -> doc::moddoc {
    let is_topmod = doc.id() == rustc::syntax::ast::crate_node_id;

    if !is_topmod { vec::push(fold.ctxt.path, doc.name()); }
    let doc = fold::default_any_fold_mod(fold, doc);
    if !is_topmod { vec::pop(fold.ctxt.path); }

    {
        item: fold.fold_item(fold, doc.item)
        with doc
    }
}

fn fold_nmod(fold: fold::fold<ctxt>, doc: doc::nmoddoc) -> doc::nmoddoc {
    vec::push(fold.ctxt.path, doc.name());
    let doc = fold::default_seq_fold_nmod(fold, doc);
    vec::pop(fold.ctxt.path);
    ret doc;
}

#[test]
fn should_record_mod_paths() {
    let source = "mod a { mod b { mod c { } } mod d { mod e { } } }";
    astsrv::from_str(source) {|srv|
        let doc = extract::from_srv(srv, "");
        let doc = run(srv, doc);
        assert doc.topmod.mods()[0].mods()[0].mods()[0].path() == ["a", "b"];
        assert doc.topmod.mods()[0].mods()[1].mods()[0].path() == ["a", "d"];
    }
}

#[test]
fn should_record_fn_paths() {
    let source = "mod a { fn b() { } }";
    astsrv::from_str(source) {|srv|
        let doc = extract::from_srv(srv, "");
        let doc = run(srv, doc);
        assert doc.topmod.mods()[0].fns()[0].path() == ["a"];
    }
}

#[test]
fn should_record_native_fn_paths() {
    let source = "native mod a { fn b(); }";
    astsrv::from_str(source) {|srv|
        let doc = extract::from_srv(srv, "");
        let doc = run(srv, doc);
        assert doc.topmod.nmods()[0].fns[0].path() == ["a"];
    }
}