// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * Conversion from AST representation of types to the ty.rs
 * representation.  The main routine here is `ast_ty_to_ty()`: each use
 * is parameterized by an instance of `AstConv` and a `RegionScope`.
 *
 * The parameterization of `ast_ty_to_ty()` is because it behaves
 * somewhat differently during the collect and check phases,
 * particularly with respect to looking up the types of top-level
 * items.  In the collect phase, the crate context is used as the
 * `AstConv` instance; in this phase, the `get_item_ty()` function
 * triggers a recursive call to `ty_of_item()`  (note that
 * `ast_ty_to_ty()` will detect recursive types and report an error).
 * In the check phase, when the FnCtxt is used as the `AstConv`,
 * `get_item_ty()` just looks up the item type in `tcx.tcache`.
 *
 * The `RegionScope` trait controls what happens when the user does
 * not specify a region in some location where a region is required
 * (e.g., if the user writes `&Foo` as a type rather than `&'a Foo`).
 * See the `rscope` module for more details.
 *
 * Unlike the `AstConv` trait, the region scope can change as we descend
 * the type.  This is to accommodate the fact that (a) fn types are binding
 * scopes and (b) the default region may change.  To understand case (a),
 * consider something like:
 *
 *   type foo = { x: &a.int, y: |&a.int| }
 *
 * The type of `x` is an error because there is no region `a` in scope.
 * In the type of `y`, however, region `a` is considered a bound region
 * as it does not already appear in scope.
 *
 * Case (b) says that if you have a type:
 *   type foo<'a> = ...;
 *   type bar = fn(&foo, &a.foo)
 * The fully expanded version of type bar is:
 *   type bar = fn(&'foo &, &a.foo<'a>)
 * Note that the self region for the `foo` defaulted to `&` in the first
 * case but `&a` in the second.  Basically, defaults that appear inside
 * an rptr (`&r.T`) use the region `r` that appears in the rptr.
 */


use middle::const_eval;
use middle::subst::Subst;
use middle::ty::{substs};
use middle::ty::{ty_param_substs_and_ty};
use middle::ty;
use middle::typeck::rscope;
use middle::typeck::rscope::{RegionScope};
use middle::typeck::lookup_def_tcx;
use util::ppaux::Repr;

use std::rc::Rc;
use syntax::abi;
use syntax::{ast, ast_util};
use syntax::codemap::Span;
use syntax::owned_slice::OwnedSlice;
use syntax::print::pprust::{lifetime_to_str, path_to_str};

pub trait AstConv {
    fn tcx<'a>(&'a self) -> &'a ty::ctxt;
    fn get_item_ty(&self, id: ast::DefId) -> ty::ty_param_bounds_and_ty;
    fn get_trait_def(&self, id: ast::DefId) -> Rc<ty::TraitDef>;

    // what type should we use when a type is omitted?
    fn ty_infer(&self, span: Span) -> ty::t;
}

pub fn ast_region_to_region(tcx: &ty::ctxt, lifetime: &ast::Lifetime)
                            -> ty::Region {
    let r = match tcx.named_region_map.find(&lifetime.id) {
        None => {
            // should have been recorded by the `resolve_lifetime` pass
            tcx.sess.span_bug(lifetime.span, "unresolved lifetime");
        }

        Some(&ast::DefStaticRegion) => {
            ty::ReStatic
        }

        Some(&ast::DefLateBoundRegion(binder_id, _, id)) => {
            ty::ReLateBound(binder_id, ty::BrNamed(ast_util::local_def(id),
                                                   lifetime.name))
        }

        Some(&ast::DefEarlyBoundRegion(index, id)) => {
            ty::ReEarlyBound(id, index, lifetime.name)
        }

        Some(&ast::DefFreeRegion(scope_id, id)) => {
            ty::ReFree(ty::FreeRegion {
                    scope_id: scope_id,
                    bound_region: ty::BrNamed(ast_util::local_def(id),
                                              lifetime.name)
                })
        }
    };

    debug!("ast_region_to_region(lifetime={} id={}) yields {}",
            lifetime_to_str(lifetime),
            lifetime.id, r.repr(tcx));

    r
}

