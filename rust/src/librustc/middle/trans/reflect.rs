// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use lib::llvm::{TypeRef, ValueRef};
use middle::trans::base::*;
use middle::trans::build::*;
use middle::trans::callee::{ArgVals, DontAutorefArg};
use middle::trans::callee;
use middle::trans::common::*;
use middle::trans::datum::*;
use middle::trans::expr::SaveIn;
use middle::trans::glue;
use middle::trans::machine;
use middle::trans::meth;
use middle::trans::type_of::*;
use middle::ty;
use util::ppaux::ty_to_str;

use core::option::None;
use core::vec;
use syntax::ast::def_id;
use syntax::ast;

pub struct Reflector {
    visitor_val: ValueRef,
    visitor_methods: @~[ty::method],
    final_bcx: block,
    tydesc_ty: TypeRef,
    bcx: block
}

pub impl Reflector {
    fn c_uint(&mut self, u: uint) -> ValueRef {
        C_uint(self.bcx.ccx(), u)
    }

    fn c_int(&mut self, i: int) -> ValueRef {
        C_int(self.bcx.ccx(), i)
    }

    fn c_slice(&mut self, s: @~str) -> ValueRef {
        // We're careful to not use first class aggregates here because that
        // will kick us off fast isel. (Issue #4352.)
        let bcx = self.bcx;
        let str_vstore = ty::vstore_slice(ty::re_static);
        let str_ty = ty::mk_estr(bcx.tcx(), str_vstore);
        let scratch = scratch_datum(bcx, str_ty, false);
        let len = C_uint(bcx.ccx(), s.len() + 1);
        let c_str = PointerCast(bcx, C_cstr(bcx.ccx(), s), T_ptr(T_i8()));
        Store(bcx, c_str, GEPi(bcx, scratch.val, [ 0, 0 ]));
        Store(bcx, len, GEPi(bcx, scratch.val, [ 0, 1 ]));
        scratch.val
    }

    fn c_size_and_align(&mut self, t: ty::t) -> ~[ValueRef] {
        let tr = type_of(self.bcx.ccx(), t);
        let s = machine::llsize_of_real(self.bcx.ccx(), tr);
        let a = machine::llalign_of_min(self.bcx.ccx(), tr);
        return ~[self.c_uint(s),
             self.c_uint(a)];
    }

    fn c_tydesc(&mut self, t: ty::t) -> ValueRef {
        let bcx = self.bcx;
        let static_ti = get_tydesc(bcx.ccx(), t);
        glue::lazily_emit_all_tydesc_glue(bcx.ccx(), static_ti);
        PointerCast(bcx, static_ti.tydesc, T_ptr(self.tydesc_ty))
    }

    fn c_mt(&mut self, mt: ty::mt) -> ~[ValueRef] {
        ~[self.c_uint(mt.mutbl as uint),
          self.c_tydesc(mt.ty)]
    }

    fn visit(&mut self, ty_name: ~str, args: ~[ValueRef]) {
        let tcx = self.bcx.tcx();
        let mth_idx = ty::method_idx(
            tcx.sess.ident_of(~"visit_" + ty_name),
            *self.visitor_methods).expect(fmt!("Couldn't find visit method \
                                                for %s", ty_name));
        let mth_ty =
            ty::mk_bare_fn(tcx, copy self.visitor_methods[mth_idx].fty);
        let v = self.visitor_val;
        debug!("passing %u args:", vec::len(args));
        let bcx = self.bcx;
        for args.eachi |i, a| {
            debug!("arg %u: %s", i, val_str(bcx.ccx().tn, *a));
        }
        let bool_ty = ty::mk_bool(tcx);
        let scratch = scratch_datum(bcx, bool_ty, false);
        // XXX: Should not be vstore_box!
        let bcx = callee::trans_call_inner(
            self.bcx, None, mth_ty, bool_ty,
            |bcx| meth::trans_trait_callee_from_llval(bcx,
                                                      mth_ty,
                                                      mth_idx,
                                                      v,
                                                      ty::vstore_box,
                                                      ast::sty_region(
                                                        ast::m_imm)),
            ArgVals(args), SaveIn(scratch.val), DontAutorefArg);
        let result = scratch.to_value_llval(bcx);
        let result = bool_to_i1(bcx, result);
        let next_bcx = sub_block(bcx, ~"next");
        CondBr(bcx, result, next_bcx.llbb, self.final_bcx.llbb);
        self.bcx = next_bcx
    }

    fn bracketed(&mut self,
                 bracket_name: ~str,
                 +extra: ~[ValueRef],
                 inner: &fn(&mut Reflector)) {
        // XXX: Bad copy.
        self.visit(~"enter_" + bracket_name, copy extra);
        inner(self);
        self.visit(~"leave_" + bracket_name, extra);
    }

