// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast;
use attr;
use codemap::DUMMY_SP;
use codemap;
use fold::Folder;
use fold;
use parse::token::InternedString;
use parse::token::special_idents;
use parse::token;
use ptr::P;
use util::small_vector::SmallVector;

use std::mem;

pub fn maybe_inject_crates_ref(krate: ast::Crate, alt_std_name: Option<String>)
                               -> ast::Crate {
    if use_std(&krate) {
        inject_crates_ref(krate, alt_std_name)
    } else {
        krate
    }
}

pub fn maybe_inject_prelude(krate: ast::Crate) -> ast::Crate {
    if use_std(&krate) {
        inject_prelude(krate)
    } else {
        krate
    }
}

fn use_std(krate: &ast::Crate) -> bool {
    !attr::contains_name(krate.attrs.as_slice(), "no_std")
}

fn no_prelude(attrs: &[ast::Attribute]) -> bool {
    attr::contains_name(attrs, "no_implicit_prelude")
}

struct StandardLibraryInjector<'a> {
    alt_std_name: Option<String>,
}

impl<'a> fold::Folder for StandardLibraryInjector<'a> {
    fn fold_crate(&mut self, mut krate: ast::Crate) -> ast::Crate {

        // The name to use in `extern crate "name" as std;`
        let actual_crate_name = match self.alt_std_name {
            Some(ref s) => token::intern_and_get_ident(s.as_slice()),
            None => token::intern_and_get_ident("std"),
        };

        let mut vis = vec!(ast::ViewItem {
            node: ast::ViewItemExternCrate(token::str_to_ident("std"),
                                           Some((actual_crate_name, ast::CookedStr)),
                                           ast::DUMMY_NODE_ID),
            attrs: vec!(
                attr::mk_attr_outer(attr::mk_attr_id(), attr::mk_list_item(
                        InternedString::new("phase"),
                        vec!(
                            attr::mk_word_item(InternedString::new("plugin")),
                            attr::mk_word_item(InternedString::new("link")
                        ))))),
            vis: ast::Inherited,
            span: DUMMY_SP
        });

        // `extern crate` must be precede `use` items
        mem::swap(&mut vis, &mut krate.module.view_items);
        krate.module.view_items.extend(vis.into_iter());

        // don't add #![no_std] here, that will block the prelude injection later.
        // Add it during the prelude injection instead.

        // Add #![feature(phase)] here, because we use #[phase] on extern crate std.
        let feat_phase_attr = attr::mk_attr_inner(attr::mk_attr_id(),
                                                  attr::mk_list_item(
                                  InternedString::new("feature"),
                                  vec![attr::mk_word_item(InternedString::new("phase"))],
                              ));
        // std_inject runs after feature checking so manually mark this attr
        attr::mark_used(&feat_phase_attr);
        krate.attrs.push(feat_phase_attr);

        krate
    }
}

fn inject_crates_ref(krate: ast::Crate, alt_std_name: Option<String>) -> ast::Crate {
    let mut fold = StandardLibraryInjector {
        alt_std_name: alt_std_name,
    };
    fold.fold_crate(krate)
}

struct PreludeInjector<'a>;


impl<'a> fold::Folder for PreludeInjector<'a> {
    fn fold_crate(&mut self, mut krate: ast::Crate) -> ast::Crate {
        // Add #![no_std] here, so we don't re-inject when compiling pretty-printed source.
        // This must happen here and not in StandardLibraryInjector because this
        // fold happens second.

        let no_std_attr = attr::mk_attr_inner(attr::mk_attr_id(),
                                              attr::mk_word_item(InternedString::new("no_std")));
        // std_inject runs after feature checking so manually mark this attr
        attr::mark_used(&no_std_attr);
        krate.attrs.push(no_std_attr);

        if !no_prelude(krate.attrs.as_slice()) {
            // only add `use std::prelude::*;` if there wasn't a
            // `#![no_implicit_prelude]` at the crate level.
            // fold_mod() will insert glob path.
            let globs_attr = attr::mk_attr_inner(attr::mk_attr_id(),
                                                 attr::mk_list_item(
                InternedString::new("feature"),
                vec!(
                    attr::mk_word_item(InternedString::new("globs")),
                )));
            // std_inject runs after feature checking so manually mark this attr
            attr::mark_used(&globs_attr);
            krate.attrs.push(globs_attr);

            krate.module = self.fold_mod(krate.module);
        }
        krate
    }

    fn fold_item(&mut self, item: P<ast::Item>) -> SmallVector<P<ast::Item>> {
        if !no_prelude(item.attrs.as_slice()) {
            // only recur if there wasn't `#![no_implicit_prelude]`
            // on this item, i.e. this means that the prelude is not
            // implicitly imported though the whole subtree
            fold::noop_fold_item(item, self)
        } else {
            SmallVector::one(item)
        }
    }

    fn fold_mod(&mut self, ast::Mod {inner, view_items, items}: ast::Mod) -> ast::Mod {
        let prelude_path = ast::Path {
            span: DUMMY_SP,
            global: false,
            segments: vec!(
                ast::PathSegment {
                    identifier: token::str_to_ident("std"),
                    parameters: ast::PathParameters::none(),
                },
                ast::PathSegment {
                    identifier: token::str_to_ident("prelude"),
                    parameters: ast::PathParameters::none(),
                }),
        };

        let (crates, uses) = view_items.partitioned(|x| {
            match x.node {
                ast::ViewItemExternCrate(..) => true,
                _ => false,
            }
        });

        // add prelude after any `extern crate` but before any `use`
        let mut view_items = crates;
        let vp = P(codemap::dummy_spanned(ast::ViewPathGlob(prelude_path, ast::DUMMY_NODE_ID)));
        view_items.push(ast::ViewItem {
            node: ast::ViewItemUse(vp),
            attrs: vec![ast::Attribute {
                span: DUMMY_SP,
                node: ast::Attribute_ {
                    id: attr::mk_attr_id(),
                    style: ast::AttrOuter,
                    value: P(ast::MetaItem {
                        span: DUMMY_SP,
                        node: ast::MetaWord(token::get_name(
                                special_idents::prelude_import.name)),
                    }),
                    is_sugared_doc: false,
                },
            }],
            vis: ast::Inherited,
            span: DUMMY_SP,
        });
        view_items.extend(uses.into_iter());

        fold::noop_fold_mod(ast::Mod {
            inner: inner,
            view_items: view_items,
            items: items
        }, self)
    }
}

fn inject_prelude(krate: ast::Crate) -> ast::Crate {
    let mut fold = PreludeInjector;
    fold.fold_crate(krate)
}
