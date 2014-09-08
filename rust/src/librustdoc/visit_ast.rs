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

use syntax::abi;
use syntax::ast;
use syntax::ast_util;
use syntax::ast_map;
use syntax::attr;
use syntax::attr::AttrMetaMethods;
use syntax::codemap::Span;

use rustc::middle::stability;

use std::gc::{Gc, GC};

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
}

impl<'a, 'tcx> RustdocVisitor<'a, 'tcx> {
    pub fn new(cx: &'a core::DocContext<'tcx>,
               analysis: Option<&'a core::CrateAnalysis>) -> RustdocVisitor<'a, 'tcx> {
        RustdocVisitor {
            module: Module::new(None),
            attrs: Vec::new(),
            cx: cx,
            analysis: analysis,
        }
    }

    fn stability(&self, id: ast::NodeId) -> Option<attr::Stability> {
        self.cx.tcx_opt().and_then(|tcx| stability::lookup(tcx, ast_util::local_def(id)))
    }

    pub fn visit(&mut self, krate: &ast::Crate) {
        self.attrs = krate.attrs.iter().map(|x| (*x).clone()).collect();

        self.module = self.visit_mod_contents(krate.span,
                                              krate.attrs
                                                   .iter()
                                                   .map(|x| *x)
                                                   .collect(),
                                              ast::Public,
                                              ast::CRATE_NODE_ID,
                                              &krate.module,
                                              None);
        // attach the crate's exported macros to the top-level module:
        self.module.macros = krate.exported_macros.iter()
            .map(|it| self.visit_macro(&**it)).collect();
        self.module.is_crate = true;
    }

    pub fn visit_struct_def(&mut self, item: &ast::Item, sd: Gc<ast::StructDef>,
                            generics: &ast::Generics) -> Struct {
        debug!("Visiting struct");
        let struct_type = struct_type_from_def(&*sd);
        Struct {
            id: item.id,
            struct_type: struct_type,
            name: item.ident,
            vis: item.vis,
            stab: self.stability(item.id),
            attrs: item.attrs.iter().map(|x| *x).collect(),
            generics: generics.clone(),
            fields: sd.fields.iter().map(|x| (*x).clone()).collect(),
            whence: item.span
        }
    }

    pub fn visit_enum_def(&mut self, it: &ast::Item, def: &ast::EnumDef,
                          params: &ast::Generics) -> Enum {
        debug!("Visiting enum");
        let mut vars: Vec<Variant> = Vec::new();
        for x in def.variants.iter() {
            vars.push(Variant {
                name: x.node.name,
                attrs: x.node.attrs.iter().map(|x| *x).collect(),
                vis: x.node.vis,
                stab: self.stability(x.node.id),
                id: x.node.id,
                kind: x.node.kind.clone(),
                whence: x.span,
            });
        }
        Enum {
            name: it.ident,
            variants: vars,
            vis: it.vis,
            stab: self.stability(it.id),
            generics: params.clone(),
            attrs: it.attrs.iter().map(|x| *x).collect(),
            id: it.id,
            whence: it.span,
        }
    }

