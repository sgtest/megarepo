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

use middle::pat_util::{PatIdMap, pat_id_map, pat_is_binding, pat_is_const};
use middle::pat_util::{pat_is_variant_or_struct};
use middle::ty;
use middle::typeck::check::demand;
use middle::typeck::check::{check_block, check_expr_with, fn_ctxt};
use middle::typeck::check::{instantiate_path, lookup_def, lookup_local};
use middle::typeck::check::{structure_of, valid_range_bounds};
use middle::typeck::require_same_types;

use core::vec;
use std::map::HashMap;
use syntax::ast;
use syntax::ast_util::walk_pat;
use syntax::ast_util;
use syntax::codemap::span;
use syntax::print::pprust;

fn check_match(fcx: @fn_ctxt,
               expr: @ast::expr,
               discrim: @ast::expr,
               arms: ~[ast::arm]) -> bool {
    let tcx = fcx.ccx.tcx;
    let mut bot;

    let pattern_ty = fcx.infcx().next_ty_var();
    bot = check_expr_with(fcx, discrim, pattern_ty);

    // Typecheck the patterns first, so that we get types for all the
    // bindings.
    for arms.each |arm| {
        let pcx = pat_ctxt {
            fcx: fcx,
            map: pat_id_map(tcx.def_map, arm.pats[0]),
            match_region: ty::re_scope(expr.id),
            block_region: ty::re_scope(arm.body.node.id)
        };

        for arm.pats.each |p| { check_pat(pcx, *p, pattern_ty);}
    }

    // Now typecheck the blocks.
    let mut result_ty = fcx.infcx().next_ty_var();
    let mut arm_non_bot = false;
    for arms.each |arm| {
        match arm.guard {
          Some(e) => { check_expr_with(fcx, e, ty::mk_bool(tcx)); },
          None => ()
        }
        if !check_block(fcx, arm.body) { arm_non_bot = true; }
        let bty = fcx.node_ty(arm.body.node.id);
        demand::suptype(fcx, arm.body.span, result_ty, bty);
    }
    bot |= !arm_non_bot;
    if !arm_non_bot { result_ty = ty::mk_bot(tcx); }
    fcx.write_ty(expr.id, result_ty);
    return bot;
}

struct pat_ctxt {
    fcx: @fn_ctxt,
    map: PatIdMap,
    match_region: ty::Region, // Region for the match as a whole
    block_region: ty::Region, // Region for the block of the arm
}

