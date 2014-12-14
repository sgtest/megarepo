// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Rust AST Visitor. Extracts useful information and massages it into a form
//! usable for clean

use std::collections::HashSet;

use syntax::abi;
use syntax::ast;
use syntax::ast_util;
use syntax::ast_map;
use syntax::attr;
use syntax::attr::AttrMetaMethods;
use syntax::codemap::Span;
use syntax::ptr::P;

use rustc::middle::stability;

use core;
use doctree::*;

// looks to me like the first two of these are actually
// output parameters, maybe only mutated once; perhaps
// better simply to have the visit method return a tuple
// containing them?

// also, is there some reason that this doesn't use the 'visit'
// framework from syntax?

pub struct RustdocVisitor<'a, 'tcx: 'a> {
    pub module: Module,
    pub attrs: Vec<ast::Attribute>,
    pub cx: &'a core::DocContext<'tcx>,
    pub analysis: Option<&'a core::CrateAnalysis>,
    view_item_stack: HashSet<ast::NodeId>,
}

impl<'a, 'tcx> RustdocVisitor<'a, 'tcx> {
    pub fn new(cx: &'a core::DocContext<'tcx>,
               analysis: Option<&'a core::CrateAnalysis>) -> RustdocVisitor<'a, 'tcx> {
        // If the root is reexported, terminate all recursion.
        let mut stack = HashSet::new();
        stack.insert(ast::CRATE_NODE_ID);
        RustdocVisitor {
            module: Module::new(None),
            attrs: Vec::new(),
            cx: cx,
            analysis: analysis,
            view_item_stack: stack,
        }
    }

    fn stability(&self, id: ast::NodeId) -> Option<attr::Stability> {
        self.cx.tcx_opt().and_then(|tcx| stability::lookup(tcx, ast_util::local_def(id)))
    }

    pub fn visit(&mut self, krate: &ast::Crate) {
        self.attrs = krate.attrs.clone();

        self.module = self.visit_mod_contents(krate.span,
                                              krate.attrs.clone(),
                                              ast::Public,
                                              ast::CRATE_NODE_ID,
                                              &krate.module,
                                              None);
        // attach the crate's exported macros to the top-level module:
        self.module.macros = krate.exported_macros.iter()
            .map(|it| self.visit_macro(&**it)).collect();
        self.module.is_crate = true;
    }

    pub fn visit_struct_def(&mut self, item: &ast::Item,
                            name: ast::Ident, sd: &ast::StructDef,
                            generics: &ast::Generics) -> Struct {
        debug!("Visiting struct");
        let struct_type = struct_type_from_def(&*sd);
        Struct {
            id: item.id,
            struct_type: struct_type,
            name: name,
            vis: item.vis,
            stab: self.stability(item.id),
            attrs: item.attrs.clone(),
            generics: generics.clone(),
            fields: sd.fields.clone(),
            whence: item.span
        }
    }

    pub fn visit_enum_def(&mut self, it: &ast::Item,
                          name: ast::Ident, def: &ast::EnumDef,
                          params: &ast::Generics) -> Enum {
        debug!("Visiting enum");
        Enum {
            name: name,
            variants: def.variants.iter().map(|v| Variant {
                name: v.node.name,
                attrs: v.node.attrs.clone(),
                vis: v.node.vis,
                stab: self.stability(v.node.id),
                id: v.node.id,
                kind: v.node.kind.clone(),
                whence: v.span,
            }).collect(),
            vis: it.vis,
            stab: self.stability(it.id),
            generics: params.clone(),
            attrs: it.attrs.clone(),
            id: it.id,
            whence: it.span,
        }
    }

    pub fn visit_fn(&mut self, item: &ast::Item,
                    name: ast::Ident, fd: &ast::FnDecl,
                    unsafety: &ast::Unsafety, _abi: &abi::Abi,
                    gen: &ast::Generics) -> Function {
        debug!("Visiting fn");
        Function {
            id: item.id,
            vis: item.vis,
            stab: self.stability(item.id),
            attrs: item.attrs.clone(),
            decl: fd.clone(),
            name: name,
            whence: item.span,
            generics: gen.clone(),
            unsafety: *unsafety,
        }
    }

    pub fn visit_mod_contents(&mut self, span: Span, attrs: Vec<ast::Attribute> ,
                              vis: ast::Visibility, id: ast::NodeId,
                              m: &ast::Mod,
                              name: Option<ast::Ident>) -> Module {
        let mut om = Module::new(name);
        for item in m.view_items.iter() {
            self.visit_view_item(item, &mut om);
        }
        om.where_outer = span;
        om.where_inner = m.inner;
        om.attrs = attrs;
        om.vis = vis;
        om.stab = self.stability(id);
        om.id = id;
        for i in m.items.iter() {
            self.visit_item(&**i, None, &mut om);
        }
        om
    }

    pub fn visit_view_item(&mut self, item: &ast::ViewItem, om: &mut Module) {
        if item.vis != ast::Public {
            return om.view_items.push(item.clone());
        }
        let please_inline = item.attrs.iter().any(|item| {
            match item.meta_item_list() {
                Some(list) => {
                    list.iter().any(|i| i.name().get() == "inline")
                }
                None => false,
            }
        });
        let item = match item.node {
            ast::ViewItemUse(ref vpath) => {
                match self.visit_view_path(&**vpath, om, please_inline) {
                    None => return,
                    Some(path) => {
                        ast::ViewItem {
                            node: ast::ViewItemUse(path),
                            .. item.clone()
                        }
                    }
                }
            }
            ast::ViewItemExternCrate(..) => item.clone()
        };
        om.view_items.push(item);
    }