    fn vstore_name_and_extra(&mut self,
                             t: ty::t,
                             vstore: ty::vstore) -> (~str, ~[ValueRef])
    {
        match vstore {
            ty::vstore_fixed(n) => {
                let extra = vec::append(~[self.c_uint(n)],
                                        self.c_size_and_align(t));
                (~"fixed", extra)
            }
            ty::vstore_slice(_) => (~"slice", ~[]),
            ty::vstore_uniq => (~"uniq", ~[]),
            ty::vstore_box => (~"box", ~[])
        }
    }

    fn leaf(&mut self, +name: ~str) {
        self.visit(name, ~[]);
    }

    // Entrypoint
    fn visit_ty(&mut self, t: ty::t) {
        let bcx = self.bcx;
        debug!("reflect::visit_ty %s",
               ty_to_str(bcx.ccx().tcx, t));

        match /*bad*/copy ty::get(t).sty {
          ty::ty_bot => self.leaf(~"bot"),
          ty::ty_nil => self.leaf(~"nil"),
          ty::ty_bool => self.leaf(~"bool"),
          ty::ty_int(ast::ty_i) => self.leaf(~"int"),
          ty::ty_int(ast::ty_char) => self.leaf(~"char"),
          ty::ty_int(ast::ty_i8) => self.leaf(~"i8"),
          ty::ty_int(ast::ty_i16) => self.leaf(~"i16"),
          ty::ty_int(ast::ty_i32) => self.leaf(~"i32"),
          ty::ty_int(ast::ty_i64) => self.leaf(~"i64"),
          ty::ty_uint(ast::ty_u) => self.leaf(~"uint"),
          ty::ty_uint(ast::ty_u8) => self.leaf(~"u8"),
          ty::ty_uint(ast::ty_u16) => self.leaf(~"u16"),
          ty::ty_uint(ast::ty_u32) => self.leaf(~"u32"),
          ty::ty_uint(ast::ty_u64) => self.leaf(~"u64"),
          ty::ty_float(ast::ty_f) => self.leaf(~"float"),
          ty::ty_float(ast::ty_f32) => self.leaf(~"f32"),
          ty::ty_float(ast::ty_f64) => self.leaf(~"f64"),

          ty::ty_unboxed_vec(mt) => {
              let values = self.c_mt(mt);
              self.visit(~"vec", values)
          }

          ty::ty_estr(vst) => {
              let (name, extra) = self.vstore_name_and_extra(t, vst);
              self.visit(~"estr_" + name, extra)
          }
          ty::ty_evec(mt, vst) => {
              let (name, extra) = self.vstore_name_and_extra(t, vst);
              let extra = extra + self.c_mt(mt);
              self.visit(~"evec_" + name, extra)
          }
          ty::ty_box(mt) => {
              let extra = self.c_mt(mt);
              self.visit(~"box", extra)
          }
          ty::ty_uniq(mt) => {
              let extra = self.c_mt(mt);
              self.visit(~"uniq", extra)
          }
          ty::ty_ptr(mt) => {
              let extra = self.c_mt(mt);
              self.visit(~"ptr", extra)
          }
          ty::ty_rptr(_, mt) => {
              let extra = self.c_mt(mt);
              self.visit(~"rptr", extra)
          }

          ty::ty_tup(tys) => {
              let extra = ~[self.c_uint(vec::len(tys))]
                  + self.c_size_and_align(t);
              do self.bracketed(~"tup", extra) |this| {
                  for tys.eachi |i, t| {
                      let extra = ~[this.c_uint(i), this.c_tydesc(*t)];
                      this.visit(~"tup_field", extra);
                  }
              }
          }

          // FIXME (#2594): fetch constants out of intrinsic
          // FIXME (#4809): visitor should break out bare fns from other fns
          ty::ty_closure(ref fty) => {
            let pureval = ast_purity_constant(fty.purity);
            let sigilval = ast_sigil_constant(fty.sigil);
            let retval = if ty::type_is_bot(fty.sig.output) {0u} else {1u};
            let extra = ~[self.c_uint(pureval),
                          self.c_uint(sigilval),
                          self.c_uint(vec::len(fty.sig.inputs)),
                          self.c_uint(retval)];
            self.visit(~"enter_fn", copy extra);    // XXX: Bad copy.
            self.visit_sig(retval, &fty.sig);
            self.visit(~"leave_fn", extra);
          }

          // FIXME (#2594): fetch constants out of intrinsic:: for the
          // numbers.
          ty::ty_bare_fn(ref fty) => {
            let pureval = ast_purity_constant(fty.purity);
            let sigilval = 0u;
            let retval = if ty::type_is_bot(fty.sig.output) {0u} else {1u};
            let extra = ~[self.c_uint(pureval),
                          self.c_uint(sigilval),
                          self.c_uint(vec::len(fty.sig.inputs)),
                          self.c_uint(retval)];
            self.visit(~"enter_fn", copy extra);    // XXX: Bad copy.
            self.visit_sig(retval, &fty.sig);
            self.visit(~"leave_fn", extra);
          }

          ty::ty_struct(did, ref substs) => {
              let bcx = self.bcx;
              let tcx = bcx.ccx().tcx;
              let fields = ty::struct_fields(tcx, did, substs);

              let extra = ~[self.c_uint(fields.len())]
                  + self.c_size_and_align(t);
              do self.bracketed(~"class", extra) |this| {
                  for fields.eachi |i, field| {
                      let extra = ~[this.c_uint(i),
                                    this.c_slice(
                                        bcx.ccx().sess.str_of(field.ident))]
                          + this.c_mt(field.mt);
                      this.visit(~"class_field", extra);
                  }
              }
          }

          // FIXME (#2595): visiting all the variants in turn is probably
          // not ideal. It'll work but will get costly on big enums. Maybe
          // let the visitor tell us if it wants to visit only a particular
          // variant?
          ty::ty_enum(did, ref substs) => {
            let bcx = self.bcx;
            let tcx = bcx.ccx().tcx;
            let variants = ty::substd_enum_variants(tcx, did, substs);

            let extra = ~[self.c_uint(vec::len(variants))]
                + self.c_size_and_align(t);
            do self.bracketed(~"enum", extra) |this| {
                for variants.eachi |i, v| {
                    let extra1 = ~[this.c_uint(i),
                                   this.c_int(v.disr_val),
                                   this.c_uint(vec::len(v.args)),
                                   this.c_slice(
                                       bcx.ccx().sess.str_of(v.name))];
                    do this.bracketed(~"enum_variant", extra1) |this| {
                        for v.args.eachi |j, a| {
                            let extra = ~[this.c_uint(j),
                                          this.c_tydesc(*a)];
                            this.visit(~"enum_variant_field", extra);
                        }
                    }
                }
            }
          }

          // Miscallaneous extra types
          ty::ty_trait(_, _, _) => self.leaf(~"trait"),
          ty::ty_infer(_) => self.leaf(~"infer"),
          ty::ty_err => self.leaf(~"err"),
          ty::ty_param(p) => {
              let extra = ~[self.c_uint(p.idx)];
              self.visit(~"param", extra)
          }
          ty::ty_self => self.leaf(~"self"),
          ty::ty_type => self.leaf(~"type"),
          ty::ty_opaque_box => self.leaf(~"opaque_box"),
          ty::ty_opaque_closure_ptr(ck) => {
              let ckval = ast_sigil_constant(ck);
              let extra = ~[self.c_uint(ckval)];
              self.visit(~"closure_ptr", extra)
          }
        }
    }