fn check_pat_variant(pcx: pat_ctxt, pat: @ast::pat, path: @ast::path,
                     +subpats: Option<~[@ast::pat]>, expected: ty::t) {

    // Typecheck the path.
    let fcx = pcx.fcx;
    let tcx = pcx.fcx.ccx.tcx;

    let arg_types, kind_name;

    // structure_of requires type variables to be resolved.
    // So when we pass in <expected>, it's an error if it
    // contains type variables.

    // Check to see whether this is an enum or a struct.
    match structure_of(pcx.fcx, pat.span, expected) {
        ty::ty_enum(_, ref expected_substs) => {
            // Lookup the enum and variant def ids:
            let v_def = lookup_def(pcx.fcx, path.span, pat.id);
            let v_def_ids = ast_util::variant_def_ids(v_def);

            // Assign the pattern the type of the *enum*, not the variant.
            let enum_tpt = ty::lookup_item_type(tcx, v_def_ids.enm);
            instantiate_path(pcx.fcx, path, enum_tpt, pat.span, pat.id,
                             pcx.block_region);

            // check that the type of the value being matched is a subtype
            // of the type of the pattern:
            let pat_ty = fcx.node_ty(pat.id);
            demand::suptype(fcx, pat.span, pat_ty, expected);

            // Get the expected types of the arguments.
            arg_types = {
                let vinfo =
                    ty::enum_variant_with_id(
                        tcx, v_def_ids.enm, v_def_ids.var);
                let var_tpt = ty::lookup_item_type(tcx, v_def_ids.var);
                vinfo.args.map(|t| {
                    if var_tpt.bounds.len() == expected_substs.tps.len() {
                        ty::subst(tcx, expected_substs, *t)
                    }
                    else {
                        *t // In this case, an error was already signaled
                           // anyway
                    }
                })
            };

            kind_name = "variant";
        }
        ty::ty_struct(struct_def_id, ref expected_substs) => {
            // Assign the pattern the type of the struct.
            let struct_tpt = ty::lookup_item_type(tcx, struct_def_id);
            instantiate_path(pcx.fcx, path, struct_tpt, pat.span, pat.id,
                             pcx.block_region);

            // Check that the type of the value being matched is a subtype of
            // the type of the pattern.
            let pat_ty = fcx.node_ty(pat.id);
            demand::suptype(fcx, pat.span, pat_ty, expected);

            // Get the expected types of the arguments.
            let class_fields = ty::struct_fields(
                tcx, struct_def_id, expected_substs);
            arg_types = class_fields.map(|field| field.mt.ty);

            kind_name = "structure";
        }
        _ => {
            tcx.sess.span_fatal(
                pat.span,
                fmt!("mismatched types: expected enum or structure but \
                      found `%s`",
                     fcx.infcx().ty_to_str(expected)));
        }
    }

    let arg_len = arg_types.len();

    // Count the number of subpatterns.
    let subpats_len;
    match subpats {
        None => subpats_len = arg_len,
        Some(ref subpats) => subpats_len = subpats.len()
    }

    if arg_len > 0u {
        // N-ary variant.
        if arg_len != subpats_len {
            let s = fmt!("this pattern has %u field%s, but the corresponding \
                          %s has %u field%s",
                         subpats_len,
                         if subpats_len == 1u { ~"" } else { ~"s" },
                         kind_name,
                         arg_len,
                         if arg_len == 1u { ~"" } else { ~"s" });
            // XXX: This should not be fatal.
            tcx.sess.span_fatal(pat.span, s);
        }

        do subpats.iter() |pats| {
            for vec::each2(*pats, arg_types) |subpat, arg_ty| {
              check_pat(pcx, *subpat, *arg_ty);
            }
        };
    } else if subpats_len > 0u {
        tcx.sess.span_fatal
            (pat.span, fmt!("this pattern has %u field%s, but the \
                             corresponding %s has no fields",
                            subpats_len,
                            if subpats_len == 1u { ~"" }
                            else { ~"s" },
                            kind_name));
    }
}

/// `path` is the AST path item naming the type of this struct.
/// `fields` is the field patterns of the struct pattern.
/// `class_fields` describes the type of each field of the struct.
/// `class_id` is the ID of the struct.
/// `substitutions` are the type substitutions applied to this struct type
/// (e.g. K,V in HashMap<K,V>).
/// `etc` is true if the pattern said '...' and false otherwise.
fn check_struct_pat_fields(pcx: pat_ctxt,
                           span: span,
                           path: @ast::path,
                           fields: ~[ast::field_pat],
                           class_fields: ~[ty::field_ty],
                           class_id: ast::def_id,
                           substitutions: &ty::substs,
                           etc: bool) {
    let tcx = pcx.fcx.ccx.tcx;

    // Index the class fields.
    let field_map = HashMap();
    for class_fields.eachi |i, class_field| {
        field_map.insert(class_field.ident, i);
    }

    // Typecheck each field.
    let found_fields = HashMap();
    for fields.each |field| {
        match field_map.find(field.ident) {
            Some(index) => {
                let class_field = class_fields[index];
                let field_type = ty::lookup_field_type(tcx,
                                                       class_id,
                                                       class_field.id,
                                                       substitutions);
                check_pat(pcx, field.pat, field_type);
                found_fields.insert(index, ());
            }
            None => {
                let name = pprust::path_to_str(path, tcx.sess.intr());
                tcx.sess.span_err(span,
                                  fmt!("struct `%s` does not have a field
                                        named `%s`", name,
                                       tcx.sess.str_of(field.ident)));
            }
        }
    }

    // Report an error if not all the fields were specified.
    if !etc {
        for class_fields.eachi |i, field| {
            if found_fields.contains_key(i) {
                loop;
            }
            tcx.sess.span_err(span,
                              fmt!("pattern does not mention field `%s`",
                                   tcx.sess.str_of(field.ident)));
        }
    }
}

