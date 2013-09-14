// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use std::option;
use syntax::{ast, fold, attr};

type in_cfg_pred = @fn(attrs: &[ast::Attribute]) -> bool;

struct Context {
    in_cfg: in_cfg_pred
}

// Support conditional compilation by transforming the AST, stripping out
// any items that do not belong in the current configuration
pub fn strip_unconfigured_items(crate: @ast::Crate) -> @ast::Crate {
    do strip_items(crate) |attrs| {
        in_cfg(crate.config, attrs)
    }
}

pub fn strip_items(crate: &ast::Crate, in_cfg: in_cfg_pred)
    -> @ast::Crate {

    let ctxt = @Context { in_cfg: in_cfg };

    let precursor = @fold::AstFoldFns {
          fold_mod: |a,b| fold_mod(ctxt, a, b),
          fold_block: |a,b| fold_block(ctxt, a, b),
          fold_foreign_mod: |a,b| fold_foreign_mod(ctxt, a, b),
          fold_item_underscore: |a,b| fold_item_underscore(ctxt, a, b),
          .. *fold::default_ast_fold()
    };

    let fold = fold::make_fold(precursor);
    @fold.fold_crate(crate)
}

fn filter_item(cx: @Context, item: @ast::item) ->
   Option<@ast::item> {
    if item_in_cfg(cx, item) { option::Some(item) } else { option::None }
}

fn filter_view_item<'r>(cx: @Context, view_item: &'r ast::view_item)-> Option<&'r ast::view_item> {
    if view_item_in_cfg(cx, view_item) {
        option::Some(view_item)
    } else {
        option::None
    }
}

fn fold_mod(cx: @Context, m: &ast::_mod, fld: @fold::ast_fold) -> ast::_mod {
    let filtered_items = do  m.items.iter().filter_map |a| {
        filter_item(cx, *a).and_then(|x| fld.fold_item(x))
    }.collect();
    let filtered_view_items = do m.view_items.iter().filter_map |a| {
        do filter_view_item(cx, a).map_move |x| {
            fld.fold_view_item(x)
        }
    }.collect();
    ast::_mod {
        view_items: filtered_view_items,
        items: filtered_items
    }
}

fn filter_foreign_item(cx: @Context, item: @ast::foreign_item) ->
   Option<@ast::foreign_item> {
    if foreign_item_in_cfg(cx, item) {
        option::Some(item)
    } else { option::None }
}

fn fold_foreign_mod(
    cx: @Context,
    nm: &ast::foreign_mod,
    fld: @fold::ast_fold
) -> ast::foreign_mod {
    let filtered_items = nm.items.iter().filter_map(|a| filter_foreign_item(cx, *a)).collect();
    let filtered_view_items = do nm.view_items.iter().filter_map |a| {
        do filter_view_item(cx, a).map_move |x| {
            fld.fold_view_item(x)
        }
    }.collect();
    ast::foreign_mod {
        sort: nm.sort,
        abis: nm.abis,
        view_items: filtered_view_items,
        items: filtered_items
    }
}

fn fold_item_underscore(cx: @Context, item: &ast::item_,
                        fld: @fold::ast_fold) -> ast::item_ {
    let item = match *item {
        ast::item_impl(ref a, ref b, ref c, ref methods) => {
            let methods = methods.iter().filter(|m| method_in_cfg(cx, **m))
                .map(|x| *x).collect();
            ast::item_impl((*a).clone(), (*b).clone(), (*c).clone(), methods)
        }
        ast::item_trait(ref a, ref b, ref methods) => {
            let methods = methods.iter().filter(|m| trait_method_in_cfg(cx, *m) )
                .map(|x| (*x).clone()).collect();
            ast::item_trait((*a).clone(), (*b).clone(), methods)
        }
        ref item => (*item).clone(),
    };

    fold::noop_fold_item_underscore(&item, fld)
}

fn filter_stmt(cx: @Context, stmt: @ast::Stmt) ->
   Option<@ast::Stmt> {
    match stmt.node {
      ast::StmtDecl(decl, _) => {
        match decl.node {
          ast::DeclItem(item) => {
            if item_in_cfg(cx, item) {
                option::Some(stmt)
            } else { option::None }
          }
          _ => option::Some(stmt)
        }
      }
      _ => option::Some(stmt)
    }
}

fn fold_block(
    cx: @Context,
    b: &ast::Block,
    fld: @fold::ast_fold
) -> ast::Block {
    let resulting_stmts = do b.stmts.iter().filter_map |a| {
        filter_stmt(cx, *a).and_then(|stmt| fld.fold_stmt(stmt))
    }.collect();
    let filtered_view_items = do b.view_items.iter().filter_map |a| {
        filter_view_item(cx, a).map(|x| fld.fold_view_item(*x))
    }.collect();
    ast::Block {
        view_items: filtered_view_items,
        stmts: resulting_stmts,
        expr: b.expr.map(|x| fld.fold_expr(*x)),
        id: b.id,
        rules: b.rules,
        span: b.span,
    }
}

fn item_in_cfg(cx: @Context, item: @ast::item) -> bool {
    return (cx.in_cfg)(item.attrs);
}

fn foreign_item_in_cfg(cx: @Context, item: @ast::foreign_item) -> bool {
    return (cx.in_cfg)(item.attrs);
}

fn view_item_in_cfg(cx: @Context, item: &ast::view_item) -> bool {
    return (cx.in_cfg)(item.attrs);
}

fn method_in_cfg(cx: @Context, meth: @ast::method) -> bool {
    return (cx.in_cfg)(meth.attrs);
}

fn trait_method_in_cfg(cx: @Context, meth: &ast::trait_method) -> bool {
    match *meth {
        ast::required(ref meth) => (cx.in_cfg)(meth.attrs),
        ast::provided(@ref meth) => (cx.in_cfg)(meth.attrs)
    }
}

// Determine if an item should be translated in the current crate
// configuration based on the item's attributes
fn in_cfg(cfg: &[@ast::MetaItem], attrs: &[ast::Attribute]) -> bool {
    attr::test_cfg(cfg, attrs.iter().map(|x| *x))
}
