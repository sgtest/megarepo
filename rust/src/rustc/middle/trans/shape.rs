// A "shape" is a compact encoding of a type that is used by interpreted glue.
// This substitutes for the runtime tags used by e.g. MLs.

use lib::llvm::llvm;
use lib::llvm::{True, False, ModuleRef, TypeRef, ValueRef};
use driver::session;
use driver::session::session;
use trans::base;
use middle::trans::common::*;
use back::abi;
use middle::ty;
use middle::ty::field;
use syntax::ast;
use syntax::ast_util::dummy_sp;
use syntax::util::interner;
use util::ppaux::ty_to_str;
use syntax::codemap::span;
use dvec::DVec;

use std::map::HashMap;
use option::is_some;

use ty_ctxt = middle::ty::ctxt;

type nominal_id_ = {did: ast::def_id, parent_id: Option<ast::def_id>,
                    tps: ~[ty::t]};
type nominal_id = @nominal_id_;

#[cfg(stage0)]
impl nominal_id_ : core::cmp::Eq {
    pure fn eq(&&other: nominal_id_) -> bool {
        if self.did != other.did ||
            self.parent_id != other.parent_id {
            false
        } else {
            do vec::all2(self.tps, other.tps) |m_tp, n_tp| {
                ty::type_id(m_tp) == ty::type_id(n_tp)
            }
        }
    }
    pure fn ne(&&other: nominal_id_) -> bool {
        ! (self == other)
    }
}
#[cfg(stage1)]
#[cfg(stage2)]
impl nominal_id_ : core::cmp::Eq {
    pure fn eq(other: &nominal_id_) -> bool {
        if self.did != other.did ||
            self.parent_id != other.parent_id {
            false
        } else {
            do vec::all2(self.tps, other.tps) |m_tp, n_tp| {
                ty::type_id(m_tp) == ty::type_id(n_tp)
            }
        }
    }
    pure fn ne(other: &nominal_id_) -> bool {
        ! (self == *other)
    }
}

impl nominal_id_ : to_bytes::IterBytes {
    pure fn iter_bytes(lsb0: bool, f: to_bytes::Cb) {
        to_bytes::iter_bytes_2(&self.did, &self.parent_id, lsb0, f);
        for self.tps.each |t| {
            ty::type_id(*t).iter_bytes(lsb0, f);
        }
    }
}

fn mk_nominal_id(tcx: ty::ctxt, did: ast::def_id,
                 parent_id: Option<ast::def_id>,
                 tps: ~[ty::t]) -> nominal_id {
    let tps_norm = tps.map(|t| ty::normalize_ty(tcx, t));
    @{did: did, parent_id: parent_id, tps: tps_norm}
}

fn new_nominal_id_hash<T: Copy>() -> HashMap<nominal_id, T> {
    return HashMap();
}

type enum_data = {did: ast::def_id, substs: ty::substs};

type ctxt =
    {mut next_tag_id: u16,
     pad: u16,
     tag_id_to_index: HashMap<nominal_id, u16>,
     tag_order: DVec<enum_data>,
     resources: interner::interner<nominal_id>,
     llshapetablesty: TypeRef,
     llshapetables: ValueRef};

const shape_u8: u8 = 0u8;
const shape_u16: u8 = 1u8;
const shape_u32: u8 = 2u8;
const shape_u64: u8 = 3u8;
const shape_i8: u8 = 4u8;
const shape_i16: u8 = 5u8;
const shape_i32: u8 = 6u8;
const shape_i64: u8 = 7u8;
const shape_f32: u8 = 8u8;
const shape_f64: u8 = 9u8;
const shape_box: u8 = 10u8;
const shape_enum: u8 = 12u8;
const shape_struct: u8 = 17u8;
const shape_box_fn: u8 = 18u8;
const shape_res: u8 = 20u8;
const shape_uniq: u8 = 22u8;
const shape_opaque_closure_ptr: u8 = 23u8; // the closure itself.
const shape_uniq_fn: u8 = 25u8;
const shape_stack_fn: u8 = 26u8;
const shape_bare_fn: u8 = 27u8;
const shape_tydesc: u8 = 28u8;
const shape_send_tydesc: u8 = 29u8;
const shape_rptr: u8 = 31u8;
const shape_fixedvec: u8 = 32u8;
const shape_slice: u8 = 33u8;
const shape_unboxed_vec: u8 = 34u8;