    pub fn visit_fn(&mut self, item: &ast::Item, fd: &ast::FnDecl,
                    fn_style: &ast::FnStyle, _abi: &abi::Abi,
                    gen: &ast::Generics) -> Function {
        debug!("Visiting fn");
        Function {
            id: item.id,
            vis: item.vis,
            stab: self.stability(item.id),
            attrs: item.attrs.iter().map(|x| *x).collect(),
            decl: fd.clone(),
            name: item.ident,
            whence: item.span,
            generics: gen.clone(),
            fn_style: *fn_style,
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
            self.visit_item(&**i, &mut om);
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
                match self.visit_view_path(*vpath, om, please_inline) {
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

    fn visit_view_path(&mut self, path: Gc<ast::ViewPath>,
                       om: &mut Module,
                       please_inline: bool) -> Option<Gc<ast::ViewPath>> {
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
                return Some(box(GC) ::syntax::codemap::Spanned {
                    node: ast::ViewPathList(p.clone(), mine, b.clone()),
                    span: path.span,
                })
            }

            // these are feature gated anyway
            ast::ViewPathGlob(_, id) => {
                if self.resolve_id(id, None, true, om, please_inline) {
                    return None
                }
            }
        }
        return Some(path);
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

        match tcx.map.get(def.node) {
            ast_map::NodeItem(it) => {
                let it = match renamed {
                    Some(ident) => {
                        box(GC) ast::Item {
                            ident: ident,
                            ..(*it).clone()
                        }
                    }
                    None => it,
                };
                if glob {
                    match it.node {
                        ast::ItemMod(ref m) => {
                            for vi in m.view_items.iter() {
                                self.visit_view_item(vi, om);
                            }
                            for i in m.items.iter() {
                                self.visit_item(&**i, om);
                            }
                        }
                        _ => { fail!("glob not mapped to a module"); }
                    }
                } else {
                    self.visit_item(&*it, om);
                }
                true
            }
            _ => false,
        }
    }

    pub fn visit_item(&mut self, item: &ast::Item, om: &mut Module) {
        debug!("Visiting item {:?}", item);
        match item.node {
            ast::ItemMod(ref m) => {
                om.mods.push(self.visit_mod_contents(item.span,
                                                     item.attrs
                                                         .iter()
                                                         .map(|x| *x)
                                                         .collect(),
                                                     item.vis,
                                                     item.id,
                                                     m,
                                                     Some(item.ident)));
            },
            ast::ItemEnum(ref ed, ref gen) =>
                om.enums.push(self.visit_enum_def(item, ed, gen)),
            ast::ItemStruct(sd, ref gen) =>
                om.structs.push(self.visit_struct_def(item, sd, gen)),
            ast::ItemFn(ref fd, ref pur, ref abi, ref gen, _) =>
                om.fns.push(self.visit_fn(item, &**fd, pur, abi, gen)),
            ast::ItemTy(ty, ref gen) => {
                let t = Typedef {
                    ty: ty,
                    gen: gen.clone(),
                    name: item.ident,
                    id: item.id,
                    attrs: item.attrs.iter().map(|x| *x).collect(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.typedefs.push(t);
            },
            ast::ItemStatic(ty, ref mut_, ref exp) => {
                let s = Static {
                    type_: ty,
                    mutability: mut_.clone(),
                    expr: exp.clone(),
                    id: item.id,
                    name: item.ident,
                    attrs: item.attrs.iter().map(|x| *x).collect(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.statics.push(s);
            },
            ast::ItemTrait(ref gen, _, ref b, ref items) => {
                let t = Trait {
                    name: item.ident,
                    items: items.iter().map(|x| (*x).clone()).collect(),
                    generics: gen.clone(),
                    bounds: b.iter().map(|x| (*x).clone()).collect(),
                    id: item.id,
                    attrs: item.attrs.iter().map(|x| *x).collect(),
                    whence: item.span,
                    vis: item.vis,
                    stab: self.stability(item.id),
                };
                om.traits.push(t);
            },
            ast::ItemImpl(ref gen, ref tr, ty, ref items) => {
                let i = Impl {
                    generics: gen.clone(),
                    trait_: tr.clone(),
                    for_: ty,
                    items: items.iter().map(|x| *x).collect(),
                    attrs: item.attrs.iter().map(|x| *x).collect(),
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
                fail!("rustdoc: macros should be gone, after expansion");
            }
        }
    }

    // convert each exported_macro into a doc item
    fn visit_macro(&self, item: &ast::Item) -> Macro {
        Macro {
            id: item.id,
            attrs: item.attrs.iter().map(|x| *x).collect(),
            name: item.ident,
            whence: item.span,
            stab: self.stability(item.id),
        }
    }
}