pub fn opt_ast_region_to_region<AC:AstConv,RS:RegionScope>(
    this: &AC,
    rscope: &RS,
    default_span: Span,
    opt_lifetime: &Option<ast::Lifetime>) -> ty::Region
{
    let r = match *opt_lifetime {
        Some(ref lifetime) => {
            ast_region_to_region(this.tcx(), lifetime)
        }

        None => {
            match rscope.anon_regions(default_span, 1) {
                Err(()) => {
                    debug!("optional region in illegal location");
                    this.tcx().sess.span_err(
                        default_span, "missing lifetime specifier");
                    ty::ReStatic
                }

                Ok(rs) => {
                    *rs.get(0)
                }
            }
        }
    };

    debug!("opt_ast_region_to_region(opt_lifetime={:?}) yields {}",
            opt_lifetime.as_ref().map(|e| lifetime_to_str(e)),
            r.repr(this.tcx()));

    r
}

fn ast_path_substs<AC:AstConv,RS:RegionScope>(
    this: &AC,
    rscope: &RS,
    decl_generics: &ty::Generics,
    self_ty: Option<ty::t>,
    path: &ast::Path) -> ty::substs
{
    /*!
     * Given a path `path` that refers to an item `I` with the
     * declared generics `decl_generics`, returns an appropriate
     * set of substitutions for this particular reference to `I`.
     */

    let tcx = this.tcx();

    // If the type is parameterized by the this region, then replace this
    // region with the current anon region binding (in other words,
    // whatever & would get replaced with).
    let expected_num_region_params = decl_generics.region_param_defs().len();
    let supplied_num_region_params = path.segments.last().unwrap().lifetimes.len();
    let regions = if expected_num_region_params == supplied_num_region_params {
        path.segments.last().unwrap().lifetimes.iter().map(
            |l| ast_region_to_region(this.tcx(), l)).collect::<Vec<_>>()
    } else {
        let anon_regions =
            rscope.anon_regions(path.span, expected_num_region_params);

        if supplied_num_region_params != 0 || anon_regions.is_err() {
            tcx.sess.span_err(
                path.span,
                format!("wrong number of lifetime parameters: \
                        expected {} but found {}",
                        expected_num_region_params,
                        supplied_num_region_params).as_slice());
        }

        match anon_regions {
            Ok(v) => v.move_iter().collect(),
            Err(()) => Vec::from_fn(expected_num_region_params,
                                    |_| ty::ReStatic) // hokey
        }
    };

    // Convert the type parameters supplied by the user.
    let supplied_ty_param_count = path.segments.iter().flat_map(|s| s.types.iter()).len();
    let formal_ty_param_count = decl_generics.type_param_defs().len();
    let required_ty_param_count = decl_generics.type_param_defs().iter()
                                               .take_while(|x| x.default.is_none())
                                               .len();
    if supplied_ty_param_count < required_ty_param_count {
        let expected = if required_ty_param_count < formal_ty_param_count {
            "expected at least"
        } else {
            "expected"
        };
        this.tcx().sess.span_fatal(path.span,
            format!("wrong number of type arguments: {} {} but found {}",
                    expected,
                    required_ty_param_count,
                    supplied_ty_param_count).as_slice());
    } else if supplied_ty_param_count > formal_ty_param_count {
        let expected = if required_ty_param_count < formal_ty_param_count {
            "expected at most"
        } else {
            "expected"
        };
        this.tcx().sess.span_fatal(path.span,
            format!("wrong number of type arguments: {} {} but found {}",
                    expected,
                    formal_ty_param_count,
                    supplied_ty_param_count).as_slice());
    }

    if supplied_ty_param_count > required_ty_param_count
        && !this.tcx().sess.features.default_type_params.get() {
        this.tcx().sess.span_err(path.span, "default type parameters are \
                                             experimental and possibly buggy");
        this.tcx().sess.span_note(path.span, "add #![feature(default_type_params)] \
                                              to the crate attributes to enable");
    }

    let tps = path.segments.iter().flat_map(|s| s.types.iter())
                            .map(|&a_t| ast_ty_to_ty(this, rscope, a_t))
                            .collect();

    let mut substs = substs {
        regions: ty::NonerasedRegions(OwnedSlice::from_vec(regions)),
        self_ty: self_ty,
        tps: tps
    };

    for param in decl_generics.type_param_defs()
                              .slice_from(supplied_ty_param_count).iter() {
        let ty = param.default.unwrap().subst_spanned(tcx, &substs, Some(path.span));
        substs.tps.push(ty);
    }

    substs
}