fn mk_global(ccx: @crate_ctxt, name: ~str, llval: ValueRef, internal: bool) ->
   ValueRef {
    let llglobal =
        str::as_c_str(name,
                      |buf| {
                        lib::llvm::llvm::LLVMAddGlobal(ccx.llmod,
                                                       val_ty(llval), buf)
                    });
    lib::llvm::llvm::LLVMSetInitializer(llglobal, llval);
    lib::llvm::llvm::LLVMSetGlobalConstant(llglobal, True);

    if internal {
        lib::llvm::SetLinkage(llglobal, lib::llvm::InternalLinkage);
    }

    return llglobal;
}


// Computes a set of variants of a enum that are guaranteed to have size and
// alignment at least as large as any other variant of the enum. This is an
// important performance optimization.

fn round_up(size: u16, align: u8) -> u16 {
    assert (align >= 1u8);
    let alignment = align as u16;
    return size - 1u16 + alignment & !(alignment - 1u16);
}

type size_align = {size: u16, align: u8};

enum enum_kind {
    tk_unit,    // 1 variant, no data
    tk_enum,    // N variants, no data
    tk_newtype, // 1 variant, data
    tk_complex  // N variants, no data
}

fn enum_kind(ccx: @crate_ctxt, did: ast::def_id) -> enum_kind {
    let variants = ty::enum_variants(ccx.tcx, did);
    if vec::any(*variants, |v| vec::len(v.args) > 0u) {
        if vec::len(*variants) == 1u { tk_newtype }
        else { tk_complex }
    } else {
        if vec::len(*variants) <= 1u { tk_unit }
        else { tk_enum }
    }
}

// Returns the code corresponding to the pointer size on this architecture.
fn s_int(tcx: ty_ctxt) -> u8 {
    return match tcx.sess.targ_cfg.arch {
        session::arch_x86 => shape_i32,
        session::arch_x86_64 => shape_i64,
        session::arch_arm => shape_i32
    };
}

fn s_uint(tcx: ty_ctxt) -> u8 {
    return match tcx.sess.targ_cfg.arch {
        session::arch_x86 => shape_u32,
        session::arch_x86_64 => shape_u64,
        session::arch_arm => shape_u32
    };
}

fn s_float(tcx: ty_ctxt) -> u8 {
    return match tcx.sess.targ_cfg.arch {
        session::arch_x86 => shape_f64,
        session::arch_x86_64 => shape_f64,
        session::arch_arm => shape_f64
    };
}

fn s_variant_enum_t(tcx: ty_ctxt) -> u8 {
    return s_int(tcx);
}

fn s_tydesc(_tcx: ty_ctxt) -> u8 {
    return shape_tydesc;
}

fn s_send_tydesc(_tcx: ty_ctxt) -> u8 {
    return shape_send_tydesc;
}

fn mk_ctxt(llmod: ModuleRef) -> ctxt {
    let llshapetablesty = trans::common::T_named_struct(~"shapes");
    let llshapetables = str::as_c_str(~"shapes", |buf| {
        lib::llvm::llvm::LLVMAddGlobal(llmod, llshapetablesty, buf)
    });

    return {mut next_tag_id: 0u16,
         pad: 0u16,
         tag_id_to_index: new_nominal_id_hash(),
         tag_order: DVec(),
         resources: interner::mk(),
         llshapetablesty: llshapetablesty,
         llshapetables: llshapetables};
}

fn add_bool(&dest: ~[u8], val: bool) {
    dest += ~[if val { 1u8 } else { 0u8 }];
}

