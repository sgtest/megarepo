// Type encoding

use io::WriterUtil;
use std::map::HashMap;
use syntax::ast::*;
use syntax::diagnostic::span_handler;
use middle::ty;
use middle::ty::vid;
use syntax::print::pprust::*;

export ctxt;
export ty_abbrev;
export ac_no_abbrevs;
export ac_use_abbrevs;
export enc_ty;
export enc_bounds;
export enc_mode;
export enc_arg;

type ctxt = {
    diag: span_handler,
    // Def -> str Callback:
    ds: fn@(def_id) -> ~str,
    // The type context.
    tcx: ty::ctxt,
    reachable: fn@(node_id) -> bool,
    abbrevs: abbrev_ctxt
};

// Compact string representation for ty.t values. API ty_str & parse_from_str.
// Extra parameters are for converting to/from def_ids in the string rep.
// Whatever format you choose should not contain pipe characters.
type ty_abbrev = {pos: uint, len: uint, s: @~str};

enum abbrev_ctxt { ac_no_abbrevs, ac_use_abbrevs(HashMap<ty::t, ty_abbrev>), }

fn cx_uses_abbrevs(cx: @ctxt) -> bool {
    match cx.abbrevs {
      ac_no_abbrevs => return false,
      ac_use_abbrevs(_) => return true
    }
}

fn enc_ty(w: io::Writer, cx: @ctxt, t: ty::t) {
    match cx.abbrevs {
      ac_no_abbrevs => {
        let result_str = match cx.tcx.short_names_cache.find(t) {
            Some(s) => *s,
            None => {
                let s = do io::with_str_writer |wr| {
                    enc_sty(wr, cx, ty::get(t).sty);
                };
                cx.tcx.short_names_cache.insert(t, @s);
                s
          }
        };
        w.write_str(result_str);
      }
      ac_use_abbrevs(abbrevs) => {
        match abbrevs.find(t) {
          Some(a) => { w.write_str(*a.s); return; }
          None => {
            let pos = w.tell();
            match ty::type_def_id(t) {
              Some(def_id) => {
                // Do not emit node ids that map to unexported names.  Those
                // are not helpful.
                if def_id.crate != local_crate ||
                    cx.reachable(def_id.node) {
                    w.write_char('"');
                    w.write_str(cx.ds(def_id));
                    w.write_char('|');
                }
              }
              _ => {}
            }
            enc_sty(w, cx, ty::get(t).sty);
            let end = w.tell();
            let len = end - pos;
            fn estimate_sz(u: uint) -> uint {
                let mut n = u;
                let mut len = 0u;
                while n != 0u { len += 1u; n = n >> 4u; }
                return len;
            }
            let abbrev_len = 3u + estimate_sz(pos) + estimate_sz(len);
            if abbrev_len < len {
                // I.e. it's actually an abbreviation.
                let s = ~"#" + uint::to_str(pos, 16u) + ~":" +
                    uint::to_str(len, 16u) + ~"#";
                let a = {pos: pos, len: len, s: @s};
                abbrevs.insert(t, a);
            }
            return;
          }
        }
      }
    }
}
fn enc_mt(w: io::Writer, cx: @ctxt, mt: ty::mt) {
    match mt.mutbl {
      m_imm => (),
      m_mutbl => w.write_char('m'),
      m_const => w.write_char('?')
    }
    enc_ty(w, cx, mt.ty);
}

fn enc_opt<T>(w: io::Writer, t: Option<T>, enc_f: fn(T)) {
    match t {
      None => w.write_char('n'),
      Some(v) => {
        w.write_char('s');
        enc_f(v);
      }
    }
}

fn enc_substs(w: io::Writer, cx: @ctxt, substs: ty::substs) {
    do enc_opt(w, substs.self_r) |r| { enc_region(w, cx, r) }
    do enc_opt(w, substs.self_ty) |t| { enc_ty(w, cx, t) }
    w.write_char('[');
    for substs.tps.each |t| { enc_ty(w, cx, t); }
    w.write_char(']');
}