pub fn ast_path_to_substs_and_ty<AC:AstConv,
                                 RS:RegionScope>(
                                 this: &AC,
                                 rscope: &RS,
                                 did: ast::DefId,
                                 path: &ast::Path)
                                 -> ty_param_substs_and_ty {
    let tcx = this.tcx();
    let ty::ty_param_bounds_and_ty {
        generics: generics,
        ty: decl_ty
    } = this.get_item_ty(did);

    let substs = ast_path_substs(this, rscope, &generics, None, path);
    let ty = ty::subst(tcx, &substs, decl_ty);
    ty_param_substs_and_ty { substs: substs, ty: ty }
}

pub fn ast_path_to_trait_ref<AC:AstConv,RS:RegionScope>(
        this: &AC,
        rscope: &RS,
        trait_def_id: ast::DefId,
        self_ty: Option<ty::t>,
        path: &ast::Path) -> Rc<ty::TraitRef> {
    let trait_def = this.get_trait_def(trait_def_id);
    Rc::new(ty::TraitRef {
        def_id: trait_def_id,
        substs: ast_path_substs(this, rscope, &trait_def.generics, self_ty, path)
    })
}

pub fn ast_path_to_ty<AC:AstConv,RS:RegionScope>(
        this: &AC,
        rscope: &RS,
        did: ast::DefId,
        path: &ast::Path)
     -> ty_param_substs_and_ty
{
    // Look up the polytype of the item and then substitute the provided types
    // for any type/region parameters.
    let ty::ty_param_substs_and_ty {
        substs: substs,
        ty: ty
    } = ast_path_to_substs_and_ty(this, rscope, did, path);
    ty_param_substs_and_ty { substs: substs, ty: ty }
}

pub static NO_REGIONS: uint = 1;
pub static NO_TPS: uint = 2;

fn check_path_args(tcx: &ty::ctxt,
                   path: &ast::Path,
                   flags: uint) {
    if (flags & NO_TPS) != 0u {
        if !path.segments.iter().all(|s| s.types.is_empty()) {
            tcx.sess.span_err(
                path.span,
                "type parameters are not allowed on this type");
        }
    }

    if (flags & NO_REGIONS) != 0u {
        if !path.segments.last().unwrap().lifetimes.is_empty() {
            tcx.sess.span_err(
                path.span,
                "region parameters are not allowed on this type");
        }
    }
}

pub fn ast_ty_to_prim_ty(tcx: &ty::ctxt, ast_ty: &ast::Ty) -> Option<ty::t> {
    match ast_ty.node {
        ast::TyPath(ref path, _, id) => {
            let a_def = match tcx.def_map.borrow().find(&id) {
                None => {
                    tcx.sess.span_bug(ast_ty.span,
                                      format!("unbound path {}",
                                              path_to_str(path)).as_slice())
                }
                Some(&d) => d
            };
            match a_def {
                ast::DefPrimTy(nty) => {
                    match nty {
                        ast::TyBool => {
                            check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                            Some(ty::mk_bool())
                        }
                        ast::TyChar => {
                            check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                            Some(ty::mk_char())
                        }
                        ast::TyInt(it) => {
                            check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                            Some(ty::mk_mach_int(it))
                        }
                        ast::TyUint(uit) => {
                            check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                            Some(ty::mk_mach_uint(uit))
                        }
                        ast::TyFloat(ft) => {
                            if ft == ast::TyF128 && !tcx.sess.features.quad_precision_float.get() {
                                tcx.sess.span_err(path.span, "quadruple precision floats are \
                                                              missing complete runtime support");
                                tcx.sess.span_note(path.span, "add \
                                                               #[feature(quad_precision_float)] \
                                                               to the crate attributes to enable");
                            }
                            check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                            Some(ty::mk_mach_float(ft))
                        }
                        ast::TyStr => {
                            tcx.sess.span_err(ast_ty.span,
                                              "bare `str` is not a type");
                            // return /something/ so they can at least get more errors
                            Some(ty::mk_uniq(tcx, ty::mk_str(tcx)))
                        }
                    }
                }
                _ => None
            }
        }
        _ => None
    }
}