fn add_u16(&dest: ~[u8], val: u16) {
    dest += ~[(val & 0xffu16) as u8, (val >> 8u16) as u8];
}

fn add_substr(&dest: ~[u8], src: ~[u8]) {
    add_u16(dest, vec::len(src) as u16);
    dest += src;
}

fn shape_of(ccx: @crate_ctxt, t: ty::t) -> ~[u8] {
    match ty::get(t).sty {
        ty::ty_nil | ty::ty_bool | ty::ty_uint(ast::ty_u8) |
        ty::ty_bot => ~[shape_u8],
        ty::ty_int(ast::ty_i) => ~[s_int(ccx.tcx)],
        ty::ty_float(ast::ty_f) => ~[s_float(ccx.tcx)],
        ty::ty_uint(ast::ty_u) | ty::ty_ptr(_) => ~[s_uint(ccx.tcx)],
        ty::ty_type => ~[s_tydesc(ccx.tcx)],
        ty::ty_int(ast::ty_i8) => ~[shape_i8],
        ty::ty_uint(ast::ty_u16) => ~[shape_u16],
        ty::ty_int(ast::ty_i16) => ~[shape_i16],
        ty::ty_uint(ast::ty_u32) => ~[shape_u32],
        ty::ty_int(ast::ty_i32) | ty::ty_int(ast::ty_char) => ~[shape_i32],
        ty::ty_uint(ast::ty_u64) => ~[shape_u64],
        ty::ty_int(ast::ty_i64) => ~[shape_i64],
        ty::ty_float(ast::ty_f32) => ~[shape_f32],
        ty::ty_float(ast::ty_f64) => ~[shape_f64],
        ty::ty_estr(ty::vstore_uniq) => {
            shape_of(ccx, tvec::expand_boxed_vec_ty(ccx.tcx, t))
        }
        ty::ty_enum(did, substs) => {
            match enum_kind(ccx, did) {
                tk_unit => ~[s_variant_enum_t(ccx.tcx)],
                tk_enum => ~[s_variant_enum_t(ccx.tcx)],
                tk_newtype | tk_complex => {
                    let mut s = ~[shape_enum], id;
                    let nom_id = mk_nominal_id(ccx.tcx, did,
                                               None, substs.tps);
                    match ccx.shape_cx.tag_id_to_index.find(nom_id) {
                        None => {
                            id = ccx.shape_cx.next_tag_id;
                            ccx.shape_cx.tag_id_to_index.insert(nom_id, id);
                            ccx.shape_cx.tag_order.push({did: did,
                                                         substs: substs});
                            ccx.shape_cx.next_tag_id += 1u16;
                        }
                        Some(existing_id) => id = existing_id,
                    }
                    add_u16(s, id as u16);

                    s
                }
            }
        }
        ty::ty_estr(ty::vstore_box) |
        ty::ty_evec(_, ty::vstore_box) |
        ty::ty_box(_) | ty::ty_opaque_box => ~[shape_box],
        ty::ty_uniq(mt) => {
            let mut s = ~[shape_uniq];
            add_substr(s, shape_of(ccx, mt.ty));
            s
        }
        ty::ty_unboxed_vec(mt) => {
            let mut s = ~[shape_unboxed_vec];
            add_bool(s, ty::type_is_pod(ccx.tcx, mt.ty));
            add_substr(s, shape_of(ccx, mt.ty));
            s
        }
        ty::ty_evec(_, ty::vstore_uniq) => {
            shape_of(ccx, tvec::expand_boxed_vec_ty(ccx.tcx, t))
        }

        ty::ty_estr(ty::vstore_fixed(n)) => {
            let mut s = ~[shape_fixedvec];
            let u8_t = ty::mk_mach_uint(ccx.tcx, ast::ty_u8);
            assert (n + 1u) <= 0xffffu;
            add_u16(s, (n + 1u) as u16);
            add_bool(s, true);
            add_substr(s, shape_of(ccx, u8_t));
            s
        }

        ty::ty_evec(mt, ty::vstore_fixed(n)) => {
            let mut s = ~[shape_fixedvec];
            assert n <= 0xffffu;
            add_u16(s, n as u16);
            add_bool(s, ty::type_is_pod(ccx.tcx, mt.ty));
            add_substr(s, shape_of(ccx, mt.ty));
            s
        }

        ty::ty_estr(ty::vstore_slice(_)) => {
            let mut s = ~[shape_slice];
            let u8_t = ty::mk_mach_uint(ccx.tcx, ast::ty_u8);
            add_bool(s, true); // is_pod
            add_bool(s, true); // is_str
            add_substr(s, shape_of(ccx, u8_t));
            s
        }

        ty::ty_evec(mt, ty::vstore_slice(_)) => {
            let mut s = ~[shape_slice];
            add_bool(s, ty::type_is_pod(ccx.tcx, mt.ty));
            add_bool(s, false); // is_str
            add_substr(s, shape_of(ccx, mt.ty));
            s
        }

        ty::ty_rec(fields) => {
            let mut s = ~[shape_struct], sub = ~[];
            for vec::each(fields) |f| {
                sub += shape_of(ccx, f.mt.ty);
            }
            add_substr(s, sub);
            s
        }
        ty::ty_tup(elts) => {
            let mut s = ~[shape_struct], sub = ~[];
            for vec::each(elts) |elt| {
                sub += shape_of(ccx, *elt);
            }
            add_substr(s, sub);
            s
        }
        ty::ty_trait(_, _, _) => ~[shape_box_fn],
        ty::ty_class(did, ref substs) => {
            // same as records, unless there's a dtor
            let tps = substs.tps;
            let m_dtor_did = ty::ty_dtor(ccx.tcx, did);
            let mut s = if option::is_some(m_dtor_did) {
                ~[shape_res]
            }
            else { ~[shape_struct] }, sub = ~[];
            do option::iter(m_dtor_did) |dtor_did| {
                let ri = @{did: dtor_did, parent_id: Some(did), tps: tps};
                let id = ccx.shape_cx.resources.intern(ri);
                add_u16(s, id as u16);
            };
            for ty::class_items_as_mutable_fields(ccx.tcx, did,
                                                  substs).each |f| {
                sub += shape_of(ccx, f.mt.ty);
            }
            add_substr(s, sub);
            s
        }
        ty::ty_rptr(_, mt) => {
            let mut s = ~[shape_rptr];
            add_substr(s, shape_of(ccx, mt.ty));
            s
        }
        ty::ty_param(*) => {
            ccx.tcx.sess.bug(~"non-monomorphized type parameter");
        }
        ty::ty_fn(ref fn_ty) => {
            match fn_ty.meta.proto {
                ty::proto_vstore(ty::vstore_box) => ~[shape_box_fn],
                ty::proto_vstore(ty::vstore_uniq) => ~[shape_uniq_fn],
                ty::proto_vstore(ty::vstore_slice(_)) => ~[shape_stack_fn],
                ty::proto_bare => ~[shape_bare_fn],
                ty::proto_vstore(ty::vstore_fixed(_)) =>
                fail ~"fixed vstore is impossible",
            }
        }
        ty::ty_opaque_closure_ptr(_) => ~[shape_opaque_closure_ptr],
        ty::ty_infer(_) | ty::ty_self => {
            ccx.sess.bug(~"shape_of: unexpected type struct found")
        }
    }
}

