// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Prune things that are private

use core::prelude::*;

use astsrv;
use doc;
use fold::Fold;
use fold;
use pass::Pass;

use core::util;

pub fn mk_pass() -> Pass {
    Pass {
        name: ~"prune_private",
        f: run
    }
}

pub fn run(srv: astsrv::Srv, doc: doc::Doc) -> doc::Doc {
    let fold = Fold {
        ctxt: srv.clone(),
        fold_mod: fold_mod,
        .. fold::default_any_fold(srv)
    };
    (fold.fold_doc)(&fold, doc)
}

fn fold_mod(
    fold: &fold::Fold<astsrv::Srv>,
    doc: doc::ModDoc
) -> doc::ModDoc {
    let doc = fold::default_any_fold_mod(fold, doc);

    doc::ModDoc {
        items: doc.items.filtered(|ItemTag| {
            is_visible(fold.ctxt.clone(), ItemTag.item())
        }),
        .. doc
    }
}

fn is_visible(srv: astsrv::Srv, doc: doc::ItemDoc) -> bool {
    use syntax::ast_map;
    use syntax::ast;

    let id = doc.id;

    do astsrv::exec(srv) |ctxt| {
        match ctxt.ast_map.get(&id) {
            ast_map::node_item(item, _) => {
                match item.node {
                    ast::item_impl(_, Some(_), _, _) => {
                        // This is a trait implementation, make it visible
                        // NOTE: This is not quite right since this could be an impl
                        // of a private trait. We can't know that without running
                        // resolve though.
                        true
                    }
                    _ => {
                        // Otherwise just look at the visibility
                        item.vis == ast::public
                    }
                }
            }
            _ => util::unreachable()
        }
    }
}

#[test]
fn should_prune_items_without_pub_modifier() {
    let doc = test::mk_doc(~"mod a { }");
    fail_unless!(vec::is_empty(doc.cratemod().mods()));
}

#[test]
fn unless_they_are_trait_impls() {
    let doc = test::mk_doc(
        ~" \
          trait Foo { } \
          impl Foo for int { } \
          ");
    fail_unless!(!doc.cratemod().impls().is_empty());
}

#[cfg(test)]
pub mod test {
    use astsrv;
    use doc;
    use extract;
    use prune_private_pass::run;

    pub fn mk_doc(source: ~str) -> doc::Doc {
        do astsrv::from_str(copy source) |srv| {
            let doc = extract::from_srv(srv.clone(), ~"");
            run(srv.clone(), doc)
        }
    }
}

