use std::map::HashMap;
use driver::session::session;
use lib::llvm::{TypeRef, ValueRef};
use syntax::ast;
use back::abi;
use common::*;
use build::*;
use base::*;
use type_of::*;
use ast::def_id;
use util::ppaux::ty_to_str;
use datum::*;
use callee::{ArgVals, DontAutorefArg};
use expr::SaveIn;

enum reflector = {
    visitor_val: ValueRef,
    visitor_methods: @~[ty::method],
    final_bcx: block,
    tydesc_ty: TypeRef,
    mut bcx: block
};

impl reflector {

    fn c_uint(u: uint) -> ValueRef {
        C_uint(self.bcx.ccx(), u)
    }

    fn c_int(i: int) -> ValueRef {
        C_int(self.bcx.ccx(), i)
    }

    fn c_slice(s: ~str) -> ValueRef {
        let ss = C_estr_slice(self.bcx.ccx(), s);
        do_spill_noroot(self.bcx, ss)
    }

    fn c_size_and_align(t: ty::t) -> ~[ValueRef] {
        let tr = type_of::type_of(self.bcx.ccx(), t);
        let s = shape::llsize_of_real(self.bcx.ccx(), tr);
        let a = shape::llalign_of_min(self.bcx.ccx(), tr);
        return ~[self.c_uint(s),
             self.c_uint(a)];
    }

    fn c_tydesc(t: ty::t) -> ValueRef {
        let bcx = self.bcx;
        let static_ti = get_tydesc(bcx.ccx(), t);
        glue::lazily_emit_all_tydesc_glue(bcx.ccx(), static_ti);
        PointerCast(bcx, static_ti.tydesc, T_ptr(self.tydesc_ty))
    }

    fn c_mt(mt: ty::mt) -> ~[ValueRef] {
        ~[self.c_uint(mt.mutbl as uint),
          self.c_tydesc(mt.ty)]
    }

    fn visit(ty_name: ~str, args: ~[ValueRef]) {
        let tcx = self.bcx.tcx();
        let mth_idx = option::get(ty::method_idx(
            tcx.sess.ident_of(~"visit_" + ty_name),
            *self.visitor_methods));
        let mth_ty = ty::mk_fn(tcx, self.visitor_methods[mth_idx].fty);
        let v = self.visitor_val;
        debug!("passing %u args:", vec::len(args));
        let bcx = self.bcx;
        for args.eachi |i, a| {
            debug!("arg %u: %s", i, val_str(bcx.ccx().tn, a));
        }
        let bool_ty = ty::mk_bool(tcx);
        let scratch = scratch_datum(bcx, bool_ty, false);
        let bcx = callee::trans_call_inner(
            self.bcx, None, mth_ty, bool_ty,
            |bcx| meth::trans_trait_callee_from_llval(bcx, mth_ty,
                                                      mth_idx, v),
            ArgVals(args), SaveIn(scratch.val), DontAutorefArg);
        let result = scratch.to_value_llval(bcx);
        let next_bcx = sub_block(bcx, ~"next");
        CondBr(bcx, result, next_bcx.llbb, self.final_bcx.llbb);
        self.bcx = next_bcx
    }

    fn bracketed(bracket_name: ~str, extra: ~[ValueRef],
                 inner: fn()) {
        self.visit(~"enter_" + bracket_name, extra);
        inner();
        self.visit(~"leave_" + bracket_name, extra);
    }

    fn vstore_name_and_extra(t: ty::t,
                             vstore: ty::vstore,
                             f: fn(~str,~[ValueRef])) {
        match vstore {
          ty::vstore_fixed(n) => {
            let extra = vec::append(~[self.c_uint(n)],
                                    self.c_size_and_align(t));
            f(~"fixed", extra)
          }
          ty::vstore_slice(_) => f(~"slice", ~[]),
          ty::vstore_uniq => f(~"uniq", ~[]),
          ty::vstore_box => f(~"box", ~[])
        }
    }

    fn leaf(name: ~str) {
        self.visit(name, ~[]);
    }