fn shape_of_variant(ccx: @crate_ctxt, v: ty::variant_info) -> ~[u8] {
    let mut s = ~[];
    for vec::each(v.args) |t| { s += shape_of(ccx, *t); }
    return s;
}

fn gen_enum_shapes(ccx: @crate_ctxt) -> ValueRef {
    // Loop over all the enum variants and write their shapes into a
    // data buffer. As we do this, it's possible for us to discover
    // new enums, so we must do this first.
    let mut data = ~[];
    let mut offsets = ~[];
    let mut i = 0u;
    let mut enum_variants = ~[];
    while i < ccx.shape_cx.tag_order.len() {
        let {did, substs} = ccx.shape_cx.tag_order[i];
        let variants = @ty::substd_enum_variants(ccx.tcx, did, &substs);
        for vec::each(*variants) |v| {
            offsets += ~[vec::len(data) as u16];

            let variant_shape = shape_of_variant(ccx, *v);
            add_substr(data, variant_shape);

            let zname = str::to_bytes(ccx.sess.str_of(v.name)) + ~[0u8];
            add_substr(data, zname);
        }
        enum_variants += ~[variants];
        i += 1u;
    }

    // Now calculate the sizes of the header space (which contains offsets to
    // info records for each enum) and the info space (which contains offsets
    // to each variant shape). As we do so, build up the header.

    let mut header = ~[];
    let mut inf = ~[];
    let header_sz = 2u16 * ccx.shape_cx.next_tag_id;
    let data_sz = vec::len(data) as u16;

    let mut inf_sz = 0u16;
    for enum_variants.each |variants| {
        let num_variants = vec::len(**variants) as u16;
        add_u16(header, header_sz + inf_sz);
        inf_sz += 2u16 * (num_variants + 2u16) + 3u16;
    }

    // Construct the info tables, which contain offsets to the shape of each
    // variant. Also construct the largest-variant table for each enum, which
    // contains the variants that the size-of operation needs to look at.

    let mut lv_table = ~[];
    let mut i = 0u;
    for enum_variants.each |variants| {
        add_u16(inf, vec::len(**variants) as u16);

        // Construct the largest-variants table.
        add_u16(inf,
                header_sz + inf_sz + data_sz + (vec::len(lv_table) as u16));

        let lv = largest_variants(ccx, *variants);
        add_u16(lv_table, vec::len(lv) as u16);
        for vec::each(lv) |v| { add_u16(lv_table, *v as u16); }

        // Determine whether the enum has dynamic size.
        assert !variants.any(|v| v.args.any(|t| ty::type_has_params(t)));

        // If we can, write in the static size and alignment of the enum.
        // Otherwise, write a placeholder.
        let size_align = compute_static_enum_size(ccx, lv, *variants);

        // Write in the static size and alignment of the enum.
        add_u16(inf, size_align.size);
        inf += ~[size_align.align];

        // Now write in the offset of each variant.
        for variants.each |_v| {
            add_u16(inf, header_sz + inf_sz + offsets[i]);
            i += 1u;
        }
    }

    assert (i == vec::len(offsets));
    assert (header_sz == vec::len(header) as u16);
    assert (inf_sz == vec::len(inf) as u16);
    assert (data_sz == vec::len(data) as u16);

    header += inf;
    header += data;
    header += lv_table;

    return mk_global(ccx, ~"tag_shapes", C_bytes(header), true);

/* tjc: Not annotating FIXMEs in this module because of #1498 */
    fn largest_variants(ccx: @crate_ctxt,
                        variants: @~[ty::variant_info]) -> ~[uint] {
        // Compute the minimum and maximum size and alignment for each
        // variant.
        //
        // NB: We could do better here; e.g. we know that any
        // variant that contains (T,T) must be as least as large as
        // any variant that contains just T.
        let mut ranges = ~[];
        for vec::each(*variants) |variant| {
            let mut bounded = true;
            let mut min_size = 0u, min_align = 0u;
            for vec::each(variant.args) |elem_t| {
                if ty::type_has_params(*elem_t) {
                    // NB: We could do better here; this causes us to
                    // conservatively assume that (int, T) has minimum size 0,
                    // when in fact it has minimum size sizeof(int).
                    bounded = false;
                } else {
                    let llty = type_of::type_of(ccx, *elem_t);
                    min_size += llsize_of_real(ccx, llty);
                    min_align += llalign_of_pref(ccx, llty);
                }
            }

            ranges +=
                ~[{size: {min: min_size, bounded: bounded},
                  align: {min: min_align, bounded: bounded}}];
        }

        // Initialize the candidate set to contain all variants.
        let mut candidates = ~[mut];
        for vec::each(*variants) |_v| { candidates += ~[mut true]; }

        // Do a pairwise comparison among all variants still in the
        // candidate set.  Throw out any variant that we know has size
        // and alignment at least as small as some other variant.
        let mut i = 0u;
        while i < vec::len(ranges) - 1u {
            if candidates[i] {
                let mut j = i + 1u;
                while j < vec::len(ranges) {
                    if candidates[j] {
                        if ranges[i].size.bounded &&
                            ranges[i].align.bounded &&
                            ranges[j].size.bounded &&
                            ranges[j].align.bounded {
                            if ranges[i].size.min >= ranges[j].size.min &&
                                ranges[i].align.min >= ranges[j].align.min {
                                // Throw out j.
                                candidates[j] = false;
                            } else if ranges[j].size.min >=
                                    ranges[i].size.min &&
                                ranges[j].align.min >=
                                    ranges[j].align.min {
                                // Throw out i.
                                candidates[i] = false;
                            }
                        }
                    }
                    j += 1u;
                }
            }
            i += 1u;
        }

        // Return the resulting set.
        let mut result = ~[];
        let mut i = 0u;
        while i < vec::len(candidates) {
            if candidates[i] { vec::push(result, i); }
            i += 1u;
        }
        return result;
    }

    fn compute_static_enum_size(ccx: @crate_ctxt, largest_variants: ~[uint],
                                variants: @~[ty::variant_info])
        -> size_align {
        let mut max_size = 0u16;
        let mut max_align = 1u8;
        for vec::each(largest_variants) |vid| {
            // We increment a "virtual data pointer" to compute the size.
            let mut lltys = ~[];
            for vec::each(variants[*vid].args) |typ| {
                lltys += ~[type_of::type_of(ccx, *typ)];
            }

            let llty = trans::common::T_struct(lltys);
            let dp = llsize_of_real(ccx, llty) as u16;
            let variant_align = llalign_of_pref(ccx, llty) as u8;

            if max_size < dp { max_size = dp; }
            if max_align < variant_align { max_align = variant_align; }
        }

        // Add space for the enum if applicable.
        // FIXME (issue #792): This is wrong. If the enum starts with an
        // 8 byte aligned quantity, we don't align it.
        if vec::len(*variants) > 1u {
            let variant_t = T_enum_discrim(ccx);
            max_size += llsize_of_real(ccx, variant_t) as u16;
            let align = llalign_of_pref(ccx, variant_t) as u8;
            if max_align < align { max_align = align; }
        }

        return {size: max_size, align: max_align};
    }
}