/// Converts the given AST type to a built-in type. A "built-in type" is, at
/// present, either a core numeric type, a string, or `Box`.
pub fn ast_ty_to_builtin_ty<AC:AstConv,
                            RS:RegionScope>(
                            this: &AC,
                            rscope: &RS,
                            ast_ty: &ast::Ty)
                            -> Option<ty::t> {
    match ast_ty_to_prim_ty(this.tcx(), ast_ty) {
        Some(typ) => return Some(typ),
        None => {}
    }

    match ast_ty.node {
        ast::TyPath(ref path, _, id) => {
            let a_def = match this.tcx().def_map.borrow().find(&id) {
                None => {
                    this.tcx()
                        .sess
                        .span_bug(ast_ty.span,
                                  format!("unbound path {}",
                                          path_to_str(path)).as_slice())
                }
                Some(&d) => d
            };

            // FIXME(#12938): This is a hack until we have full support for
            // DST.
            match a_def {
                ast::DefTy(did) | ast::DefStruct(did)
                        if Some(did) == this.tcx().lang_items.owned_box() => {
                    if path.segments
                           .iter()
                           .flat_map(|s| s.types.iter())
                           .len() > 1 {
                        this.tcx()
                            .sess
                            .span_err(path.span,
                                      "`Box` has only one type parameter")
                    }

                    for inner_ast_type in path.segments
                                              .iter()
                                              .flat_map(|s| s.types.iter()) {
                        let mt = ast::MutTy {
                            ty: *inner_ast_type,
                            mutbl: ast::MutImmutable,
                        };
                        return Some(mk_pointer(this,
                                               rscope,
                                               &mt,
                                               Uniq,
                                               |typ| {
                            match ty::get(typ).sty {
                                ty::ty_str => {
                                    this.tcx()
                                        .sess
                                        .span_err(path.span,
                                                  "`Box<str>` is not a type");
                                    ty::mk_err()
                                }
                                ty::ty_vec(_, None) => {
                                    this.tcx()
                                        .sess
                                        .span_err(path.span,
                                                  "`Box<[T]>` is not a type");
                                    ty::mk_err()
                                }
                                _ => ty::mk_uniq(this.tcx(), typ),
                            }
                        }))
                    }
                    this.tcx().sess.span_err(path.span,
                                             "not enough type parameters \
                                              supplied to `Box<T>`");
                    Some(ty::mk_err())
                }
                _ => None
            }
        }
        _ => None
    }
}

enum PointerTy {
    Box,
    RPtr(ty::Region),
    Uniq
}

fn ast_ty_to_mt<AC:AstConv, RS:RegionScope>(this: &AC,
                                            rscope: &RS,
                                            ty: &ast::Ty) -> ty::mt {
    ty::mt {ty: ast_ty_to_ty(this, rscope, ty), mutbl: ast::MutImmutable}
}

