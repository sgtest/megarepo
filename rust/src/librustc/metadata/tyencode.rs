// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Type encoding

#![allow(unused_must_use)] // as with encoding, everything is a no-fail MemWriter
#![allow(non_camel_case_types)]

use std::cell::RefCell;
use collections::HashMap;
use std::io;
use std::io::MemWriter;
use std::str;
use std::fmt;

use middle::ty::param_ty;
use middle::ty;

use syntax::abi::Abi;
use syntax::ast;
use syntax::ast::*;
use syntax::diagnostic::SpanHandler;
use syntax::parse::token;

macro_rules! mywrite( ($wr:expr, $($arg:tt)*) => (
    format_args!(|a| { mywrite($wr, a) }, $($arg)*)
) )

pub struct ctxt<'a> {
    pub diag: &'a SpanHandler,
    // Def -> str Callback:
    pub ds: fn(DefId) -> ~str,
    // The type context.
    pub tcx: &'a ty::ctxt,
    pub abbrevs: abbrev_ctxt
}

// Compact string representation for ty.t values. API ty_str & parse_from_str.
// Extra parameters are for converting to/from def_ids in the string rep.
// Whatever format you choose should not contain pipe characters.
pub struct ty_abbrev {
    pos: uint,
    len: uint,
    s: ~str
}

pub enum abbrev_ctxt {
    ac_no_abbrevs,
    ac_use_abbrevs(@RefCell<HashMap<ty::t, ty_abbrev>>),
}

fn mywrite(w: &mut MemWriter, fmt: &fmt::Arguments) {
    fmt::write(&mut *w as &mut io::Writer, fmt);
}

pub fn enc_ty(w: &mut MemWriter, cx: &ctxt, t: ty::t) {
    match cx.abbrevs {
      ac_no_abbrevs => {
          let result_str_opt = cx.tcx.short_names_cache.borrow()
                                            .find(&t)
                                            .map(|result| {
                                                (*result).clone()
                                            });
          let result_str = match result_str_opt {
            Some(s) => s,
            None => {
                let wr = &mut MemWriter::new();
                enc_sty(wr, cx, &ty::get(t).sty);
                let s = str::from_utf8(wr.get_ref()).unwrap();
                cx.tcx.short_names_cache.borrow_mut().insert(t, s.to_str());
                s.to_str()
            }
          };
          w.write(result_str.as_bytes());
      }
      ac_use_abbrevs(abbrevs) => {
          match abbrevs.borrow_mut().find(&t) {
              Some(a) => { w.write(a.s.as_bytes()); return; }
              None => {}
          }
          let pos = w.tell().unwrap();
          enc_sty(w, cx, &ty::get(t).sty);
          let end = w.tell().unwrap();
          let len = end - pos;
          fn estimate_sz(u: u64) -> u64 {
              let mut n = u;
              let mut len = 0;
              while n != 0 { len += 1; n = n >> 4; }
              return len;
          }
          let abbrev_len = 3 + estimate_sz(pos) + estimate_sz(len);
          if abbrev_len < len {
              // I.e. it's actually an abbreviation.
              let s = format!("\\#{:x}:{:x}\\#", pos, len);
              let a = ty_abbrev { pos: pos as uint,
                                  len: len as uint,
                                  s: s };
              abbrevs.borrow_mut().insert(t, a);
          }
          return;
      }
    }
}

fn enc_mutability(w: &mut MemWriter, mt: ast::Mutability) {
    match mt {
        MutImmutable => (),
        MutMutable => mywrite!(w, "m"),
    }
}

fn enc_mt(w: &mut MemWriter, cx: &ctxt, mt: ty::mt) {
    enc_mutability(w, mt.mutbl);
    enc_ty(w, cx, mt.ty);
}

fn enc_opt<T>(w: &mut MemWriter, t: Option<T>, enc_f: |&mut MemWriter, T|) {
    match t {
        None => mywrite!(w, "n"),
        Some(v) => {
            mywrite!(w, "s");
            enc_f(w, v);
        }
    }
}

pub fn enc_substs(w: &mut MemWriter, cx: &ctxt, substs: &ty::substs) {
    enc_region_substs(w, cx, &substs.regions);
    enc_opt(w, substs.self_ty, |w, t| enc_ty(w, cx, t));
    mywrite!(w, "[");
    for t in substs.tps.iter() { enc_ty(w, cx, *t); }
    mywrite!(w, "]");
}

fn enc_region_substs(w: &mut MemWriter, cx: &ctxt, substs: &ty::RegionSubsts) {
    match *substs {
        ty::ErasedRegions => {
            mywrite!(w, "e");
        }
        ty::NonerasedRegions(ref regions) => {
            mywrite!(w, "n");
            for &r in regions.iter() {
                enc_region(w, cx, r);
            }
            mywrite!(w, ".");
        }
    }
}