fn gen_resource_shapes(ccx: @crate_ctxt) -> ValueRef {
    let mut dtors = ~[];
    let len = ccx.shape_cx.resources.len();
    for uint::range(0u, len) |i| {
        let ri = ccx.shape_cx.resources.get(i);
        for ri.tps.each() |s| { assert !ty::type_has_params(*s); }
        do option::iter(ri.parent_id) |id| {
            dtors += ~[trans::base::get_res_dtor(ccx, ri.did, id, ri.tps)];
        }
    }
    return mk_global(ccx, ~"resource_shapes", C_struct(dtors), true);
}

// This function serves to break a cyclical dependence between
// emit_tydescs and gen_shape_tables.
//
//  * emit_tydescs calls shape_of, which causes changes to the shape
//    tables
//  * gen_shape_tables transitively calls get_tydesc, which causes new
//    tydescs to be created
//
// We force those tydescs to be emitted now, thus breaking the
// dependency.
fn force_declare_tydescs(ccx: @crate_ctxt) {
    // Walk all known tydescs first to force shape code to declare
    // dependencies.
    for ccx.tydescs.each |key, _val| {
        shape_of(ccx, key);
    }

    // Then walk all resource shapes to force emit all dtors.
    let len = ccx.shape_cx.resources.len();
    for uint::range(0u, len) |i| {
        let ri = ccx.shape_cx.resources.get(i);
        for ri.tps.each() |s| { assert !ty::type_has_params(*s); }
        do option::iter(ri.parent_id) |id| {
            trans::base::get_res_dtor(ccx, ri.did, id, ri.tps);
        }
    }
}