    // Entrypoint
    fn visit_ty(t: ty::t) {

        let bcx = self.bcx;
        debug!("reflect::visit_ty %s",
               ty_to_str(bcx.ccx().tcx, t));

        match ty::get(t).sty {
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

          ty::ty_unboxed_vec(mt) => self.visit(~"vec", self.c_mt(mt)),
          ty::ty_estr(vst) => {
            do self.vstore_name_and_extra(t, vst) |name, extra| {
                self.visit(~"estr_" + name, extra)
            }
          }
          ty::ty_evec(mt, vst) => {
            do self.vstore_name_and_extra(t, vst) |name, extra| {
                self.visit(~"evec_" + name, extra +
                           self.c_mt(mt))
            }
          }
          ty::ty_box(mt) => self.visit(~"box", self.c_mt(mt)),
          ty::ty_uniq(mt) => self.visit(~"uniq", self.c_mt(mt)),
          ty::ty_ptr(mt) => self.visit(~"ptr", self.c_mt(mt)),
          ty::ty_rptr(_, mt) => self.visit(~"rptr", self.c_mt(mt)),

          ty::ty_rec(fields) => {
            do self.bracketed(~"rec",
                              ~[self.c_uint(vec::len(fields))]
                              + self.c_size_and_align(t)) {
                for fields.eachi |i, field| {
                    self.visit(~"rec_field",
                               ~[self.c_uint(i),
                                 self.c_slice(
                                     bcx.ccx().sess.str_of(field.ident))]
                               + self.c_mt(field.mt));
                }
            }
          }

          ty::ty_tup(tys) => {
            do self.bracketed(~"tup",
                              ~[self.c_uint(vec::len(tys))]
                              + self.c_size_and_align(t)) {
                for tys.eachi |i, t| {
                    self.visit(~"tup_field",
                               ~[self.c_uint(i),
                                 self.c_tydesc(t)]);
                }
            }
          }

          // FIXME (#2594): fetch constants out of intrinsic:: for the
          // numbers.
          ty::ty_fn(ref fty) => {
            let pureval = match fty.meta.purity {
              ast::pure_fn => 0u,
              ast::unsafe_fn => 1u,
              ast::impure_fn => 2u,
              ast::extern_fn => 3u
            };
            let protoval = match fty.meta.proto {
              ty::proto_bare => 0u,
              ty::proto_vstore(ty::vstore_uniq) => 2u,
              ty::proto_vstore(ty::vstore_box) => 3u,
              ty::proto_vstore(ty::vstore_slice(_)) => 4u,
              ty::proto_vstore(ty::vstore_fixed(_)) =>
                fail ~"fixed unexpected"
            };
            let retval = match fty.meta.ret_style {
              ast::noreturn => 0u,
              ast::return_val => 1u
            };
            let extra = ~[self.c_uint(pureval),
                          self.c_uint(protoval),
                          self.c_uint(vec::len(fty.sig.inputs)),
                          self.c_uint(retval)];
            self.visit(~"enter_fn", extra);
            for fty.sig.inputs.eachi |i, arg| {
                let modeval = match arg.mode {
                  ast::infer(_) => 0u,
                  ast::expl(e) => match e {
                    ast::by_ref => 1u,
                    ast::by_val => 2u,
                    ast::by_mutbl_ref => 3u,
                    ast::by_move => 4u,
                    ast::by_copy => 5u
                  }
                };
                self.visit(~"fn_input",
                           ~[self.c_uint(i),
                             self.c_uint(modeval),
                             self.c_tydesc(arg.ty)]);
            }
            self.visit(~"fn_output",
                       ~[self.c_uint(retval),
                         self.c_tydesc(fty.sig.output)]);
            self.visit(~"leave_fn", extra);
          }

          ty::ty_class(did, ref substs) => {
            let bcx = self.bcx;
            let tcx = bcx.ccx().tcx;
            let fields = ty::class_items_as_fields(tcx, did, substs);

            do self.bracketed(~"class", ~[self.c_uint(vec::len(fields))]
                              + self.c_size_and_align(t)) {
                for fields.eachi |i, field| {
                    self.visit(~"class_field",
                               ~[self.c_uint(i),
                                 self.c_slice(
                                     bcx.ccx().sess.str_of(field.ident))]
                               + self.c_mt(field.mt));
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

            do self.bracketed(~"enum",
                              ~[self.c_uint(vec::len(variants))]
                              + self.c_size_and_align(t)) {
                for variants.eachi |i, v| {
                    do self.bracketed(~"enum_variant",
                                      ~[self.c_uint(i),
                                        self.c_int(v.disr_val),
                                        self.c_uint(vec::len(v.args)),
                                        self.c_slice(
                                            bcx.ccx().sess.str_of(v.name))]) {
                        for v.args.eachi |j, a| {
                            self.visit(~"enum_variant_field",
                                       ~[self.c_uint(j),
                                         self.c_tydesc(a)]);
                        }
                    }
                }
            }
          }

          // Miscallaneous extra types
          ty::ty_trait(_, _, _) => self.leaf(~"trait"),
          ty::ty_infer(_) => self.leaf(~"infer"),
          ty::ty_param(p) => self.visit(~"param", ~[self.c_uint(p.idx)]),
          ty::ty_self => self.leaf(~"self"),
          ty::ty_type => self.leaf(~"type"),
          ty::ty_opaque_box => self.leaf(~"opaque_box"),
          ty::ty_opaque_closure_ptr(ck) => {
            let ckval = match ck {
              ty::ck_block => 0u,
              ty::ck_box => 1u,
              ty::ck_uniq => 2u
            };
            self.visit(~"closure_ptr", ~[self.c_uint(ckval)])
          }
        }
    }
}

// Emit a sequence of calls to visit_ty::visit_foo
fn emit_calls_to_trait_visit_ty(bcx: block, t: ty::t,
                                visitor_val: ValueRef,
                                visitor_trait_id: def_id) -> block {
    use syntax::parse::token::special_idents::tydesc;
    let final = sub_block(bcx, ~"final");
    assert bcx.ccx().tcx.intrinsic_defs.contains_key(tydesc);
    let (_, tydesc_ty) = bcx.ccx().tcx.intrinsic_defs.get(tydesc);
    let tydesc_ty = type_of::type_of(bcx.ccx(), tydesc_ty);
    let r = reflector({
        visitor_val: visitor_val,
        visitor_methods: ty::trait_methods(bcx.tcx(), visitor_trait_id),
        final_bcx: final,
        tydesc_ty: tydesc_ty,
        mut bcx: bcx
    });
    r.visit_ty(t);
    Br(r.bcx, final.llbb);
    return final;
}