fn enc_region(w: io::Writer, cx: @ctxt, r: ty::region) {
    match r {
      ty::re_bound(br) => {
        w.write_char('b');
        enc_bound_region(w, cx, br);
      }
      ty::re_free(id, br) => {
        w.write_char('f');
        w.write_char('[');
        w.write_int(id);
        w.write_char('|');
        enc_bound_region(w, cx, br);
        w.write_char(']');
      }
      ty::re_scope(nid) => {
        w.write_char('s');
        w.write_int(nid);
        w.write_char('|');
      }
      ty::re_static => {
        w.write_char('t');
      }
      ty::re_var(_) => {
        // these should not crop up after typeck
        cx.diag.handler().bug(~"Cannot encode region variables");
      }
    }
}

fn enc_bound_region(w: io::Writer, cx: @ctxt, br: ty::bound_region) {
    match br {
      ty::br_self => w.write_char('s'),
      ty::br_anon(idx) => {
        w.write_char('a');
        w.write_uint(idx);
        w.write_char('|');
      }
      ty::br_named(s) => {
        w.write_char('[');
        w.write_str(cx.tcx.sess.str_of(s));
        w.write_char(']')
      }
      ty::br_cap_avoid(id, br) => {
        w.write_char('c');
        w.write_int(id);
        w.write_char('|');
        enc_bound_region(w, cx, *br);
      }
    }
}

fn enc_vstore(w: io::Writer, cx: @ctxt, v: ty::vstore) {
    w.write_char('/');
    match v {
      ty::vstore_fixed(u) => {
        w.write_uint(u);
        w.write_char('|');
      }
      ty::vstore_uniq => {
        w.write_char('~');
      }
      ty::vstore_box => {
        w.write_char('@');
      }
      ty::vstore_slice(r) => {
        w.write_char('&');
        enc_region(w, cx, r);
      }
    }
}

fn enc_sty(w: io::Writer, cx: @ctxt, st: ty::sty) {
    match st {
      ty::ty_nil => w.write_char('n'),
      ty::ty_bot => w.write_char('z'),
      ty::ty_bool => w.write_char('b'),
      ty::ty_int(t) => {
        match t {
          ty_i => w.write_char('i'),
          ty_char => w.write_char('c'),
          ty_i8 => w.write_str(&"MB"),
          ty_i16 => w.write_str(&"MW"),
          ty_i32 => w.write_str(&"ML"),
          ty_i64 => w.write_str(&"MD")
        }
      }
      ty::ty_uint(t) => {
        match t {
          ty_u => w.write_char('u'),
          ty_u8 => w.write_str(&"Mb"),
          ty_u16 => w.write_str(&"Mw"),
          ty_u32 => w.write_str(&"Ml"),
          ty_u64 => w.write_str(&"Md")
        }
      }
      ty::ty_float(t) => {
        match t {
          ty_f => w.write_char('l'),
          ty_f32 => w.write_str(&"Mf"),
          ty_f64 => w.write_str(&"MF"),
        }
      }
      ty::ty_enum(def, substs) => {
        w.write_str(&"t[");
        w.write_str(cx.ds(def));
        w.write_char('|');
        enc_substs(w, cx, substs);
        w.write_char(']');
      }
      ty::ty_trait(def, substs, vstore) => {
        w.write_str(&"x[");
        w.write_str(cx.ds(def));
        w.write_char('|');
        enc_substs(w, cx, substs);
        enc_vstore(w, cx, vstore);
        w.write_char(']');
      }
      ty::ty_tup(ts) => {
        w.write_str(&"T[");
        for ts.each |t| { enc_ty(w, cx, t); }
        w.write_char(']');
      }
      ty::ty_box(mt) => { w.write_char('@'); enc_mt(w, cx, mt); }
      ty::ty_uniq(mt) => { w.write_char('~'); enc_mt(w, cx, mt); }
      ty::ty_ptr(mt) => { w.write_char('*'); enc_mt(w, cx, mt); }
      ty::ty_rptr(r, mt) => {
        w.write_char('&');
        enc_region(w, cx, r);
        enc_mt(w, cx, mt);
      }
      ty::ty_evec(mt, v) => {
        w.write_char('V');
        enc_mt(w, cx, mt);
        enc_vstore(w, cx, v);
      }
      ty::ty_estr(v) => {
        w.write_char('v');
        enc_vstore(w, cx, v);
      }
      ty::ty_unboxed_vec(mt) => { w.write_char('U'); enc_mt(w, cx, mt); }
      ty::ty_rec(fields) => {
        w.write_str(&"R[");
        for fields.each |field| {
            w.write_str(cx.tcx.sess.str_of(field.ident));
            w.write_char('=');
            enc_mt(w, cx, field.mt);
        }
        w.write_char(']');
      }
      ty::ty_fn(f) => {
        enc_ty_fn(w, cx, f);
      }
      ty::ty_infer(ty::TyVar(id)) => {
        w.write_char('X');
        w.write_uint(id.to_uint());
      }
      ty::ty_infer(ty::IntVar(id)) => {
        w.write_char('X');
        w.write_char('I');
        w.write_uint(id.to_uint());
      }
      ty::ty_param({idx: id, def_id: did}) => {
        w.write_char('p');
        w.write_str(cx.ds(did));
        w.write_char('|');
        w.write_str(uint::str(id));
      }
      ty::ty_self => {
        w.write_char('s');
      }
      ty::ty_type => w.write_char('Y'),
      ty::ty_opaque_closure_ptr(ty::ck_block) => w.write_str(&"C&"),
      ty::ty_opaque_closure_ptr(ty::ck_box) => w.write_str(&"C@"),
      ty::ty_opaque_closure_ptr(ty::ck_uniq) => w.write_str(&"C~"),
      ty::ty_opaque_box => w.write_char('B'),
      ty::ty_class(def, substs) => {
          debug!("~~~~ %s", ~"a[");
          w.write_str(&"a[");
          let s = cx.ds(def);
          debug!("~~~~ %s", s);
          w.write_str(s);
          debug!("~~~~ %s", ~"|");
          w.write_char('|');
          enc_substs(w, cx, substs);
          debug!("~~~~ %s", ~"]");
          w.write_char(']');
      }
    }
}