fn gen_shape_tables(ccx: @crate_ctxt) {
    let lltagstable = gen_enum_shapes(ccx);
    let llresourcestable = gen_resource_shapes(ccx);
    trans::common::set_struct_body(ccx.shape_cx.llshapetablesty,
                                   ~[val_ty(lltagstable),
                                    val_ty(llresourcestable)]);

    let lltables =
        C_named_struct(ccx.shape_cx.llshapetablesty,
                       ~[lltagstable, llresourcestable]);
    lib::llvm::llvm::LLVMSetInitializer(ccx.shape_cx.llshapetables, lltables);
    lib::llvm::llvm::LLVMSetGlobalConstant(ccx.shape_cx.llshapetables, True);
    lib::llvm::SetLinkage(ccx.shape_cx.llshapetables,
                          lib::llvm::InternalLinkage);
}

// ______________________________________________________________________
// compute sizeof / alignof

type metrics = {
    bcx: block,
    sz: ValueRef,
    align: ValueRef
};

type tag_metrics = {
    bcx: block,
    sz: ValueRef,
    align: ValueRef,
    payload_align: ValueRef
};

// Returns the number of bytes clobbered by a Store to this type.
fn llsize_of_store(cx: @crate_ctxt, t: TypeRef) -> uint {
    return llvm::LLVMStoreSizeOfType(cx.td.lltd, t) as uint;
}