// Handle `~`, `Box`, and `&` being able to mean strs and vecs.
// If a_seq_ty is a str or a vec, make it a str/vec.
// Also handle first-class trait types.
fn mk_pointer<AC:AstConv,
              RS:RegionScope>(
              this: &AC,
              rscope: &RS,
              a_seq_ty: &ast::MutTy,
              ptr_ty: PointerTy,
              constr: |ty::t| -> ty::t)
              -> ty::t {
    let tcx = this.tcx();
    debug!("mk_pointer(ptr_ty={:?})", ptr_ty);

    match a_seq_ty.ty.node {
        ast::TyVec(ty) => {
            let mut mt = ast_ty_to_mt(this, rscope, ty);
            if a_seq_ty.mutbl == ast::MutMutable {
                mt.mutbl = ast::MutMutable;
            }
            return constr(ty::mk_vec(tcx, mt, None));
        }
        ast::TyPath(ref path, ref bounds, id) => {
            // Note that the "bounds must be empty if path is not a trait"
            // restriction is enforced in the below case for ty_path, which
            // will run after this as long as the path isn't a trait.
            match tcx.def_map.borrow().find(&id) {
                Some(&ast::DefPrimTy(ast::TyStr)) => {
                    check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                    match ptr_ty {
                        Uniq => {
                            return constr(ty::mk_str(tcx));
                        }
                        RPtr(r) => {
                            return ty::mk_str_slice(tcx, r, ast::MutImmutable);
                        }
                        _ => {
                            tcx.sess
                               .span_err(path.span,
                                         "managed strings are not supported")
                        }
                    }
                }
                Some(&ast::DefTrait(trait_def_id)) => {
                    let result = ast_path_to_trait_ref(
                        this, rscope, trait_def_id, None, path);
                    let trait_store = match ptr_ty {
                        Uniq => ty::UniqTraitStore,
                        RPtr(r) => {
                            ty::RegionTraitStore(r, a_seq_ty.mutbl)
                        }
                        _ => {
                            tcx.sess.span_err(
                                path.span,
                                "~trait or &trait are the only supported \
                                 forms of casting-to-trait");
                            return ty::mk_err();
                        }
                    };
                    let bounds = conv_builtin_bounds(this.tcx(), bounds, trait_store);
                    return ty::mk_trait(tcx,
                                        result.def_id,
                                        result.substs.clone(),
                                        trait_store,
                                        bounds);
                }
                _ => {}
            }
        }
        _ => {}
    }

    constr(ast_ty_to_ty(this, rscope, a_seq_ty.ty))
}