fn enc_proto(w: io::Writer, cx: @ctxt, proto: ty::fn_proto) {
    w.write_str(&"f");
    match proto {
        ty::proto_bare => w.write_str(&"n"),
        ty::proto_vstore(vstore) => {
            w.write_str(&"v");
            enc_vstore(w, cx, vstore);
        }
    }
}

fn enc_arg(w: io::Writer, cx: @ctxt, arg: ty::arg) {
    enc_mode(w, cx, arg.mode);
    enc_ty(w, cx, arg.ty);
}

fn enc_mode(w: io::Writer, cx: @ctxt, m: mode) {
    match ty::resolved_mode(cx.tcx, m) {
      by_mutbl_ref => w.write_char('&'),
      by_move => w.write_char('-'),
      by_copy => w.write_char('+'),
      by_ref => w.write_char('='),
      by_val => w.write_char('#')
    }
}

fn enc_purity(w: io::Writer, p: purity) {
    match p {
      pure_fn => w.write_char('p'),
      impure_fn => w.write_char('i'),
      unsafe_fn => w.write_char('u'),
      extern_fn => w.write_char('c')
    }
}

fn enc_ty_fn(w: io::Writer, cx: @ctxt, ft: ty::FnTy) {
    enc_proto(w, cx, ft.meta.proto);
    enc_purity(w, ft.meta.purity);
    enc_bounds(w, cx, ft.meta.bounds);
    w.write_char('[');
    for ft.sig.inputs.each |arg| {
        enc_arg(w, cx, arg);
    }
    w.write_char(']');
    match ft.meta.ret_style {
      noreturn => w.write_char('!'),
      _ => enc_ty(w, cx, ft.sig.output)
    }
}

fn enc_bounds(w: io::Writer, cx: @ctxt, bs: @~[ty::param_bound]) {
    for vec::each(*bs) |bound| {
        match bound {
          ty::bound_send => w.write_char('S'),
          ty::bound_copy => w.write_char('C'),
          ty::bound_const => w.write_char('K'),
          ty::bound_owned => w.write_char('O'),
          ty::bound_trait(tp) => {
            w.write_char('I');
            enc_ty(w, cx, tp);
          }
        }
    }
    w.write_char('.');
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
