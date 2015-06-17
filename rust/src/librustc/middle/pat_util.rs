// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::def::*;
use middle::ty;
use util::nodemap::FnvHashMap;

use syntax::ast;
use syntax::ast_util::walk_pat;
use syntax::codemap::{Span, DUMMY_SP};

pub type PatIdMap = FnvHashMap<ast::Ident, ast::NodeId>;

// This is used because same-named variables in alternative patterns need to
// use the NodeId of their namesake in the first pattern.
pub fn pat_id_map(dm: &DefMap, pat: &ast::Pat) -> PatIdMap {
    let mut map = FnvHashMap();
    pat_bindings(dm, pat, |_bm, p_id, _s, path1| {
        map.insert(path1.node, p_id);
    });
    map
}

pub fn pat_is_refutable(dm: &DefMap, pat: &ast::Pat) -> bool {
    match pat.node {
        ast::PatLit(_) | ast::PatRange(_, _) | ast::PatQPath(..) => true,
        ast::PatEnum(_, _) |
        ast::PatIdent(_, _, None) |
        ast::PatStruct(..) => {
            match dm.borrow().get(&pat.id).map(|d| d.full_def()) {
                Some(DefVariant(..)) => true,
                _ => false
            }
        }
        ast::PatVec(_, _, _) => true,
        _ => false
    }
}

pub fn pat_is_variant_or_struct(dm: &DefMap, pat: &ast::Pat) -> bool {
    match pat.node {
        ast::PatEnum(_, _) |
        ast::PatIdent(_, _, None) |
        ast::PatStruct(..) => {
            match dm.borrow().get(&pat.id).map(|d| d.full_def()) {
                Some(DefVariant(..)) | Some(DefStruct(..)) => true,
                _ => false
            }
        }
        _ => false
    }
}

pub fn pat_is_const(dm: &DefMap, pat: &ast::Pat) -> bool {
    match pat.node {
        ast::PatIdent(_, _, None) | ast::PatEnum(..) | ast::PatQPath(..) => {
            match dm.borrow().get(&pat.id).map(|d| d.full_def()) {
                Some(DefConst(..)) | Some(DefAssociatedConst(..)) => true,
                _ => false
            }
        }
        _ => false
    }
}

// Same as above, except that partially-resolved defs cause `false` to be
// returned instead of a panic.
pub fn pat_is_resolved_const(dm: &DefMap, pat: &ast::Pat) -> bool {
    match pat.node {
        ast::PatIdent(_, _, None) | ast::PatEnum(..) | ast::PatQPath(..) => {
            match dm.borrow().get(&pat.id)
                    .and_then(|d| if d.depth == 0 { Some(d.base_def) }
                                  else { None } ) {
                Some(DefConst(..)) | Some(DefAssociatedConst(..)) => true,
                _ => false
            }
        }
        _ => false
    }
}

pub fn pat_is_binding(dm: &DefMap, pat: &ast::Pat) -> bool {
    match pat.node {
        ast::PatIdent(..) => {
            !pat_is_variant_or_struct(dm, pat) &&
            !pat_is_const(dm, pat)
        }
        _ => false
    }
}

pub fn pat_is_binding_or_wild(dm: &DefMap, pat: &ast::Pat) -> bool {
    match pat.node {
        ast::PatIdent(..) => pat_is_binding(dm, pat),
        ast::PatWild(_) => true,
        _ => false
    }
}

/// Call `it` on every "binding" in a pattern, e.g., on `a` in
/// `match foo() { Some(a) => (), None => () }`
pub fn pat_bindings<I>(dm: &DefMap, pat: &ast::Pat, mut it: I) where
    I: FnMut(ast::BindingMode, ast::NodeId, Span, &ast::SpannedIdent),
{
    walk_pat(pat, |p| {
        match p.node {
          ast::PatIdent(binding_mode, ref pth, _) if pat_is_binding(dm, p) => {
            it(binding_mode, p.id, p.span, pth);
          }
          _ => {}
        }
        true
    });
}

/// Checks if the pattern contains any patterns that bind something to
/// an ident, e.g. `foo`, or `Foo(foo)` or `foo @ Bar(..)`.
pub fn pat_contains_bindings(dm: &DefMap, pat: &ast::Pat) -> bool {
    let mut contains_bindings = false;
    walk_pat(pat, |p| {
        if pat_is_binding(dm, p) {
            contains_bindings = true;
            false // there's at least one binding, can short circuit now.
        } else {
            true
        }
    });
    contains_bindings
}

/// Checks if the pattern contains any `ref` or `ref mut` bindings,
/// and if yes wether its containing mutable ones or just immutables ones.
pub fn pat_contains_ref_binding(dm: &DefMap, pat: &ast::Pat) -> Option<ast::Mutability> {
    let mut result = None;
    pat_bindings(dm, pat, |mode, _, _, _| {
        match mode {
            ast::BindingMode::BindByRef(m) => {
                // Pick Mutable as maximum
                match result {
                    None | Some(ast::MutImmutable) => result = Some(m),
                    _ => (),
                }
            }
            ast::BindingMode::BindByValue(_) => { }
        }
    });
    result
}

/// Checks if the patterns for this arm contain any `ref` or `ref mut`
/// bindings, and if yes wether its containing mutable ones or just immutables ones.
pub fn arm_contains_ref_binding(dm: &DefMap, arm: &ast::Arm) -> Option<ast::Mutability> {
    arm.pats.iter()
            .filter_map(|pat| pat_contains_ref_binding(dm, pat))
            .max_by(|m| match *m {
                ast::MutMutable => 1,
                ast::MutImmutable => 0,
            })
}

/// Checks if the pattern contains any patterns that bind something to
/// an ident or wildcard, e.g. `foo`, or `Foo(_)`, `foo @ Bar(..)`,
pub fn pat_contains_bindings_or_wild(dm: &DefMap, pat: &ast::Pat) -> bool {
    let mut contains_bindings = false;
    walk_pat(pat, |p| {
        if pat_is_binding_or_wild(dm, p) {
            contains_bindings = true;
            false // there's at least one binding/wildcard, can short circuit now.
        } else {
            true
        }
    });
    contains_bindings
}

pub fn simple_identifier<'a>(pat: &'a ast::Pat) -> Option<&'a ast::Ident> {
    match pat.node {
        ast::PatIdent(ast::BindByValue(_), ref path1, None) => {
            Some(&path1.node)
        }
        _ => {
            None
        }
    }
}

pub fn def_to_path(tcx: &ty::ctxt, id: ast::DefId) -> ast::Path {
    ty::with_path(tcx, id, |path| ast::Path {
        global: false,
        segments: path.last().map(|elem| ast::PathSegment {
            identifier: ast::Ident::new(elem.name()),
            parameters: ast::PathParameters::none(),
        }).into_iter().collect(),
        span: DUMMY_SP,
    })
}

/// Return variants that are necessary to exist for the pattern to match.
pub fn necessary_variants(dm: &DefMap, pat: &ast::Pat) -> Vec<ast::NodeId> {
    let mut variants = vec![];
    walk_pat(pat, |p| {
        match p.node {
            ast::PatEnum(_, _) |
            ast::PatIdent(_, _, None) |
            ast::PatStruct(..) => {
                match dm.borrow().get(&p.id) {
                    Some(&PathResolution { base_def: DefVariant(_, id, _), .. }) => {
                        variants.push(id.node);
                    }
                    _ => ()
                }
            }
            _ => ()
        }
        true
    });
    variants.sort();
    variants.dedup();
    variants
}