// Returns the number of bytes between successive elements of type T in an
// array of T. This is the "ABI" size. It includes any ABI-mandated padding.
fn llsize_of_alloc(cx: @crate_ctxt, t: TypeRef) -> uint {
    return llvm::LLVMABISizeOfType(cx.td.lltd, t) as uint;
}

// Returns, as near as we can figure, the "real" size of a type. As in, the
// bits in this number of bytes actually carry data related to the datum
// with the type. Not junk, padding, accidentally-damaged words, or
// whatever. Rounds up to the nearest byte though, so if you have a 1-bit
// value, we return 1 here, not 0. Most of rustc works in bytes.
fn llsize_of_real(cx: @crate_ctxt, t: TypeRef) -> uint {
    let nbits = llvm::LLVMSizeOfTypeInBits(cx.td.lltd, t) as uint;
    if nbits & 7u != 0u {
        // Not an even number of bytes, spills into "next" byte.
        1u + (nbits >> 3)
    } else {
        nbits >> 3
    }
}

// Returns the "default" size of t, which is calculated by casting null to a
// *T and then doing gep(1) on it and measuring the result. Really, look in
// the LLVM sources. It does that. So this is likely similar to the ABI size
// (i.e. including alignment-padding), but goodness knows which alignment it
// winds up using. Probably the ABI one? Not recommended.
fn llsize_of(cx: @crate_ctxt, t: TypeRef) -> ValueRef {
    return llvm::LLVMConstIntCast(lib::llvm::llvm::LLVMSizeOf(t), cx.int_type,
                               False);
}

// Returns the preferred alignment of the given type for the current target.
// The preffered alignment may be larger than the alignment used when
// packing the type into structs. This will be used for things like
// allocations inside a stack frame, which LLVM has a free hand in.
fn llalign_of_pref(cx: @crate_ctxt, t: TypeRef) -> uint {
    return llvm::LLVMPreferredAlignmentOfType(cx.td.lltd, t) as uint;
}

// Returns the minimum alignment of a type required by the plattform.
// This is the alignment that will be used for struct fields, arrays,
// and similar ABI-mandated things.
fn llalign_of_min(cx: @crate_ctxt, t: TypeRef) -> uint {
    return llvm::LLVMABIAlignmentOfType(cx.td.lltd, t) as uint;
}