fn enc_region(w: &mut MemWriter, cx: &ctxt, r: ty::Region) {
    match r {
        ty::ReLateBound(id, br) => {
            mywrite!(w, "b[{}|", id);
            enc_bound_region(w, cx, br);
            mywrite!(w, "]");
        }
        ty::ReEarlyBound(node_id, index, name) => {
            mywrite!(w, "B[{}|{}|{}]",
                     node_id,
                     index,
                     token::get_name(name));
        }
        ty::ReFree(ref fr) => {
            mywrite!(w, "f[{}|", fr.scope_id);
            enc_bound_region(w, cx, fr.bound_region);
            mywrite!(w, "]");
        }
        ty::ReScope(nid) => {
            mywrite!(w, "s{}|", nid);
        }
        ty::ReStatic => {
            mywrite!(w, "t");
        }
        ty::ReEmpty => {
            mywrite!(w, "e");
        }
        ty::ReInfer(_) => {
            // these should not crop up after typeck
            cx.diag.handler().bug("cannot encode region variables");
        }
    }
}

fn enc_bound_region(w: &mut MemWriter, cx: &ctxt, br: ty::BoundRegion) {
    match br {
        ty::BrAnon(idx) => {
            mywrite!(w, "a{}|", idx);
        }
        ty::BrNamed(d, name) => {
            mywrite!(w, "[{}|{}]",
                     (cx.ds)(d),
                     token::get_name(name));
        }
        ty::BrFresh(id) => {
            mywrite!(w, "f{}|", id);
        }
    }
}

pub fn enc_vstore<M>(w: &mut MemWriter, cx: &ctxt,
                     v: ty::Vstore<M>,
                     enc_mut: |&mut MemWriter, M|) {
    mywrite!(w, "/");
    match v {
        ty::VstoreFixed(u) => mywrite!(w, "{}|", u),
        ty::VstoreUniq => mywrite!(w, "~"),
        ty::VstoreSlice(r, m) => {
            mywrite!(w, "&");
            enc_region(w, cx, r);
            enc_mut(w, m);
        }
    }
}

pub fn enc_trait_ref(w: &mut MemWriter, cx: &ctxt, s: &ty::TraitRef) {
    mywrite!(w, "{}|", (cx.ds)(s.def_id));
    enc_substs(w, cx, &s.substs);
}

pub fn enc_trait_store(w: &mut MemWriter, cx: &ctxt, s: ty::TraitStore) {
    match s {
        ty::UniqTraitStore => mywrite!(w, "~"),
        ty::RegionTraitStore(re, m) => {
            mywrite!(w, "&");
            enc_region(w, cx, re);
            enc_mutability(w, m);
        }
    }
}

fn enc_sty(w: &mut MemWriter, cx: &ctxt, st: &ty::sty) {
    match *st {
        ty::ty_nil => mywrite!(w, "n"),
        ty::ty_bot => mywrite!(w, "z"),
        ty::ty_bool => mywrite!(w, "b"),
        ty::ty_char => mywrite!(w, "c"),
        ty::ty_int(t) => {
            match t {
                TyI => mywrite!(w, "i"),
                TyI8 => mywrite!(w, "MB"),
                TyI16 => mywrite!(w, "MW"),
                TyI32 => mywrite!(w, "ML"),
                TyI64 => mywrite!(w, "MD")
            }
        }
        ty::ty_uint(t) => {
            match t {
                TyU => mywrite!(w, "u"),
                TyU8 => mywrite!(w, "Mb"),
                TyU16 => mywrite!(w, "Mw"),
                TyU32 => mywrite!(w, "Ml"),
                TyU64 => mywrite!(w, "Md")
            }
        }
        ty::ty_float(t) => {
            match t {
                TyF32 => mywrite!(w, "Mf"),
                TyF64 => mywrite!(w, "MF"),
            }
        }
        ty::ty_enum(def, ref substs) => {
            mywrite!(w, "t[{}|", (cx.ds)(def));
            enc_substs(w, cx, substs);
            mywrite!(w, "]");
        }
        ty::ty_trait(~ty::TyTrait { def_id, ref substs, store, bounds }) => {
            mywrite!(w, "x[{}|", (cx.ds)(def_id));
            enc_substs(w, cx, substs);
            enc_trait_store(w, cx, store);
            let bounds = ty::ParamBounds {builtin_bounds: bounds,
                                          trait_bounds: Vec::new()};
            enc_bounds(w, cx, &bounds);
            mywrite!(w, "]");
        }
        ty::ty_tup(ref ts) => {
            mywrite!(w, "T[");
            for t in ts.iter() { enc_ty(w, cx, *t); }
            mywrite!(w, "]");
        }
        ty::ty_box(typ) => { mywrite!(w, "@"); enc_ty(w, cx, typ); }
        ty::ty_uniq(typ) => { mywrite!(w, "~"); enc_ty(w, cx, typ); }
        ty::ty_ptr(mt) => { mywrite!(w, "*"); enc_mt(w, cx, mt); }
        ty::ty_rptr(r, mt) => {
            mywrite!(w, "&");
            enc_region(w, cx, r);
            enc_mt(w, cx, mt);
        }
        ty::ty_vec(ty, v) => {
            mywrite!(w, "V");
            enc_ty(w, cx, ty);
            enc_vstore(w, cx, v, enc_mutability);
        }
        ty::ty_str(v) => {
            mywrite!(w, "v");
            enc_vstore(w, cx, v, |_, ()| {});
        }
        ty::ty_closure(ref f) => {
            mywrite!(w, "f");
            enc_closure_ty(w, cx, *f);
        }
        ty::ty_bare_fn(ref f) => {
            mywrite!(w, "F");
            enc_bare_fn_ty(w, cx, f);
        }
        ty::ty_infer(_) => {
            cx.diag.handler().bug("cannot encode inference variable types");
        }
        ty::ty_param(param_ty {idx: id, def_id: did}) => {
            mywrite!(w, "p{}|{}", (cx.ds)(did), id);
        }
        ty::ty_self(did) => {
            mywrite!(w, "s{}|", (cx.ds)(did));
        }
        ty::ty_struct(def, ref substs) => {
            mywrite!(w, "a[{}|", (cx.ds)(def));
            enc_substs(w, cx, substs);
            mywrite!(w, "]");
        }
        ty::ty_err => fail!("shouldn't encode error type")
    }
}