// Parses the programmer's textual representation of a type into our
// internal notion of a type.
pub fn ast_ty_to_ty<AC:AstConv, RS:RegionScope>(
    this: &AC, rscope: &RS, ast_ty: &ast::Ty) -> ty::t {

    let tcx = this.tcx();

    let mut ast_ty_to_ty_cache = tcx.ast_ty_to_ty_cache.borrow_mut();
    match ast_ty_to_ty_cache.find(&ast_ty.id) {
        Some(&ty::atttce_resolved(ty)) => return ty,
        Some(&ty::atttce_unresolved) => {
            tcx.sess.span_fatal(ast_ty.span,
                                "illegal recursive type; insert an enum \
                                 or struct in the cycle, if this is \
                                 desired");
        }
        None => { /* go on */ }
    }
    ast_ty_to_ty_cache.insert(ast_ty.id, ty::atttce_unresolved);
    drop(ast_ty_to_ty_cache);

    let typ = ast_ty_to_builtin_ty(this, rscope, ast_ty).unwrap_or_else(|| {
        match ast_ty.node {
            ast::TyNil => ty::mk_nil(),
            ast::TyBot => ty::mk_bot(),
            ast::TyBox(ty) => {
                let mt = ast::MutTy { ty: ty, mutbl: ast::MutImmutable };
                mk_pointer(this, rscope, &mt, Box, |ty| ty::mk_box(tcx, ty))
            }
            ast::TyUniq(ty) => {
                let mt = ast::MutTy { ty: ty, mutbl: ast::MutImmutable };
                mk_pointer(this, rscope, &mt, Uniq,
                           |ty| ty::mk_uniq(tcx, ty))
            }
            ast::TyVec(ty) => {
                tcx.sess.span_err(ast_ty.span, "bare `[]` is not a type");
                // return /something/ so they can at least get more errors
                let vec_ty = ty::mk_vec(tcx, ast_ty_to_mt(this, rscope, ty), None);
                ty::mk_uniq(tcx, vec_ty)
            }
            ast::TyPtr(ref mt) => {
                ty::mk_ptr(tcx, ty::mt {
                    ty: ast_ty_to_ty(this, rscope, mt.ty),
                    mutbl: mt.mutbl
                })
            }
            ast::TyRptr(ref region, ref mt) => {
                let r = opt_ast_region_to_region(this, rscope, ast_ty.span, region);
                debug!("ty_rptr r={}", r.repr(this.tcx()));
                mk_pointer(this, rscope, mt, RPtr(r),
                           |ty| ty::mk_rptr(tcx, r, ty::mt {ty: ty, mutbl: mt.mutbl}))
            }
            ast::TyTup(ref fields) => {
                let flds = fields.iter()
                                 .map(|&t| ast_ty_to_ty(this, rscope, t))
                                 .collect();
                ty::mk_tup(tcx, flds)
            }
            ast::TyBareFn(ref bf) => {
                if bf.decl.variadic && bf.abi != abi::C {
                    tcx.sess.span_err(ast_ty.span,
                                      "variadic function must have C calling convention");
                }
                ty::mk_bare_fn(tcx, ty_of_bare_fn(this, ast_ty.id, bf.fn_style,
                                                  bf.abi, bf.decl))
            }
            ast::TyClosure(ref f, ref region) => {

                // resolve the function bound region in the original region
                // scope `rscope`, not the scope of the function parameters
                let bound_region = opt_ast_region_to_region(this, rscope,
                                                            ast_ty.span, region);

                let store = ty::RegionTraitStore(bound_region, ast::MutMutable);

                // Use corresponding trait store to figure out default bounds
                // if none were specified.
                let bounds = conv_builtin_bounds(this.tcx(), &f.bounds, store);

                let fn_decl = ty_of_closure(this,
                                            ast_ty.id,
                                            f.fn_style,
                                            f.onceness,
                                            bounds,
                                            store,
                                            f.decl,
                                            None);
                ty::mk_closure(tcx, fn_decl)
            }
            ast::TyProc(ref f) => {
                // Use corresponding trait store to figure out default bounds
                // if none were specified.
                let bounds = conv_builtin_bounds(this.tcx(), &f.bounds, ty::UniqTraitStore);

                let fn_decl = ty_of_closure(this,
                                            ast_ty.id,
                                            f.fn_style,
                                            f.onceness,
                                            bounds,
                                            ty::UniqTraitStore,
                                            f.decl,
                                            None);
                ty::mk_closure(tcx, fn_decl)
            }
            ast::TyPath(ref path, ref bounds, id) => {
                let a_def = match tcx.def_map.borrow().find(&id) {
                    None => {
                        tcx.sess
                           .span_bug(ast_ty.span,
                                     format!("unbound path {}",
                                             path_to_str(path)).as_slice())
                    }
                    Some(&d) => d
                };
                // Kind bounds on path types are only supported for traits.
                match a_def {
                    // But don't emit the error if the user meant to do a trait anyway.
                    ast::DefTrait(..) => { },
                    _ if bounds.is_some() =>
                        tcx.sess.span_err(ast_ty.span,
                                          "kind bounds can only be used on trait types"),
                    _ => { },
                }
                match a_def {
                    ast::DefTrait(_) => {
                        let path_str = path_to_str(path);
                        tcx.sess.span_err(
                            ast_ty.span,
                            format!("reference to trait `{name}` where a \
                                     type is expected; try `Box<{name}>` or \
                                     `&{name}`",
                                    name=path_str).as_slice());
                        ty::mk_err()
                    }
                    ast::DefTy(did) | ast::DefStruct(did) => {
                        ast_path_to_ty(this, rscope, did, path).ty
                    }
                    ast::DefTyParam(id, n) => {
                        check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                        ty::mk_param(tcx, n, id)
                    }
                    ast::DefSelfTy(id) => {
                        // n.b.: resolve guarantees that the this type only appears in a
                        // trait, which we rely upon in various places when creating
                        // substs
                        check_path_args(tcx, path, NO_TPS | NO_REGIONS);
                        let did = ast_util::local_def(id);
                        ty::mk_self(tcx, did)
                    }
                    ast::DefMod(id) => {
                        tcx.sess.span_fatal(ast_ty.span,
                            format!("found module name used as a type: {}",
                                    tcx.map.node_to_str(id.node)).as_slice());
                    }
                    ast::DefPrimTy(_) => {
                        fail!("DefPrimTy arm missed in previous ast_ty_to_prim_ty call");
                    }
                    _ => {
                        tcx.sess.span_fatal(ast_ty.span,
                                            format!("found value name used \
                                                     as a type: {:?}",
                                                    a_def).as_slice());
                    }
                }
            }
            ast::TyFixedLengthVec(ty, e) => {
                match const_eval::eval_const_expr_partial(tcx, e) {
                    Ok(ref r) => {
                        match *r {
                            const_eval::const_int(i) =>
                                ty::mk_vec(tcx, ast_ty_to_mt(this, rscope, ty),
                                           Some(i as uint)),
                            const_eval::const_uint(i) =>
                                ty::mk_vec(tcx, ast_ty_to_mt(this, rscope, ty),
                                           Some(i as uint)),
                            _ => {
                                tcx.sess.span_fatal(
                                    ast_ty.span, "expected constant expr for vector length");
                            }
                        }
                    }
                    Err(ref r) => {
                        tcx.sess.span_fatal(
                            ast_ty.span,
                            format!("expected constant expr for vector \
                                     length: {}",
                                    *r).as_slice());
                    }
                }
            }
            ast::TyTypeof(_e) => {
                tcx.sess.span_bug(ast_ty.span, "typeof is reserved but unimplemented");
            }
            ast::TyInfer => {
                // TyInfer also appears as the type of arguments or return
                // values in a ExprFnBlock or ExprProc, or as the type of
                // local variables. Both of these cases are handled specially
                // and will not descend into this routine.
                this.ty_infer(ast_ty.span)
            }
        }
    });

    tcx.ast_ty_to_ty_cache.borrow_mut().insert(ast_ty.id, ty::atttce_resolved(typ));
    return typ;
}

