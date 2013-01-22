// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use ast::*;
use ast;
use ast_util::{path_to_ident, stmt_id};
use ast_util;
use attr;
use codemap;
use diagnostic::span_handler;
use parse::token::ident_interner;
use print::pprust;
use visit;

use core::cmp;
use core::either;
use core::str;
use core::vec;
use std::map::HashMap;
use std::map;
use std;

enum path_elt {
    path_mod(ident),
    path_name(ident)
}

impl path_elt : cmp::Eq {
    pure fn eq(&self, other: &path_elt) -> bool {
        match (*self) {
            path_mod(e0a) => {
                match (*other) {
                    path_mod(e0b) => e0a == e0b,
                    _ => false
                }
            }
            path_name(e0a) => {
                match (*other) {
                    path_name(e0b) => e0a == e0b,
                    _ => false
                }
            }
        }
    }
    pure fn ne(&self, other: &path_elt) -> bool { !(*self).eq(other) }
}

type path = ~[path_elt];

fn path_to_str_with_sep(p: &[path_elt], sep: ~str, itr: @ident_interner)
    -> ~str {
    let strs = do p.map |e| {
        match *e {
          path_mod(s) => *itr.get(s),
          path_name(s) => *itr.get(s)
        }
    };
    str::connect(strs, sep)
}

fn path_ident_to_str(p: path, i: ident, itr: @ident_interner) -> ~str {
    if vec::is_empty(p) {
        //FIXME /* FIXME (#2543) */ copy *i
        *itr.get(i)
    } else {
        fmt!("%s::%s", path_to_str(p, itr), *itr.get(i))
    }
}

fn path_to_str(p: &[path_elt], itr: @ident_interner) -> ~str {
    path_to_str_with_sep(p, ~"::", itr)
}

fn path_elt_to_str(pe: path_elt, itr: @ident_interner) -> ~str {
    match pe {
        path_mod(s) => *itr.get(s),
        path_name(s) => *itr.get(s)
    }
}

enum ast_node {
    node_item(@item, @path),
    node_foreign_item(@foreign_item, foreign_abi, @path),
    node_trait_method(@trait_method, def_id /* trait did */,
                      @path /* path to the trait */),
    node_method(@method, def_id /* impl did */, @path /* path to the impl */),
    node_variant(variant, @item, @path),
    node_expr(@expr),
    node_stmt(@stmt),
    node_export(@view_path, @path),
    // Locals are numbered, because the alias analysis needs to know in which
    // order they are introduced.
    node_arg(arg, uint),
    node_local(uint),
    // Destructor for a struct
    node_dtor(~[ty_param], @struct_dtor, def_id, @path),
    node_block(blk),
    node_struct_ctor(@struct_def, @item, @path),
}

type map = std::map::HashMap<node_id, ast_node>;
struct ctx {
    map: map,
    mut path: path,
    mut local_id: uint,
    diag: span_handler,
}
type vt = visit::vt<ctx>;

fn extend(cx: ctx, +elt: ident) -> @path {
    @(vec::append(cx.path, ~[path_name(elt)]))
}

fn mk_ast_map_visitor() -> vt {
    return visit::mk_vt(@visit::Visitor {
        visit_item: map_item,
        visit_expr: map_expr,
        visit_stmt: map_stmt,
        visit_fn: map_fn,
        visit_local: map_local,
        visit_arm: map_arm,
        visit_view_item: map_view_item,
        visit_block: map_block,
        .. *visit::default_visitor()
    });
}

fn map_crate(diag: span_handler, c: crate) -> map {
    let cx = ctx {
        map: std::map::HashMap(),
        mut path: ~[],
        mut local_id: 0u,
        diag: diag,
    };
    visit::visit_crate(c, cx, mk_ast_map_visitor());
    cx.map
}

// Used for items loaded from external crate that are being inlined into this
// crate.  The `path` should be the path to the item but should not include
// the item itself.
fn map_decoded_item(diag: span_handler,
                    map: map, +path: path, ii: inlined_item) {
    // I believe it is ok for the local IDs of inlined items from other crates
    // to overlap with the local ids from this crate, so just generate the ids
    // starting from 0.  (In particular, I think these ids are only used in
    // alias analysis, which we will not be running on the inlined items, and
    // even if we did I think it only needs an ordering between local
    // variables that are simultaneously in scope).
    let cx = ctx {
        map: map,
        mut path: /* FIXME (#2543) */ copy path,
        mut local_id: 0u,
        diag: diag,
    };
    let v = mk_ast_map_visitor();

    // methods get added to the AST map when their impl is visited.  Since we
    // don't decode and instantiate the impl, but just the method, we have to
    // add it to the table now:
    match ii {
      ii_item(*) | ii_dtor(*) => { /* fallthrough */ }
      ii_foreign(i) => {
        cx.map.insert(i.id, node_foreign_item(i, foreign_abi_rust_intrinsic,
                                             @path));
      }
      ii_method(impl_did, m) => {
        map_method(impl_did, @path, m, cx);
      }
    }

    // visit the item / method contents and add those to the map:
    ii.accept(cx, v);
}