fn enc_sigil(w: &mut MemWriter, sigil: Sigil) {
    match sigil {
        ManagedSigil => mywrite!(w, "@"),
        OwnedSigil => mywrite!(w, "~"),
        BorrowedSigil => mywrite!(w, "&"),
    }
}

fn enc_fn_style(w: &mut MemWriter, p: FnStyle) {
    match p {
        NormalFn => mywrite!(w, "n"),
        UnsafeFn => mywrite!(w, "u"),
        ExternFn => mywrite!(w, "c")
    }
}

fn enc_abi(w: &mut MemWriter, abi: Abi) {
    mywrite!(w, "[");
    mywrite!(w, "{}", abi.name());
    mywrite!(w, "]")
}

fn enc_onceness(w: &mut MemWriter, o: Onceness) {
    match o {
        Once => mywrite!(w, "o"),
        Many => mywrite!(w, "m")
    }
}

pub fn enc_bare_fn_ty(w: &mut MemWriter, cx: &ctxt, ft: &ty::BareFnTy) {
    enc_fn_style(w, ft.fn_style);
    enc_abi(w, ft.abi);
    enc_fn_sig(w, cx, &ft.sig);
}

fn enc_closure_ty(w: &mut MemWriter, cx: &ctxt, ft: &ty::ClosureTy) {
    enc_sigil(w, ft.sigil);
    enc_fn_style(w, ft.fn_style);
    enc_onceness(w, ft.onceness);
    enc_region(w, cx, ft.region);
    let bounds = ty::ParamBounds {builtin_bounds: ft.bounds,
                                  trait_bounds: Vec::new()};
    enc_bounds(w, cx, &bounds);
    enc_fn_sig(w, cx, &ft.sig);
}

fn enc_fn_sig(w: &mut MemWriter, cx: &ctxt, fsig: &ty::FnSig) {
    mywrite!(w, "[{}|", fsig.binder_id);
    for ty in fsig.inputs.iter() {
        enc_ty(w, cx, *ty);
    }
    mywrite!(w, "]");
    if fsig.variadic {
        mywrite!(w, "V");
    } else {
        mywrite!(w, "N");
    }
    enc_ty(w, cx, fsig.output);
}

fn enc_bounds(w: &mut MemWriter, cx: &ctxt, bs: &ty::ParamBounds) {
    for bound in bs.builtin_bounds.iter() {
        match bound {
            ty::BoundSend => mywrite!(w, "S"),
            ty::BoundStatic => mywrite!(w, "O"),
            ty::BoundSized => mywrite!(w, "Z"),
            ty::BoundCopy => mywrite!(w, "P"),
            ty::BoundShare => mywrite!(w, "T"),
        }
    }

    for &tp in bs.trait_bounds.iter() {
        mywrite!(w, "I");
        enc_trait_ref(w, cx, tp);
    }

    mywrite!(w, ".");
}

pub fn enc_type_param_def(w: &mut MemWriter, cx: &ctxt, v: &ty::TypeParameterDef) {
    mywrite!(w, "{}:{}|", token::get_ident(v.ident), (cx.ds)(v.def_id));
    enc_bounds(w, cx, v.bounds);
    enc_opt(w, v.default, |w, t| enc_ty(w, cx, t));
}