pub fn ty_of_arg<AC: AstConv, RS: RegionScope>(this: &AC, rscope: &RS, a: &ast::Arg,
                                               expected_ty: Option<ty::t>) -> ty::t {
    match a.ty.node {
        ast::TyInfer if expected_ty.is_some() => expected_ty.unwrap(),
        ast::TyInfer => this.ty_infer(a.ty.span),
        _ => ast_ty_to_ty(this, rscope, a.ty),
    }
}

struct SelfInfo {
    untransformed_self_ty: ty::t,
    explicit_self: ast::ExplicitSelf
}

pub fn ty_of_method<AC:AstConv>(
    this: &AC,
    id: ast::NodeId,
    fn_style: ast::FnStyle,
    untransformed_self_ty: ty::t,
    explicit_self: ast::ExplicitSelf,
    decl: &ast::FnDecl) -> ty::BareFnTy {
    ty_of_method_or_bare_fn(this, id, fn_style, abi::Rust, Some(SelfInfo {
        untransformed_self_ty: untransformed_self_ty,
        explicit_self: explicit_self
    }), decl)
}

pub fn ty_of_bare_fn<AC:AstConv>(this: &AC, id: ast::NodeId,
                                 fn_style: ast::FnStyle, abi: abi::Abi,
                                 decl: &ast::FnDecl) -> ty::BareFnTy {
    ty_of_method_or_bare_fn(this, id, fn_style, abi, None, decl)
}

fn ty_of_method_or_bare_fn<AC:AstConv>(this: &AC, id: ast::NodeId,
                                       fn_style: ast::FnStyle, abi: abi::Abi,
                                       opt_self_info: Option<SelfInfo>,
                                       decl: &ast::FnDecl) -> ty::BareFnTy {
    debug!("ty_of_method_or_bare_fn");

    // new region names that appear inside of the fn decl are bound to
    // that function type
    let rb = rscope::BindingRscope::new(id);

    let self_ty = opt_self_info.and_then(|self_info| {
        match self_info.explicit_self.node {
            ast::SelfStatic => None,
            ast::SelfValue => {
                Some(self_info.untransformed_self_ty)
            }
            ast::SelfRegion(ref lifetime, mutability) => {
                let region =
                    opt_ast_region_to_region(this, &rb,
                                             self_info.explicit_self.span,
                                             lifetime);
                Some(ty::mk_rptr(this.tcx(), region,
                                 ty::mt {ty: self_info.untransformed_self_ty,
                                         mutbl: mutability}))
            }
            ast::SelfUniq => {
                Some(ty::mk_uniq(this.tcx(), self_info.untransformed_self_ty))
            }
        }
    });

    // HACK(eddyb) replace the fake self type in the AST with the actual type.
    let input_tys = if self_ty.is_some() {
        decl.inputs.slice_from(1)
    } else {
        decl.inputs.as_slice()
    };
    let input_tys = input_tys.iter().map(|a| ty_of_arg(this, &rb, a, None));

    let self_and_input_tys = self_ty.move_iter().chain(input_tys).collect();

    let output_ty = match decl.output.node {
        ast::TyInfer => this.ty_infer(decl.output.span),
        _ => ast_ty_to_ty(this, &rb, decl.output)
    };

    return ty::BareFnTy {
        fn_style: fn_style,
        abi: abi,
        sig: ty::FnSig {
            binder_id: id,
            inputs: self_and_input_tys,
            output: output_ty,
            variadic: decl.variadic
        }
    };
}