fn map_fn(fk: visit::fn_kind, decl: fn_decl, body: blk,
          sp: codemap::span, id: node_id, cx: ctx, v: vt) {
    for decl.inputs.each |a| {
        cx.map.insert(a.id,
                      node_arg(/* FIXME (#2543) */
                          copy *a, cx.local_id));
        cx.local_id += 1u;
    }
    match fk {
        visit::fk_dtor(tps, ref attrs, self_id, parent_id) => {
            let dt = @spanned {
                node: ast::struct_dtor_ {
                    id: id,
                    attrs: (*attrs),
                    self_id: self_id,
                    body: /* FIXME (#2543) */ copy body,
                },
                span: sp,
            };
            cx.map.insert(id, node_dtor(/* FIXME (#2543) */ copy tps, dt,
                                        parent_id,
                                        @/* FIXME (#2543) */ copy cx.path));
      }
      _ => ()
    }
    visit::visit_fn(fk, decl, body, sp, id, cx, v);
}

fn map_block(b: blk, cx: ctx, v: vt) {
    cx.map.insert(b.node.id, node_block(/* FIXME (#2543) */ copy b));
    visit::visit_block(b, cx, v);
}

fn number_pat(cx: ctx, pat: @pat) {
    do ast_util::walk_pat(pat) |p| {
        match p.node {
          pat_ident(*) => {
            cx.map.insert(p.id, node_local(cx.local_id));
            cx.local_id += 1u;
          }
          _ => ()
        }
    };
}

fn map_local(loc: @local, cx: ctx, v: vt) {
    number_pat(cx, loc.node.pat);
    visit::visit_local(loc, cx, v);
}

fn map_arm(arm: arm, cx: ctx, v: vt) {
    number_pat(cx, arm.pats[0]);
    visit::visit_arm(arm, cx, v);
}

fn map_method(impl_did: def_id, impl_path: @path,
              m: @method, cx: ctx) {
    cx.map.insert(m.id, node_method(m, impl_did, impl_path));
    cx.map.insert(m.self_id, node_local(cx.local_id));
    cx.local_id += 1u;
}

fn map_item(i: @item, cx: ctx, v: vt) {
    let item_path = @/* FIXME (#2543) */ copy cx.path;
    cx.map.insert(i.id, node_item(i, item_path));
    match i.node {
      item_impl(_, _, _, ms) => {
        let impl_did = ast_util::local_def(i.id);
        for ms.each |m| {
            map_method(impl_did, extend(cx, i.ident), *m, cx);
        }
      }
      item_enum(ref enum_definition, _) => {
        for (*enum_definition).variants.each |v| {
            cx.map.insert(v.node.id, node_variant(
                /* FIXME (#2543) */ copy *v, i,
                extend(cx, i.ident)));
        }
      }
      item_foreign_mod(nm) => {
        let abi = match attr::foreign_abi(i.attrs) {
          either::Left(ref msg) => cx.diag.span_fatal(i.span, (*msg)),
          either::Right(abi) => abi
        };
        for nm.items.each |nitem| {
            cx.map.insert(nitem.id,
                          node_foreign_item(*nitem, abi,
                                           /* FIXME (#2543) */
                                            if nm.sort == ast::named {
                                                extend(cx, i.ident)
                                            }
                                            else {
                                                /* Anonymous extern mods go
                                                in the parent scope */
                                                @copy cx.path
                                            }));
        }
      }
      item_struct(struct_def, _) => {
        map_struct_def(struct_def, node_item(i, item_path), i.ident, cx,
                       v);
      }
      item_trait(_, traits, ref methods) => {
        for traits.each |p| {
            cx.map.insert(p.ref_id, node_item(i, item_path));
        }
        for (*methods).each |tm| {
            let id = ast_util::trait_method_to_ty_method(*tm).id;
            let d_id = ast_util::local_def(i.id);
            cx.map.insert(id, node_trait_method(@*tm, d_id, item_path));
        }
      }
      _ => ()
    }
    match i.node {
      item_mod(_) | item_foreign_mod(_) => {
        cx.path.push(path_mod(i.ident));
      }
      _ => cx.path.push(path_name(i.ident))
    }
    visit::visit_item(i, cx, v);
    cx.path.pop();
}