    fn visit_sig(&mut self, retval: uint, sig: &ty::FnSig) {
        for sig.inputs.eachi |i, arg| {
            let modeval = match arg.mode {
                ast::infer(_) => 0u,
                ast::expl(e) => match e {
                    ast::by_ref => 1u,
                    ast::by_val => 2u,
                    ast::by_copy => 5u
                }
            };
            let extra = ~[self.c_uint(i),
                         self.c_uint(modeval),
                         self.c_tydesc(arg.ty)];
            self.visit(~"fn_input", extra);
        }
        let extra = ~[self.c_uint(retval),
                      self.c_tydesc(sig.output)];
        self.visit(~"fn_output", extra);
    }
}

// Emit a sequence of calls to visit_ty::visit_foo
pub fn emit_calls_to_trait_visit_ty(bcx: block,
                                    t: ty::t,
                                    visitor_val: ValueRef,
                                    visitor_trait_id: def_id)
                                 -> block {
    use syntax::parse::token::special_idents::tydesc;
    let final = sub_block(bcx, ~"final");
    fail_unless!(bcx.ccx().tcx.intrinsic_defs.contains_key(&tydesc));
    let (_, tydesc_ty) = bcx.ccx().tcx.intrinsic_defs.get(&tydesc);
    let tydesc_ty = type_of(bcx.ccx(), tydesc_ty);
    let mut r = Reflector {
        visitor_val: visitor_val,
        visitor_methods: ty::trait_methods(bcx.tcx(), visitor_trait_id),
        final_bcx: final,
        tydesc_ty: tydesc_ty,
        bcx: bcx
    };
    r.visit_ty(t);
    Br(r.bcx, final.llbb);
    return final;
}

pub fn ast_sigil_constant(sigil: ast::Sigil) -> uint {
    match sigil {
        ast::OwnedSigil => 2u,
        ast::ManagedSigil => 3u,
        ast::BorrowedSigil => 4u,
    }
}

pub fn ast_purity_constant(purity: ast::purity) -> uint {
    match purity {
        ast::pure_fn => 0u,
        ast::unsafe_fn => 1u,
        ast::impure_fn => 2u,
        ast::extern_fn => 3u
    }
}