fn check_struct_pat(pcx: pat_ctxt, pat_id: ast::node_id, span: span,
                    expected: ty::t, path: @ast::path,
                    +fields: ~[ast::field_pat], etc: bool,
                    class_id: ast::def_id, substitutions: &ty::substs) {
    let fcx = pcx.fcx;
    let tcx = pcx.fcx.ccx.tcx;

    let class_fields = ty::lookup_struct_fields(tcx, class_id);

    // Check to ensure that the struct is the one specified.
    match tcx.def_map.find(pat_id) {
        Some(ast::def_struct(supplied_def_id))
                if supplied_def_id == class_id => {
            // OK.
        }
        Some(ast::def_struct(*)) | Some(ast::def_variant(*)) => {
            let name = pprust::path_to_str(path, tcx.sess.intr());
            tcx.sess.span_err(span,
                              fmt!("mismatched types: expected `%s` but \
                                    found `%s`",
                                   fcx.infcx().ty_to_str(expected),
                                   name));
        }
        _ => {
            tcx.sess.span_bug(span, ~"resolve didn't write in class");
        }
    }

    // Forbid pattern-matching structs with destructors.
    if ty::has_dtor(tcx, class_id) {
        tcx.sess.span_err(span, ~"deconstructing struct not allowed in \
                                  pattern (it has a destructor)");
    }

    check_struct_pat_fields(pcx, span, path, fields, class_fields, class_id,
                            substitutions, etc);
}

fn check_struct_like_enum_variant_pat(pcx: pat_ctxt,
                                      pat_id: ast::node_id,
                                      span: span,
                                      expected: ty::t,
                                      path: @ast::path,
                                      +fields: ~[ast::field_pat],
                                      etc: bool,
                                      enum_id: ast::def_id,
                                      substitutions: &ty::substs) {
    let fcx = pcx.fcx;
    let tcx = pcx.fcx.ccx.tcx;

    // Find the variant that was specified.
    match tcx.def_map.find(pat_id) {
        Some(ast::def_variant(found_enum_id, variant_id))
                if found_enum_id == enum_id => {
            // Get the struct fields from this struct-like enum variant.
            let class_fields = ty::lookup_struct_fields(tcx, variant_id);

            check_struct_pat_fields(pcx, span, path, fields, class_fields,
                                    variant_id, substitutions, etc);
        }
        Some(ast::def_struct(*)) | Some(ast::def_variant(*)) => {
            let name = pprust::path_to_str(path, tcx.sess.intr());
            tcx.sess.span_err(span,
                              fmt!("mismatched types: expected `%s` but \
                                    found `%s`",
                                   fcx.infcx().ty_to_str(expected),
                                   name));
        }
        _ => {
            tcx.sess.span_bug(span, ~"resolve didn't write in variant");
        }
    }
}