fn map_struct_def(struct_def: @ast::struct_def, parent_node: ast_node,
                  ident: ast::ident, cx: ctx, _v: vt) {
    let p = extend(cx, ident);
    // If this is a tuple-like struct, register the constructor.
    match struct_def.ctor_id {
        None => {}
        Some(ctor_id) => {
            match parent_node {
                node_item(item, _) => {
                    cx.map.insert(ctor_id,
                                  node_struct_ctor(struct_def, item, p));
                }
                _ => fail ~"struct def parent wasn't an item"
            }
        }
    }
}

fn map_view_item(vi: @view_item, cx: ctx, _v: vt) {
    match vi.node {
      view_item_export(vps) => for vps.each |vp| {
        let (id, name) = match vp.node {
          view_path_simple(nm, _, _, id) => {
            (id, /* FIXME (#2543) */ copy nm)
          }
          view_path_glob(pth, id) | view_path_list(pth, _, id) => {
            (id, path_to_ident(pth))
          }
        };
        cx.map.insert(id, node_export(*vp, extend(cx, name)));
      },
      _ => ()
    }
}

fn map_expr(ex: @expr, cx: ctx, v: vt) {
    cx.map.insert(ex.id, node_expr(ex));
    visit::visit_expr(ex, cx, v);
}

fn map_stmt(stmt: @stmt, cx: ctx, v: vt) {
    cx.map.insert(stmt_id(*stmt), node_stmt(stmt));
    visit::visit_stmt(stmt, cx, v);
}

fn node_id_to_str(map: map, id: node_id, itr: @ident_interner) -> ~str {
    match map.find(id) {
      None => {
        fmt!("unknown node (id=%d)", id)
      }
      Some(node_item(item, path)) => {
        let path_str = path_ident_to_str(*path, item.ident, itr);
        let item_str = match item.node {
          item_const(*) => ~"const",
          item_fn(*) => ~"fn",
          item_mod(*) => ~"mod",
          item_foreign_mod(*) => ~"foreign mod",
          item_ty(*) => ~"ty",
          item_enum(*) => ~"enum",
          item_struct(*) => ~"struct",
          item_trait(*) => ~"trait",
          item_impl(*) => ~"impl",
          item_mac(*) => ~"macro"
        };
        fmt!("%s %s (id=%?)", item_str, path_str, id)
      }
      Some(node_foreign_item(item, abi, path)) => {
        fmt!("foreign item %s with abi %? (id=%?)",
             path_ident_to_str(*path, item.ident, itr), abi, id)
      }
      Some(node_method(m, _, path)) => {
        fmt!("method %s in %s (id=%?)",
             *itr.get(m.ident), path_to_str(*path, itr), id)
      }
      Some(node_trait_method(tm, _, path)) => {
        let m = ast_util::trait_method_to_ty_method(*tm);
        fmt!("method %s in %s (id=%?)",
             *itr.get(m.ident), path_to_str(*path, itr), id)
      }
      Some(node_variant(ref variant, _, path)) => {
        fmt!("variant %s in %s (id=%?)",
             *itr.get((*variant).node.name), path_to_str(*path, itr), id)
      }
      Some(node_expr(expr)) => {
        fmt!("expr %s (id=%?)", pprust::expr_to_str(expr, itr), id)
      }
      Some(node_stmt(stmt)) => {
        fmt!("stmt %s (id=%?)",
             pprust::stmt_to_str(*stmt, itr), id)
      }
      // FIXMEs are as per #2410
      Some(node_export(_, path)) => {
        fmt!("export %s (id=%?)", // add more info here
             path_to_str(*path, itr), id)
      }
      Some(node_arg(_, _)) => { // add more info here
        fmt!("arg (id=%?)", id)
      }
      Some(node_local(_)) => { // add more info here
        fmt!("local (id=%?)", id)
      }
      Some(node_dtor(*)) => { // add more info here
        fmt!("node_dtor (id=%?)", id)
      }
      Some(node_block(_)) => {
        fmt!("block")
      }
      Some(node_struct_ctor(*)) => {
        fmt!("struct_ctor")
      }
    }
}

fn node_item_query<Result>(items: map, id: node_id,
                           query: fn(@item) -> Result,
                           error_msg: ~str) -> Result {
    match items.find(id) {
        Some(node_item(it, _)) => query(it),
        _ => fail(error_msg)
    }
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