    fn visit_view_path(&mut self, path: &ast::ViewPath,
                       om: &mut Module,
                       please_inline: bool) -> Option<P<ast::ViewPath>> {
        match path.node {
            ast::ViewPathSimple(dst, _, id) => {
                if self.resolve_id(id, Some(dst), false, om, please_inline) {
                    return None
                }
            }
            ast::ViewPathList(ref p, ref paths, ref b) => {
                let mut mine = Vec::new();
                for path in paths.iter() {
                    if !self.resolve_id(path.node.id(), None, false, om,
                                        please_inline) {
                        mine.push(path.clone());
                    }
                }

                if mine.len() == 0 { return None }
                return Some(P(::syntax::codemap::Spanned {
                    node: ast::ViewPathList(p.clone(), mine, b.clone()),
                    span: path.span,
                }))
            }

            // these are feature gated anyway
            ast::ViewPathGlob(_, id) => {
                if self.resolve_id(id, None, true, om, please_inline) {
                    return None
                }
            }
        }
        Some(P(path.clone()))
    }

    fn resolve_id(&mut self, id: ast::NodeId, renamed: Option<ast::Ident>,
                  glob: bool, om: &mut Module, please_inline: bool) -> bool {
        let tcx = match self.cx.tcx_opt() {
            Some(tcx) => tcx,
            None => return false
        };
        let def = (*tcx.def_map.borrow())[id].def_id();
        if !ast_util::is_local(def) { return false }
        let analysis = match self.analysis {
            Some(analysis) => analysis, None => return false
        };
        if !please_inline && analysis.public_items.contains(&def.node) {
            return false
        }
        if !self.view_item_stack.insert(def.node) { return false }

        let ret = match tcx.map.get(def.node) {
            ast_map::NodeItem(it) => {
                if glob {
                    match it.node {
                        ast::ItemMod(ref m) => {
                            for vi in m.view_items.iter() {
                                self.visit_view_item(vi, om);
                            }
                            for i in m.items.iter() {
                                self.visit_item(&**i, None, om);
                            }
                        }
                        ast::ItemEnum(..) => {}
                        _ => { panic!("glob not mapped to a module or enum"); }
                    }
                } else {
                    self.visit_item(it, renamed, om);
                }
                true
            }
            _ => false,
        };
        self.view_item_stack.remove(&id);
        return ret;
    }

    pub fn visit_item(&mut self, item: &ast::Item,
                      renamed: Option<ast::Ident>, om: &mut Module) {
        debug!("Visiting item {}", item);
        let name = renamed.unwrap_or(item.ident);
        match item.node {
            ast::ItemMod(ref m) => {
                om.mods.push(self.visit_mod_contents(item.span,
                                                     item.attrs.clone(),
                                                     item.vis,
                                                     item.id,
                                                     m,
                                                     Some(name)));
            },
            ast::ItemEnum(ref ed, ref gen) =>
                om.enums.push(self.visit_enum_def(item, name, ed, gen)),
            ast::ItemStruct(ref sd, ref gen) =>
                om.structs.push(self.visit_struct_def(item, name, &**sd, gen)),
            ast::ItemFn(ref fd, ref pur, ref abi, ref gen, _) =>
                om.fns.push(self.visit_fn(item, name, &**fd, pur, abi, gen)),
            ast::ItemTy(ref ty, ref gen) => {
                let t = Typedef {
                    ty: ty.clone(),
                    gen: gen.clone(),
                    name: name,
                    id: item.id,
                    attrs: item.attrs.clone(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.typedefs.push(t);
            },
            ast::ItemStatic(ref ty, ref mut_, ref exp) => {
                let s = Static {
                    type_: ty.clone(),
                    mutability: mut_.clone(),
                    expr: exp.clone(),
                    id: item.id,
                    name: name,
                    attrs: item.attrs.clone(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.statics.push(s);
            },
            ast::ItemConst(ref ty, ref exp) => {
                let s = Constant {
                    type_: ty.clone(),
                    expr: exp.clone(),
                    id: item.id,
                    name: name,
                    attrs: item.attrs.clone(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.constants.push(s);
            },
            ast::ItemTrait(unsafety, ref gen, ref def_ub, ref b, ref items) => {
                let t = Trait {
                    unsafety: unsafety,
                    name: name,
                    items: items.clone(),
                    generics: gen.clone(),
                    bounds: b.iter().map(|x| (*x).clone()).collect(),
                    id: item.id,
                    attrs: item.attrs.clone(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                    default_unbound: def_ub.clone()
                };
                om.traits.push(t);
            },
            ast::ItemImpl(unsafety, ref gen, ref tr, ref ty, ref items) => {
                let i = Impl {
                    unsafety: unsafety,
                    generics: gen.clone(),
                    trait_: tr.clone(),
                    for_: ty.clone(),
                    items: items.clone(),
                    attrs: item.attrs.clone(),
                    id: item.id,
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.impls.push(i);
            },
            ast::ItemForeignMod(ref fm) => {
                om.foreigns.push(fm.clone());
            }
            ast::ItemMac(_) => {
                panic!("rustdoc: macros should be gone, after expansion");
            }
        }
    }

    // convert each exported_macro into a doc item
    fn visit_macro(&self, item: &ast::Item) -> Macro {
        Macro {
            id: item.id,
            attrs: item.attrs.clone(),
            name: item.ident,
            whence: item.span,
            stab: self.stability(item.id),
        }
    }
}