// Pattern checking is top-down rather than bottom-up so that bindings get
// their types immediately.
fn check_pat(pcx: pat_ctxt, pat: @ast::pat, expected: ty::t) {
    let fcx = pcx.fcx;
    let tcx = pcx.fcx.ccx.tcx;

    match /*bad*/copy pat.node {
      ast::pat_wild => {
        fcx.write_ty(pat.id, expected);
      }
      ast::pat_lit(lt) => {
        check_expr_with(fcx, lt, expected);
        fcx.write_ty(pat.id, fcx.expr_ty(lt));
      }
      ast::pat_range(begin, end) => {
        check_expr_with(fcx, begin, expected);
        check_expr_with(fcx, end, expected);
        let b_ty =
            fcx.infcx().resolve_type_vars_if_possible(fcx.expr_ty(begin));
        let e_ty =
            fcx.infcx().resolve_type_vars_if_possible(fcx.expr_ty(end));
        debug!("pat_range beginning type: %?", b_ty);
        debug!("pat_range ending type: %?", e_ty);
        if !require_same_types(
            tcx, Some(fcx.infcx()), false, pat.span, b_ty, e_ty,
            || ~"mismatched types in range")
        {
            // no-op
        } else if !ty::type_is_numeric(b_ty) {
            tcx.sess.span_err(pat.span, ~"non-numeric type used in range");
        } else if !valid_range_bounds(fcx.ccx, begin, end) {
            tcx.sess.span_err(begin.span, ~"lower range bound must be less \
                                           than upper");
        }
        fcx.write_ty(pat.id, b_ty);
      }
      ast::pat_ident(*) if pat_is_const(tcx.def_map, pat) => {
        let const_did = ast_util::def_id_of_def(tcx.def_map.get(pat.id));
        let const_tpt = ty::lookup_item_type(tcx, const_did);
        fcx.write_ty(pat.id, const_tpt.ty);
      }
      ast::pat_ident(bm, name, sub) if pat_is_binding(tcx.def_map, pat) => {
        let vid = lookup_local(fcx, pat.span, pat.id);
        let mut typ = ty::mk_var(tcx, vid);

        match bm {
          ast::bind_by_ref(mutbl) => {
            // if the binding is like
            //    ref x | ref const x | ref mut x
            // then the type of x is &M T where M is the mutability
            // and T is the expected type
            let region_var =
                fcx.infcx().next_region_var_with_lb(
                    pat.span, pcx.block_region);
            let mt = ty::mt {ty: expected, mutbl: mutbl};
            let region_ty = ty::mk_rptr(tcx, region_var, mt);
            demand::eqtype(fcx, pat.span, region_ty, typ);
          }
          // otherwise the type of x is the expected type T
          ast::bind_by_value | ast::bind_by_move | ast::bind_infer => {
            demand::eqtype(fcx, pat.span, expected, typ);
          }
        }

        let canon_id = pcx.map.get(ast_util::path_to_ident(name));
        if canon_id != pat.id {
            let tv_id = lookup_local(fcx, pat.span, canon_id);
            let ct = ty::mk_var(tcx, tv_id);
            demand::eqtype(fcx, pat.span, ct, typ);
        }
        fcx.write_ty(pat.id, typ);

        debug!("(checking match) writing type for pat id %d", pat.id);

        match sub {
          Some(p) => check_pat(pcx, p, expected),
          _ => ()
        }
      }
      ast::pat_ident(_, path, _) => {
        check_pat_variant(pcx, pat, path, Some(~[]), expected);
      }
      ast::pat_enum(path, subpats) => {
        check_pat_variant(pcx, pat, path, subpats, expected);
      }
      ast::pat_rec(fields, etc) => {
        let ex_fields = match structure_of(fcx, pat.span, expected) {
          ty::ty_rec(fields) => fields,
          _ => {
            tcx.sess.span_fatal
                (pat.span,
                fmt!("mismatched types: expected `%s` but found record",
                     fcx.infcx().ty_to_str(expected)));
          }
        };
        let f_count = vec::len(fields);
        let ex_f_count = vec::len(ex_fields);
        if ex_f_count < f_count || !etc && ex_f_count > f_count {
            tcx.sess.span_fatal
                (pat.span, fmt!("mismatched types: expected a record \
                      with %u fields, found one with %u \
                      fields",
                                ex_f_count, f_count));
        }

        for fields.each |f| {
            match vec::find(ex_fields, |a| f.ident == a.ident) {
              Some(field) => {
                check_pat(pcx, f.pat, field.mt.ty);
              }
              None => {
                tcx.sess.span_fatal(pat.span,
                                    fmt!("mismatched types: did not \
                                          expect a record with a field `%s`",
                                          tcx.sess.str_of(f.ident)));
              }
            }
        }
        fcx.write_ty(pat.id, expected);
      }
      ast::pat_struct(path, fields, etc) => {
        // Grab the class data that we care about.
        let structure = structure_of(fcx, pat.span, expected);
        match structure {
            ty::ty_struct(cid, ref substs) => {
                check_struct_pat(pcx, pat.id, pat.span, expected, path,
                                 fields, etc, cid, substs);
            }
            ty::ty_enum(eid, ref substs) => {
                check_struct_like_enum_variant_pat(
                    pcx, pat.id, pat.span, expected, path, fields, etc, eid,
                    substs);
            }
            _ => {
                // XXX: This should not be fatal.
                tcx.sess.span_fatal(pat.span,
                                    fmt!("mismatched types: expected `%s` \
                                          but found struct",
                                         fcx.infcx().ty_to_str(expected)));
            }
        }

        // Finally, write in the type.
        fcx.write_ty(pat.id, expected);
      }
      ast::pat_tup(elts) => {
        let ex_elts = match structure_of(fcx, pat.span, expected) {
          ty::ty_tup(elts) => elts,
          _ => {
            tcx.sess.span_fatal
                (pat.span,
                 fmt!("mismatched types: expected `%s`, found tuple",
                      fcx.infcx().ty_to_str(expected)));
          }
        };
        let e_count = vec::len(elts);
        if e_count != vec::len(ex_elts) {
            tcx.sess.span_fatal
                (pat.span, fmt!("mismatched types: expected a tuple \
                      with %u fields, found one with %u \
                      fields", vec::len(ex_elts), e_count));
        }
        let mut i = 0u;
        for elts.each |elt| {
            check_pat(pcx, *elt, ex_elts[i]);
            i += 1u;
        }

        fcx.write_ty(pat.id, expected);
      }
      ast::pat_box(inner) => {
        match structure_of(fcx, pat.span, expected) {
          ty::ty_box(e_inner) => {
            check_pat(pcx, inner, e_inner.ty);
            fcx.write_ty(pat.id, expected);
          }
          _ => {
            tcx.sess.span_fatal(
                pat.span,
                ~"mismatched types: expected `" +
                fcx.infcx().ty_to_str(expected) +
                ~"` found box");
          }
        }
      }
      ast::pat_uniq(inner) => {
        match structure_of(fcx, pat.span, expected) {
          ty::ty_uniq(e_inner) => {
            check_pat(pcx, inner, e_inner.ty);
            fcx.write_ty(pat.id, expected);
          }
          _ => {
            tcx.sess.span_fatal(
                pat.span,
                ~"mismatched types: expected `" +
                fcx.infcx().ty_to_str(expected) +
                ~"` found uniq");
          }
        }
      }
      ast::pat_region(inner) => {
        match structure_of(fcx, pat.span, expected) {
          ty::ty_rptr(_, e_inner) => {
            check_pat(pcx, inner, e_inner.ty);
            fcx.write_ty(pat.id, expected);
          }
          _ => {
            tcx.sess.span_fatal(
                pat.span,
                ~"mismatched types: expected `" +
                fcx.infcx().ty_to_str(expected) +
                ~"` found borrowed pointer");
          }
        }
      }
      ast::pat_vec(elts, tail) => {
        let default_region_var =
            fcx.infcx().next_region_var_with_lb(
                pat.span, pcx.block_region
            );

        let (elt_type, region_var) = match structure_of(
          fcx, pat.span, expected
        ) {
          ty::ty_evec(mt, vstore) => {
            let region_var = match vstore {
                ty::vstore_slice(r) => r,
                ty::vstore_box | ty::vstore_uniq | ty::vstore_fixed(_) => {
                    default_region_var
                }
            };
            (mt, region_var)
          }
          ty::ty_unboxed_vec(mt) => {
            (mt, default_region_var)
          },
          _ => {
            tcx.sess.span_fatal(
                pat.span,
                fmt!("mismatched type: expected `%s` but found vector",
                     fcx.infcx().ty_to_str(expected))
            );
          }
        };
        for elts.each |elt| {
            check_pat(pcx, *elt, elt_type.ty);
        }
        fcx.write_ty(pat.id, expected);

        match tail {
            Some(tail_pat) => {
                let slice_ty = ty::mk_evec(tcx,
                    ty::mt {ty: elt_type.ty, mutbl: elt_type.mutbl},
                    ty::vstore_slice(region_var)
                );
                check_pat(pcx, tail_pat, slice_ty);
            }
            None => ()
        }
      }
    }
}