// Returns the "default" alignment of t, which is calculated by casting
// null to a record containing a single-bit followed by a t value, then
// doing gep(0,1) to get at the trailing (and presumably padded) t cell.
fn llalign_of(cx: @crate_ctxt, t: TypeRef) -> ValueRef {
    return llvm::LLVMConstIntCast(
        lib::llvm::llvm::LLVMAlignOf(t), cx.int_type, False);
}

// Computes the static size of a enum, without using mk_tup(), which is
// bad for performance.
//
// NB: Migrate trans over to use this.

// Computes the size of the data part of an enum.
fn static_size_of_enum(cx: @crate_ctxt, t: ty::t) -> uint {
    if cx.enum_sizes.contains_key(t) { return cx.enum_sizes.get(t); }
    match ty::get(t).sty {
      ty::ty_enum(tid, ref substs) => {
        // Compute max(variant sizes).
        let mut max_size = 0u;
        let variants = ty::enum_variants(cx.tcx, tid);
        for vec::each(*variants) |variant| {
            let tup_ty = simplify_type(cx.tcx,
                                       ty::mk_tup(cx.tcx, variant.args));
            // Perform any type parameter substitutions.
            let tup_ty = ty::subst(cx.tcx, substs, tup_ty);
            // Here we possibly do a recursive call.
            let this_size =
                llsize_of_real(cx, type_of::type_of(cx, tup_ty));
            if max_size < this_size { max_size = this_size; }
        }
        cx.enum_sizes.insert(t, max_size);
        return max_size;
      }
      _ => cx.sess.bug(~"static_size_of_enum called on non-enum")
    }
}

// Creates a simpler, size-equivalent type. The resulting type is guaranteed
// to have (a) the same size as the type that was passed in; (b) to be non-
// recursive. This is done by replacing all boxes in a type with boxed unit
// types.
// This should reduce all pointers to some simple pointer type, to
// ensure that we don't recurse endlessly when computing the size of a
// nominal type that has pointers to itself in it.
fn simplify_type(tcx: ty::ctxt, typ: ty::t) -> ty::t {
    fn nilptr(tcx: ty::ctxt) -> ty::t {
        ty::mk_ptr(tcx, {ty: ty::mk_nil(tcx), mutbl: ast::m_imm})
    }
    fn simplifier(tcx: ty::ctxt, typ: ty::t) -> ty::t {
        match ty::get(typ).sty {
          ty::ty_box(_) | ty::ty_opaque_box | ty::ty_uniq(_) |
          ty::ty_evec(_, ty::vstore_uniq) | ty::ty_evec(_, ty::vstore_box) |
          ty::ty_estr(ty::vstore_uniq) | ty::ty_estr(ty::vstore_box) |
          ty::ty_ptr(_) | ty::ty_rptr(_,_) => nilptr(tcx),
          ty::ty_fn(_) => ty::mk_tup(tcx, ~[nilptr(tcx), nilptr(tcx)]),
          ty::ty_evec(_, ty::vstore_slice(_)) |
          ty::ty_estr(ty::vstore_slice(_)) => {
            ty::mk_tup(tcx, ~[nilptr(tcx), ty::mk_int(tcx)])
          }
          // Reduce a class type to a record type in which all the fields are
          // simplified
          ty::ty_class(did, ref substs) => {
            let simpl_fields = (if is_some(ty::ty_dtor(tcx, did)) {
                // remember the drop flag
                  ~[{ident: syntax::parse::token::special_idents::dtor,
                     mt: {ty: ty::mk_u8(tcx),
                          mutbl: ast::m_mutbl}}] }
                else { ~[] }) +
                do ty::lookup_class_fields(tcx, did).map |f| {
                 let t = ty::lookup_field_type(tcx, did, f.id, substs);
                 {ident: f.ident,
                  mt: {ty: simplify_type(tcx, t), mutbl: ast::m_const}}
            };
            ty::mk_rec(tcx, simpl_fields)
          }
          _ => typ
        }
    }
    ty::fold_ty(tcx, typ, |t| simplifier(tcx, t))
}