pub fn ty_of_closure<AC:AstConv>(
    this: &AC,
    id: ast::NodeId,
    fn_style: ast::FnStyle,
    onceness: ast::Onceness,
    bounds: ty::BuiltinBounds,
    store: ty::TraitStore,
    decl: &ast::FnDecl,
    expected_sig: Option<ty::FnSig>)
    -> ty::ClosureTy
{
    debug!("ty_of_fn_decl");

    // new region names that appear inside of the fn decl are bound to
    // that function type
    let rb = rscope::BindingRscope::new(id);

    let input_tys = decl.inputs.iter().enumerate().map(|(i, a)| {
        let expected_arg_ty = expected_sig.as_ref().and_then(|e| {
            // no guarantee that the correct number of expected args
            // were supplied
            if i < e.inputs.len() {
                Some(*e.inputs.get(i))
            } else {
                None
            }
        });
        ty_of_arg(this, &rb, a, expected_arg_ty)
    }).collect();

    let expected_ret_ty = expected_sig.map(|e| e.output);
    let output_ty = match decl.output.node {
        ast::TyInfer if expected_ret_ty.is_some() => expected_ret_ty.unwrap(),
        ast::TyInfer => this.ty_infer(decl.output.span),
        _ => ast_ty_to_ty(this, &rb, decl.output)
    };

    ty::ClosureTy {
        fn_style: fn_style,
        onceness: onceness,
        store: store,
        bounds: bounds,
        sig: ty::FnSig {binder_id: id,
                        inputs: input_tys,
                        output: output_ty,
                        variadic: decl.variadic}
    }
}

fn conv_builtin_bounds(tcx: &ty::ctxt, ast_bounds: &Option<OwnedSlice<ast::TyParamBound>>,
                       store: ty::TraitStore)
                       -> ty::BuiltinBounds {
    //! Converts a list of bounds from the AST into a `BuiltinBounds`
    //! struct. Reports an error if any of the bounds that appear
    //! in the AST refer to general traits and not the built-in traits
    //! like `Send`. Used to translate the bounds that
    //! appear in closure and trait types, where only builtin bounds are
    //! legal.
    //! If no bounds were specified, we choose a "default" bound based on
    //! the allocation type of the fn/trait, as per issue #7264. The user can
    //! override this with an empty bounds list, e.g. "Box<fn:()>" or
    //! "Box<Trait:>".

    match (ast_bounds, store) {
        (&Some(ref bound_vec), _) => {
            let mut builtin_bounds = ty::EmptyBuiltinBounds();
            for ast_bound in bound_vec.iter() {
                match *ast_bound {
                    ast::TraitTyParamBound(ref b) => {
                        match lookup_def_tcx(tcx, b.path.span, b.ref_id) {
                            ast::DefTrait(trait_did) => {
                                if ty::try_add_builtin_trait(tcx, trait_did,
                                                             &mut builtin_bounds) {
                                    continue; // success
                                }
                            }
                            _ => { }
                        }
                        tcx.sess.span_fatal(
                            b.path.span,
                            "only the builtin traits can be used as closure \
                             or object bounds");
                    }
                    ast::StaticRegionTyParamBound => {
                        builtin_bounds.add(ty::BoundStatic);
                    }
                    ast::OtherRegionTyParamBound(span) => {
                        if !tcx.sess.features.issue_5723_bootstrap.get() {
                            tcx.sess.span_err(
                                span,
                                "only the 'static lifetime is accepted \
                                 here.");
                        }
                    }
                }
            }
            builtin_bounds
        },
        // &'static Trait is sugar for &'static Trait:'static.
        (&None, ty::RegionTraitStore(ty::ReStatic, _)) => {
            let mut set = ty::EmptyBuiltinBounds(); set.add(ty::BoundStatic); set
        }
        // No bounds are automatically applied for &'r Trait or ~Trait
        (&None, ty::RegionTraitStore(..)) |
        (&None, ty::UniqTraitStore) => ty::EmptyBuiltinBounds(),
    }
}
