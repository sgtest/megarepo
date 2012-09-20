/*!
 *
 * # Compilation of match statements
 *
 * I will endeavor to explain the code as best I can.  I have only a loose
 * understanding of some parts of it.
 *
 * ## Matching
 *
 * The basic state of the code is maintained in an array `m` of `@Match`
 * objects.  Each `@Match` describes some list of patterns, all of which must
 * match against the current list of values.  If those patterns match, then
 * the arm listed in the match is the correct arm.  A given arm may have
 * multiple corresponding match entries, one for each alternative that
 * remains.  As we proceed these sets of matches are adjusted.  Anyway this
 * part I am pretty vague on.  Perhaps I or someone else can add more
 * documentation when they understand it. :)
 *
 * ## Bindings
 *
 * We store information about the bound variables for each arm as part of the
 * per-arm `ArmData` struct.  There is a mapping from identifiers to
 * `BindingInfo` structs.  These structs contain the mode/id/type of the
 * binding, but they also contain up to two LLVM values, called `llmatch` and
 * `llbinding` respectively (the `llbinding`, as will be described shortly, is
 * optional and only present for by-value bindings---therefore it is bundled
 * up as part of the `TransBindingMode` type).  Both point at allocas.
 *
 * The `llmatch` binding always stores a pointer into the value being matched
 * which points at the data for the binding.  If the value being matched has
 * type `T`, then, `llmatch` will point at an alloca of type `T*` (and hence
 * `llmatch` has type `T**`).  So, if you have a pattern like:
 *
 *    let a: A = ...;
 *    let b: B = ...;
 *    match (a, b) { (ref c, copy d) => { ... } }
 *
 * For `c` and `d`, we would generate allocas of type `C*` and `D*`
 * respectively.  These are called the `llmatch`.  As we match, when we come
 * up against an identifier, we store the current pointer into the
 * corresponding alloca.
 *
 * In addition, for each by-value binding (copy or move), we will create a
 * second alloca (`llbinding`) that will hold the final value.  In this
 * example, that means that `d` would have this second alloca of type `D` (and
 * hence `llbinding` has type `D*`).
 *
 * Once a pattern is completely matched, and assuming that there is no guard
 * pattern, we will branch to a block that leads to the body itself.  For any
 * by-value bindings, this block will first load the ptr from `llmatch` (the
 * one of type `D*`) and copy/move the value into `llbinding` (the one of type
 * `D`).  The second alloca then becomes the value of the local variable.  For
 * by ref bindings, the value of the local variable is simply the first
 * alloca.
 *
 * So, for the example above, we would generate a setup kind of like this:
 *
 *        +-------+
 *        | Entry |
 *        +-------+
 *            |
 *        +-------------------------------------------+
 *        | llmatch_c = (addr of first half of tuple) |
 *        | llmatch_d = (addr of first half of tuple) |
 *        +-------------------------------------------+
 *            |
 *        +--------------------------------------+
 *        | *llbinding_d = **llmatch_dlbinding_d |
 *        +--------------------------------------+
 *
 * If there is a guard, the situation is slightly different, because we must
 * execute the guard code.  Moreover, we need to do so once for each of the
 * alternatives that lead to the arm, because if the guard fails, they may
 * have different points from which to continue the search. Therefore, in that
 * case, we generate code that looks more like:
 *
 *        +-------+
 *        | Entry |
 *        +-------+
 *            |
 *        +-------------------------------------------+
 *        | llmatch_c = (addr of first half of tuple) |
 *        | llmatch_d = (addr of first half of tuple) |
 *        +-------------------------------------------+
 *            |
 *        +-------------------------------------------------+
 *        | *llbinding_d = **llmatch_dlbinding_d            |
 *        | check condition                                 |
 *        | if false { free *llbinding_d, goto next case }  |
 *        | if true { goto body }                           |
 *        +-------------------------------------------------+
 *
 * The handling for the cleanups is a bit... sensitive.  Basically, the body
 * is the one that invokes `add_clean()` for each binding.  During the guard
 * evaluation, we add temporary cleanups and revoke them after the guard is
 * evaluated (it could fail, after all).  Presuming the guard fails, we drop
 * the various values we copied explicitly.  Note that guards and moves are
 * just plain incompatible.
 *
 */

use driver::session::session;
use lib::llvm::llvm;
use lib::llvm::{ValueRef, BasicBlockRef};
use pat_util::*;
use build::*;
use base::*;
use syntax::ast;
use syntax::ast_util;
use syntax::ast_util::{dummy_sp, path_to_ident};
use syntax::ast::def_id;
use syntax::codemap::span;
use syntax::print::pprust::pat_to_str;
use middle::resolve::DefMap;
use back::abi;
use std::map::HashMap;
use dvec::DVec;
use datum::*;
use common::*;
use expr::Dest;
use util::common::indenter;

fn macros() { include!("macros.rs"); } // FIXME(#3114): Macro import/export.

// An option identifying a branch (either a literal, a enum variant or a
// range)
enum Opt {
    lit(@ast::expr),
    var(/* disr val */int, /* variant dids */{enm: def_id, var: def_id}),
    range(@ast::expr, @ast::expr)
}
fn opt_eq(tcx: ty::ctxt, a: &Opt, b: &Opt) -> bool {
    match (*a, *b) {
      (lit(a), lit(b)) => const_eval::compare_lit_exprs(tcx, a, b) == 0,
      (range(a1, a2), range(b1, b2)) => {
        const_eval::compare_lit_exprs(tcx, a1, b1) == 0 &&
        const_eval::compare_lit_exprs(tcx, a2, b2) == 0
      }
      (var(a, _), var(b, _)) => a == b,
      _ => false
    }
}

enum opt_result {
    single_result(Result),
    range_result(Result, Result),
}
fn trans_opt(bcx: block, o: &Opt) -> opt_result {
    let _icx = bcx.insn_ctxt("alt::trans_opt");
    let ccx = bcx.ccx();
    let mut bcx = bcx;
    match *o {
        lit(lit_expr) => {
            let datumblock = expr::trans_to_datum(bcx, lit_expr);
            return single_result(datumblock.to_result());
        }
        var(disr_val, _) => {
            return single_result(rslt(bcx, C_int(ccx, disr_val)));
        }
        range(l1, l2) => {
            return range_result(rslt(bcx, consts::const_expr(ccx, l1)),
                                rslt(bcx, consts::const_expr(ccx, l2)));
        }
    }
}

fn variant_opt(tcx: ty::ctxt, pat_id: ast::node_id) -> Opt {
    let vdef = ast_util::variant_def_ids(tcx.def_map.get(pat_id));
    let variants = ty::enum_variants(tcx, vdef.enm);
    for vec::each(*variants) |v| {
        if vdef.var == v.id { return var(v.disr_val, vdef); }
    }
    core::unreachable();
}

enum TransBindingMode {
    TrByValue(/*ismove:*/ bool, /*llbinding:*/ ValueRef),
    TrByRef,
    TrByImplicitRef
}

/**
 * Information about a pattern binding:
 * - `llmatch` is a pointer to a stack slot.  The stack slot contains a
 *   pointer into the value being matched.  Hence, llmatch has type `T**`
 *   where `T` is the value being matched.
 * - `trmode` is the trans binding mode
 * - `id` is the node id of the binding
 * - `ty` is the Rust type of the binding */
struct BindingInfo {
    llmatch: ValueRef,
    trmode: TransBindingMode,
    id: ast::node_id,
    ty: ty::t,
}

type BindingsMap = HashMap<ident, BindingInfo>;

struct ArmData {
    bodycx: block,
    arm: &ast::arm,
    bindings_map: BindingsMap
}

struct Match {
    pats: ~[@ast::pat],
    data: @ArmData
}

fn match_to_str(bcx: block, m: &Match) -> ~str {
    if bcx.sess().verbose() {
        // for many programs, this just take too long to serialize
        fmt!("%?", m.pats.map(|p| pat_to_str(p, bcx.sess().intr())))
    } else {
        fmt!("%u pats", m.pats.len())
    }
}

fn matches_to_str(bcx: block, m: &[@Match]) -> ~str {
    fmt!("%?", m.map(|n| match_to_str(bcx, n)))
}

fn has_nested_bindings(m: &[@Match], col: uint) -> bool {
    for vec::each(m) |br| {
        match br.pats[col].node {
          ast::pat_ident(_, _, Some(_)) => return true,
          _ => ()
        }
    }
    return false;
}

fn expand_nested_bindings(bcx: block, m: &[@Match/&r],
                          col: uint, val: ValueRef)
    -> ~[@Match/&r]
{
    debug!("expand_nested_bindings(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    do m.map |br| {
        match br.pats[col].node {
            ast::pat_ident(_, path, Some(inner)) => {
                let pats = vec::append(
                    vec::slice(br.pats, 0u, col),
                    vec::append(~[inner],
                                vec::view(br.pats, col + 1u, br.pats.len())));

                let binding_info =
                    br.data.bindings_map.get(path_to_ident(path));

                Store(bcx, val, binding_info.llmatch);
                @Match {pats: pats, data: br.data}
            }
            _ => {
                br
            }
        }
    }
}

type enter_pat = fn(@ast::pat) -> Option<~[@ast::pat]>;

fn assert_is_binding_or_wild(bcx: block, p: @ast::pat) {
    if !pat_is_binding_or_wild(bcx.tcx().def_map, p) {
        bcx.sess().span_bug(
            p.span,
            fmt!("Expected an identifier pattern but found p: %s",
                 pat_to_str(p, bcx.sess().intr())));
    }
}

fn enter_match(bcx: block, dm: DefMap, m: &[@Match/&r],
               col: uint, val: ValueRef, e: enter_pat)
    -> ~[@Match/&r]
{
    debug!("enter_match(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    let mut result = ~[];
    for vec::each(m) |br| {
        match e(br.pats[col]) {
            Some(sub) => {
                let pats =
                    vec::append(
                        vec::append(sub, vec::view(br.pats, 0u, col)),
                        vec::view(br.pats, col + 1u, br.pats.len()));

                let self = br.pats[col];
                match self.node {
                    ast::pat_ident(_, path, None) => {
                        if !pat_is_variant(dm, self) {
                            let binding_info =
                                br.data.bindings_map.get(path_to_ident(path));
                            Store(bcx, val, binding_info.llmatch);
                        }
                    }
                    _ => {}
                }

                vec::push(result, @Match {pats: pats, data: br.data});
            }
            None => ()
        }
    }

    debug!("result=%s", matches_to_str(bcx, result));

    return result;
}

fn enter_default(bcx: block, dm: DefMap, m: &[@Match/&r],
                 col: uint, val: ValueRef)
    -> ~[@Match/&r]
{
    debug!("enter_default(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    do enter_match(bcx, dm, m, col, val) |p| {
        match p.node {
          ast::pat_wild | ast::pat_rec(_, _) | ast::pat_tup(_) |
          ast::pat_struct(*) => Some(~[]),
          ast::pat_ident(_, _, None) if !pat_is_variant(dm, p) => Some(~[]),
          _ => None
        }
    }
}

fn enter_opt(bcx: block, m: &[@Match/&r], opt: &Opt, col: uint,
             variant_size: uint, val: ValueRef)
    -> ~[@Match/&r]
{
    debug!("enter_opt(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    let tcx = bcx.tcx();
    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    do enter_match(bcx, tcx.def_map, m, col, val) |p| {
        match p.node {
            ast::pat_enum(_, subpats) => {
                if opt_eq(tcx, &variant_opt(tcx, p.id), opt) {
                    Some(option::get_default(subpats,
                                             vec::from_elem(variant_size,
                                                            dummy)))
                } else {
                    None
                }
            }
            ast::pat_ident(_, _, None) if pat_is_variant(tcx.def_map, p) => {
                if opt_eq(tcx, &variant_opt(tcx, p.id), opt) {
                    Some(~[])
                } else {
                    None
                }
            }
            ast::pat_lit(l) => {
                if opt_eq(tcx, &lit(l), opt) {Some(~[])} else {None}
            }
            ast::pat_range(l1, l2) => {
                if opt_eq(tcx, &range(l1, l2), opt) {Some(~[])} else {None}
            }
            _ => {
                assert_is_binding_or_wild(bcx, p);
                Some(vec::from_elem(variant_size, dummy))
            }
        }
    }
}

fn enter_rec_or_struct(bcx: block, dm: DefMap, m: &[@Match/&r], col: uint,
                       fields: ~[ast::ident], val: ValueRef) -> ~[@Match/&r] {
    debug!("enter_rec_or_struct(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    do enter_match(bcx, dm, m, col, val) |p| {
        match p.node {
            ast::pat_rec(fpats, _) | ast::pat_struct(_, fpats, _) => {
                let mut pats = ~[];
                for vec::each(fields) |fname| {
                    match fpats.find(|p| p.ident == *fname) {
                        None => vec::push(pats, dummy),
                        Some(pat) => vec::push(pats, pat.pat)
                    }
                }
                Some(pats)
            }
            _ => {
                assert_is_binding_or_wild(bcx, p);
                Some(vec::from_elem(fields.len(), dummy))
            }
        }
    }
}

fn enter_tup(bcx: block, dm: DefMap, m: &[@Match/&r],
             col: uint, val: ValueRef, n_elts: uint)
    -> ~[@Match/&r]
{
    debug!("enter_tup(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    do enter_match(bcx, dm, m, col, val) |p| {
        match p.node {
            ast::pat_tup(elts) => {
                Some(elts)
            }
            _ => {
                assert_is_binding_or_wild(bcx, p);
                Some(vec::from_elem(n_elts, dummy))
            }
        }
    }
}

fn enter_box(bcx: block, dm: DefMap, m: &[@Match/&r],
             col: uint, val: ValueRef)
    -> ~[@Match/&r]
{
    debug!("enter_box(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    do enter_match(bcx, dm, m, col, val) |p| {
        match p.node {
            ast::pat_box(sub) => {
                Some(~[sub])
            }
            _ => {
                assert_is_binding_or_wild(bcx, p);
                Some(~[dummy])
            }
        }
    }
}

fn enter_uniq(bcx: block, dm: DefMap, m: &[@Match/&r],
              col: uint, val: ValueRef)
    -> ~[@Match/&r]
{
    debug!("enter_uniq(bcx=%s, m=%s, col=%u, val=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           col,
           bcx.val_str(val));
    let _indenter = indenter();

    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    do enter_match(bcx, dm, m, col, val) |p| {
        match p.node {
            ast::pat_uniq(sub) => {
                Some(~[sub])
            }
            _ => {
                assert_is_binding_or_wild(bcx, p);
                Some(~[dummy])
            }
        }
    }
}

fn get_options(ccx: @crate_ctxt, m: &[@Match], col: uint) -> ~[Opt] {
    fn add_to_set(tcx: ty::ctxt, set: &DVec<Opt>, val: Opt) {
        if set.any(|l| opt_eq(tcx, &l, &val)) {return;}
        set.push(val);
    }

    let found = DVec();
    for vec::each(m) |br| {
        let cur = br.pats[col];
        if pat_is_variant(ccx.tcx.def_map, cur) {
            add_to_set(ccx.tcx, &found, variant_opt(ccx.tcx, cur.id));
        } else {
            match cur.node {
                ast::pat_lit(l) => {
                    add_to_set(ccx.tcx, &found, lit(l));
                }
                ast::pat_range(l1, l2) => {
                    add_to_set(ccx.tcx, &found, range(l1, l2));
                }
                _ => ()
            }
        }
    }
    return dvec::unwrap(move found);
}

fn extract_variant_args(bcx: block, pat_id: ast::node_id,
                        vdefs: {enm: def_id, var: def_id},
                        val: ValueRef)
    -> {vals: ~[ValueRef], bcx: block}
{
    let _icx = bcx.insn_ctxt("alt::extract_variant_args");
    let ccx = bcx.fcx.ccx;
    let enum_ty_substs = match ty::get(node_id_type(bcx, pat_id)).sty {
      ty::ty_enum(id, substs) => { assert id == vdefs.enm; substs.tps }
      _ => bcx.sess().bug(~"extract_variant_args: pattern has non-enum type")
    };
    let mut blobptr = val;
    let variants = ty::enum_variants(ccx.tcx, vdefs.enm);
    let size = ty::enum_variant_with_id(ccx.tcx, vdefs.enm,
                                        vdefs.var).args.len();
    if size > 0u && (*variants).len() != 1u {
        let enumptr =
            PointerCast(bcx, val, T_opaque_enum_ptr(ccx));
        blobptr = GEPi(bcx, enumptr, [0u, 1u]);
    }
    let vdefs_tg = vdefs.enm;
    let vdefs_var = vdefs.var;
    let args = do vec::from_fn(size) |i| {
        GEP_enum(bcx, blobptr, vdefs_tg, vdefs_var,
                 enum_ty_substs, i)
    };
    return {vals: args, bcx: bcx};
}

fn collect_record_or_struct_fields(m: &[@Match], col: uint) -> ~[ast::ident] {
    let mut fields: ~[ast::ident] = ~[];
    for vec::each(m) |br| {
        match br.pats[col].node {
          ast::pat_rec(fs, _) => extend(&mut fields, fs),
          ast::pat_struct(_, fs, _) => extend(&mut fields, fs),
          _ => ()
        }
    }
    return fields;

    fn extend(idents: &mut ~[ast::ident], field_pats: &[ast::field_pat]) {
        for field_pats.each |field_pat| {
            let field_ident = field_pat.ident;
            if !vec::any(*idents, |x| x == field_ident) {
                vec::push(*idents, field_ident);
            }
        }
    }
}

fn root_pats_as_necessary(bcx: block, m: &[@Match],
                          col: uint, val: ValueRef)
{
    for vec::each(m) |br| {
        let pat_id = br.pats[col].id;

        match bcx.ccx().maps.root_map.find({id:pat_id, derefs:0u}) {
            None => (),
            Some(scope_id) => {
                // Note: the scope_id will always be the id of the alt.  See
                // the extended comment in rustc::middle::borrowck::preserve()
                // for details (look for the case covering cat_discr).

                let datum = Datum {val: val, ty: node_id_type(bcx, pat_id),
                                   mode: ByRef, source: FromLvalue};
                datum.root(bcx, scope_id);
                return; // if we kept going, we'd only re-root the same value
            }
        }
    }
}

fn any_box_pat(m: &[@Match], col: uint) -> bool {
    for vec::each(m) |br| {
        match br.pats[col].node {
          ast::pat_box(_) => return true,
          _ => ()
        }
    }
    return false;
}

fn any_uniq_pat(m: &[@Match], col: uint) -> bool {
    for vec::each(m) |br| {
        match br.pats[col].node {
          ast::pat_uniq(_) => return true,
          _ => ()
        }
    }
    return false;
}

fn any_tup_pat(m: &[@Match], col: uint) -> bool {
    for vec::each(m) |br| {
        match br.pats[col].node {
          ast::pat_tup(_) => return true,
          _ => ()
        }
    }
    return false;
}

type mk_fail = fn@() -> BasicBlockRef;

fn pick_col(m: &[@Match]) -> uint {
    fn score(p: @ast::pat) -> uint {
        match p.node {
          ast::pat_lit(_) | ast::pat_enum(_, _) | ast::pat_range(_, _) => 1u,
          ast::pat_ident(_, _, Some(p)) => score(p),
          _ => 0u
        }
    }
    let scores = vec::to_mut(vec::from_elem(m[0].pats.len(), 0u));
    for vec::each(m) |br| {
        let mut i = 0u;
        for vec::each(br.pats) |p| { scores[i] += score(*p); i += 1u; }
    }
    let mut max_score = 0u;
    let mut best_col = 0u;
    let mut i = 0u;
    for vec::each(scores) |score| {
        let score = *score;

        // Irrefutable columns always go first, they'd only be duplicated in
        // the branches.
        if score == 0u { return i; }
        // If no irrefutable ones are found, we pick the one with the biggest
        // branching factor.
        if score > max_score { max_score = score; best_col = i; }
        i += 1u;
    }
    return best_col;
}

enum branch_kind { no_branch, single, switch, compare, }

#[cfg(stage0)]
impl branch_kind : cmp::Eq {
    pure fn eq(&&other: branch_kind) -> bool {
        (self as uint) == (other as uint)
    }
    pure fn ne(&&other: branch_kind) -> bool { !self.eq(other) }
}
#[cfg(stage1)]
#[cfg(stage2)]
impl branch_kind : cmp::Eq {
    pure fn eq(other: &branch_kind) -> bool {
        (self as uint) == ((*other) as uint)
    }
    pure fn ne(other: &branch_kind) -> bool { !self.eq(other) }
}

// Compiles a comparison between two things.
fn compare_values(cx: block, lhs: ValueRef, rhs: ValueRef, rhs_t: ty::t) ->
                  Result {
    let _icx = cx.insn_ctxt("compare_values");
    if ty::type_is_scalar(rhs_t) {
      let rs = compare_scalar_types(cx, lhs, rhs, rhs_t, ast::eq);
      return rslt(rs.bcx, rs.val);
    }

    match ty::get(rhs_t).sty {
        ty::ty_estr(ty::vstore_uniq) => {
            let scratch_result = scratch_datum(cx, ty::mk_bool(cx.tcx()),
                                               false);
            let scratch_lhs = alloca(cx, val_ty(lhs));
            Store(cx, lhs, scratch_lhs);
            let scratch_rhs = alloca(cx, val_ty(rhs));
            Store(cx, rhs, scratch_rhs);
            let did = cx.tcx().lang_items.uniq_str_eq_fn.get();
            let bcx = callee::trans_rtcall_or_lang_call(cx, did,
                                                        ~[scratch_lhs,
                                                          scratch_rhs],
                                                        expr::SaveIn(
                                                         scratch_result.val));
            return scratch_result.to_result(bcx);
        }
        _ => {
            cx.tcx().sess.bug(~"only scalars and unique strings supported in \
                                compare_values");
        }
    }
}

fn store_non_ref_bindings(bcx: block,
                          data: &ArmData,
                          opt_temp_cleanups: Option<&DVec<ValueRef>>)
    -> block
{
    /*!
     *
     * For each copy/move binding, copy the value from the value
     * being matched into its final home.  This code executes once
     * one of the patterns for a given arm has completely matched.
     * It adds temporary cleanups to the `temp_cleanups` array,
     * if one is provided.
     */

    let mut bcx = bcx;
    for data.bindings_map.each_value |binding_info| {
        match binding_info.trmode {
            TrByValue(is_move, lldest) => {
                let llval = Load(bcx, binding_info.llmatch); // get a T*
                let datum = Datum {val: llval, ty: binding_info.ty,
                                   mode: ByRef, source: FromLvalue};
                bcx = {
                    if is_move {
                        datum.move_to(bcx, INIT, lldest)
                    } else {
                        datum.copy_to(bcx, INIT, lldest)
                    }
                };

                for opt_temp_cleanups.each |temp_cleanups| {
                    add_clean_temp_mem(bcx, lldest, binding_info.ty);
                    temp_cleanups.push(lldest);
                }
            }
            TrByRef | TrByImplicitRef => {}
        }
    }
    return bcx;
}

fn insert_lllocals(bcx: block,
                   data: &ArmData,
                   add_cleans: bool) -> block {
    /*!
     *
     * For each binding in `data.bindings_map`, adds an appropriate entry into
     * the `fcx.lllocals` map.  If add_cleans is true, then adds cleanups for
     * the bindings. */

    for data.bindings_map.each_value |binding_info| {
        let llval = match binding_info.trmode {
            // By value bindings: use the stack slot that we
            // copied/moved the value into
            TrByValue(_, lldest) => {
                if add_cleans {
                    add_clean(bcx, lldest, binding_info.ty);
                }

                lldest
            }

            // By ref binding: use the ptr into the matched value
            TrByRef => {
                binding_info.llmatch
            }

            // Ugly: for implicit ref, we actually want a T*, but
            // we have a T**, so we had to load.  This will go away
            // once implicit refs go away.
            TrByImplicitRef => {
                Load(bcx, binding_info.llmatch)
            }
        };

        bcx.fcx.lllocals.insert(binding_info.id,
                                local_mem(llval));
    }
    return bcx;
}

fn compile_guard(bcx: block,
                 guard_expr: @ast::expr,
                 data: &ArmData,
                 m: &[@Match],
                 vals: &[ValueRef],
                 chk: Option<mk_fail>)
    -> block
{
    debug!("compile_guard(bcx=%s, guard_expr=%s, m=%s, vals=%?)",
           bcx.to_str(),
           bcx.expr_to_str(guard_expr),
           matches_to_str(bcx, m),
           vals.map(|v| bcx.val_str(v)));
    let _indenter = indenter();

    let mut bcx = bcx;
    let temp_cleanups = DVec();
    bcx = store_non_ref_bindings(bcx, data, Some(&temp_cleanups));
    bcx = insert_lllocals(bcx, data, false);

    let val = unpack_result!(bcx, {
        do with_scope_result(bcx, guard_expr.info(),
                             ~"guard") |bcx| {
            expr::trans_to_datum(bcx, guard_expr).to_result()
        }
    });

    // Revoke the temp cleanups now that the guard successfully executed.
    for temp_cleanups.each |llval| {
        revoke_clean(bcx, *llval);
    }

    return do with_cond(bcx, Not(bcx, val)) |bcx| {
        // Guard does not match: free the values we copied,
        // and remove all bindings from the lllocals table
        let bcx = drop_bindings(bcx, data);
        compile_submatch(bcx, m, vals, chk);
        bcx
    };

    fn drop_bindings(bcx: block, data: &ArmData) -> block {
        let mut bcx = bcx;
        for data.bindings_map.each_value |binding_info| {
            match binding_info.trmode {
                TrByValue(_, llval) => {
                    bcx = glue::drop_ty(bcx, llval, binding_info.ty);
                }
                TrByRef | TrByImplicitRef => {}
            }
            bcx.fcx.lllocals.remove(binding_info.id);
        }
        return bcx;
    }
}

fn compile_submatch(bcx: block,
                    m: &[@Match],
                    vals: &[ValueRef],
                    chk: Option<mk_fail>)
{
    debug!("compile_submatch(bcx=%s, m=%s, vals=%?)",
           bcx.to_str(),
           matches_to_str(bcx, m),
           vals.map(|v| bcx.val_str(v)));
    let _indenter = indenter();

    /*
      For an empty match, a fall-through case must exist
     */
    assert(m.len() > 0u || is_some(chk));
    let _icx = bcx.insn_ctxt("alt::compile_submatch");
    let mut bcx = bcx;
    let tcx = bcx.tcx(), dm = tcx.def_map;
    if m.len() == 0u {
        Br(bcx, option::get(chk)());
        return;
    }
    if m[0].pats.len() == 0u {
        let data = m[0].data;
        match data.arm.guard {
            Some(guard_expr) => {
                bcx = compile_guard(bcx, guard_expr, m[0].data,
                                    vec::view(m, 1, m.len()),
                                    vals, chk);
            }
            _ => ()
        }
        Br(bcx, data.bodycx.llbb);
        return;
    }

    let col = pick_col(m);
    let val = vals[col];
    let m = {
        if has_nested_bindings(m, col) {
            expand_nested_bindings(bcx, m, col, val)
        } else {
            m.to_vec()
        }
    };

    let vals_left = vec::append(vec::slice(vals, 0u, col),
                                vec::view(vals, col + 1u, vals.len()));
    let ccx = bcx.fcx.ccx;
    let mut pat_id = 0;
    for vec::each(m) |br| {
        // Find a real id (we're adding placeholder wildcard patterns, but
        // each column is guaranteed to have at least one real pattern)
        if pat_id == 0 { pat_id = br.pats[col].id; }
    }

    root_pats_as_necessary(bcx, m, col, val);

    let rec_fields = collect_record_or_struct_fields(m, col);
    if rec_fields.len() > 0 {
        let pat_ty = node_id_type(bcx, pat_id);
        do expr::with_field_tys(tcx, pat_ty) |_has_dtor, field_tys| {
            let rec_vals = rec_fields.map(|field_name| {
                let ix = ty::field_idx_strict(tcx, field_name, field_tys);
                GEPi(bcx, val, struct_field(ix))
            });
            compile_submatch(
                bcx,
                enter_rec_or_struct(bcx, dm, m, col, rec_fields, val),
                vec::append(rec_vals, vals_left),
                chk);
        }
        return;
    }

    if any_tup_pat(m, col) {
        let tup_ty = node_id_type(bcx, pat_id);
        let n_tup_elts = match ty::get(tup_ty).sty {
          ty::ty_tup(elts) => elts.len(),
          _ => ccx.sess.bug(~"non-tuple type in tuple pattern")
        };
        let tup_vals = vec::from_fn(n_tup_elts, |i| GEPi(bcx, val, [0u, i]));
        compile_submatch(bcx, enter_tup(bcx, dm, m, col, val, n_tup_elts),
                         vec::append(tup_vals, vals_left), chk);
        return;
    }

    // Unbox in case of a box field
    if any_box_pat(m, col) {
        let llbox = Load(bcx, val);
        let box_no_addrspace = non_gc_box_cast(bcx, llbox);
        let unboxed =
            GEPi(bcx, box_no_addrspace, [0u, abi::box_field_body]);
        compile_submatch(bcx, enter_box(bcx, dm, m, col, val),
                         vec::append(~[unboxed], vals_left), chk);
        return;
    }

    if any_uniq_pat(m, col) {
        let llbox = Load(bcx, val);
        let box_no_addrspace = non_gc_box_cast(bcx, llbox);
        let unboxed =
            GEPi(bcx, box_no_addrspace, [0u, abi::box_field_body]);
        compile_submatch(bcx, enter_uniq(bcx, dm, m, col, val),
                         vec::append(~[unboxed], vals_left), chk);
        return;
    }

    // Decide what kind of branch we need
    let opts = get_options(ccx, m, col);
    let mut kind = no_branch;
    let mut test_val = val;
    if opts.len() > 0u {
        match opts[0] {
            var(_, vdef) => {
                if (*ty::enum_variants(tcx, vdef.enm)).len() == 1u {
                    kind = single;
                } else {
                    let enumptr =
                        PointerCast(bcx, val, T_opaque_enum_ptr(ccx));
                    let discrimptr = GEPi(bcx, enumptr, [0u, 0u]);
                    test_val = Load(bcx, discrimptr);
                    kind = switch;
                }
            }
            lit(_) => {
                let pty = node_id_type(bcx, pat_id);
                test_val = load_if_immediate(bcx, val, pty);
                kind = if ty::type_is_integral(pty) { switch }
                else { compare };
            }
            range(_, _) => {
                test_val = Load(bcx, val);
                kind = compare;
            }
        }
    }
    for vec::each(opts) |o| {
        match *o {
            range(_, _) => { kind = compare; break }
            _ => ()
        }
    }
    let else_cx = match kind {
        no_branch | single => bcx,
        _ => sub_block(bcx, ~"match_else")
    };
    let sw = if kind == switch {
        Switch(bcx, test_val, else_cx.llbb, opts.len())
    } else {
        C_int(ccx, 0) // Placeholder for when not using a switch
    };

    let defaults = enter_default(else_cx, dm, m, col, val);
    let exhaustive = option::is_none(chk) && defaults.len() == 0u;
    let len = opts.len();
    let mut i = 0u;

    // Compile subtrees for each option
    for vec::each(opts) |opt| {
        i += 1u;
        let mut opt_cx = else_cx;
        if !exhaustive || i < len {
            opt_cx = sub_block(bcx, ~"match_case");
            match kind {
              single => Br(bcx, opt_cx.llbb),
              switch => {
                  match trans_opt(bcx, opt) {
                      single_result(r) => {
                          llvm::LLVMAddCase(sw, r.val, opt_cx.llbb);
                          bcx = r.bcx;
                      }
                      _ => {
                          bcx.sess().bug(
                              ~"in compile_submatch, expected \
                                trans_opt to return a single_result")
                      }
                  }
              }
              compare => {
                  let t = node_id_type(bcx, pat_id);
                  let Result {bcx: after_cx, val: matches} = {
                      do with_scope_result(bcx, None,
                                           ~"compare_scope") |bcx| {
                          match trans_opt(bcx, opt) {
                              single_result(
                                  Result {bcx, val}) => {
                                  compare_values(bcx, test_val, val, t)
                              }
                              range_result(
                                  Result {val: vbegin, _},
                                  Result {bcx, val: vend}) => {
                                  let Result {bcx, val: llge} =
                                      compare_scalar_types(
                                          bcx, test_val,
                                          vbegin, t, ast::ge);
                                  let Result {bcx, val: llle} =
                                      compare_scalar_types(
                                          bcx, test_val, vend,
                                          t, ast::le);
                                  rslt(bcx, And(bcx, llge, llle))
                              }
                          }
                      }
                  };
                  bcx = sub_block(after_cx, ~"compare_next");
                  CondBr(after_cx, matches, opt_cx.llbb, bcx.llbb);
              }
                _ => ()
            }
        } else if kind == compare {
            Br(bcx, else_cx.llbb);
        }

        let mut size = 0u;
        let mut unpacked = ~[];
        match *opt {
            var(_, vdef) => {
                let args = extract_variant_args(opt_cx, pat_id, vdef, val);
                size = args.vals.len();
                unpacked = args.vals;
                opt_cx = args.bcx;
            }
            lit(_) | range(_, _) => ()
        }
        let opt_ms = enter_opt(opt_cx, m, opt, col, size, val);
        let opt_vals = vec::append(unpacked, vals_left);
        compile_submatch(opt_cx, opt_ms, opt_vals, chk);
    }

    // Compile the fall-through case, if any
    if !exhaustive {
        if kind == compare { Br(bcx, else_cx.llbb); }
        if kind != single {
            compile_submatch(else_cx, defaults, vals_left, chk);
        }
    }
}

fn trans_alt(bcx: block,
             alt_expr: @ast::expr,
             discr_expr: @ast::expr,
             arms: ~[ast::arm],
             dest: Dest) -> block {
    let _icx = bcx.insn_ctxt("alt::trans_alt");
    do with_scope(bcx, alt_expr.info(), ~"alt") |bcx| {
        trans_alt_inner(bcx, discr_expr, arms, dest)
    }
}

fn trans_alt_inner(scope_cx: block,
                   discr_expr: @ast::expr,
                   arms: &[ast::arm],
                   dest: Dest) -> block {
    let _icx = scope_cx.insn_ctxt("alt::trans_alt_inner");
    let mut bcx = scope_cx;
    let tcx = bcx.tcx();

    let discr_datum = unpack_datum!(bcx, {
        expr::trans_to_datum(bcx, discr_expr)
    });
    if bcx.unreachable {
        return bcx;
    }

    let mut arm_datas = ~[], matches = ~[];
    for vec::each(arms) |arm| {
        let body = scope_block(bcx, arm.body.info(), ~"case_body");

        // Create the bindings map, which is a mapping from each binding name
        // to an alloca() that will be the value for that local variable.
        // Note that we use the names because each binding will have many ids
        // from the various alternatives.
        let bindings_map = std::map::HashMap();
        do pat_bindings(tcx.def_map, arm.pats[0]) |bm, p_id, _s, path| {
            let ident = path_to_ident(path);
            let variable_ty = node_id_type(bcx, p_id);
            let llvariable_ty = type_of::type_of(bcx.ccx(), variable_ty);

            let llmatch, trmode;
            match bm {
                ast::bind_by_value | ast::bind_by_move => {
                    // in this case, the type of the variable will be T,
                    // but we need to store a *T
                    let is_move = (bm == ast::bind_by_move);
                    llmatch = alloca(bcx, T_ptr(llvariable_ty));
                    trmode = TrByValue(is_move, alloca(bcx, llvariable_ty));
                }
                ast::bind_by_implicit_ref => {
                    llmatch = alloca(bcx, T_ptr(llvariable_ty));
                    trmode = TrByImplicitRef;
                }
                ast::bind_by_ref(_) => {
                    llmatch = alloca(bcx, llvariable_ty);
                    trmode = TrByRef;
                }
            };
            bindings_map.insert(ident, BindingInfo {
                llmatch: llmatch, trmode: trmode,
                id: p_id, ty: variable_ty
            });
        }

        let arm_data = @ArmData {bodycx: body,
                                 arm: arm,
                                 bindings_map: bindings_map};
        vec::push(arm_datas, arm_data);
        for vec::each(arm.pats) |p| {
            vec::push(matches, @Match {pats: ~[*p], data: arm_data});
        }
    }

    let t = node_id_type(bcx, discr_expr.id);
    let chk = {
        if ty::type_is_empty(tcx, t) {
            // Special case for empty types
            let fail_cx = @mut None;
            Some(|| mk_fail(scope_cx, discr_expr.span,
                            ~"scrutinizing value that can't exist", fail_cx))
        } else {
            None
        }
    };
    let lldiscr = discr_datum.to_ref_llval(bcx);
    compile_submatch(bcx, matches, ~[lldiscr], chk);

    let arm_cxs = DVec();
    for arm_datas.each |arm_data| {
        let mut bcx = arm_data.bodycx;

        // If this arm has a guard, then the various by-value bindings have
        // already been copied into their homes.  If not, we do it here.  This
        // is just to reduce code space.  See extensive comment at the start
        // of the file for more details.
        if arm_data.arm.guard.is_none() {
            bcx = store_non_ref_bindings(bcx, *arm_data, None);
        }

        // insert bindings into the lllocals map and add cleanups
        bcx = insert_lllocals(bcx, *arm_data, true);

        bcx = controlflow::trans_block(bcx, arm_data.arm.body, dest);
        bcx = trans_block_cleanups(bcx, block_cleanups(arm_data.bodycx));
        arm_cxs.push(bcx);
    }

    return controlflow::join_blocks(scope_cx, dvec::unwrap(arm_cxs));

    fn mk_fail(bcx: block, sp: span, msg: ~str,
               done: @mut Option<BasicBlockRef>) -> BasicBlockRef {
        match *done { Some(bb) => return bb, _ => () }
        let fail_cx = sub_block(bcx, ~"case_fallthrough");
        controlflow::trans_fail(fail_cx, Some(sp), msg);
        *done = Some(fail_cx.llbb);
        return fail_cx.llbb;
    }
}

// Not alt-related, but similar to the pattern-munging code above
fn bind_irrefutable_pat(bcx: block, pat: @ast::pat, val: ValueRef,
                        make_copy: bool) -> block {
    let _icx = bcx.insn_ctxt("alt::bind_irrefutable_pat");
    let ccx = bcx.fcx.ccx;
    let mut bcx = bcx;

    // Necessary since bind_irrefutable_pat is called outside trans_alt
    match pat.node {
        ast::pat_ident(_, _,inner) => {
            if pat_is_variant(bcx.tcx().def_map, pat) {
                return bcx;
            }

            if make_copy {
                let binding_ty = node_id_type(bcx, pat.id);
                let datum = Datum {val: val, ty: binding_ty,
                                   mode: ByRef, source: FromRvalue};
                let scratch = scratch_datum(bcx, binding_ty, false);
                datum.copy_to_datum(bcx, INIT, scratch);
                bcx.fcx.lllocals.insert(pat.id, local_mem(scratch.val));
                add_clean(bcx, scratch.val, binding_ty);
            } else {
                bcx.fcx.lllocals.insert(pat.id, local_mem(val));
            }

            for inner.each |inner_pat| {
                bcx = bind_irrefutable_pat(bcx, *inner_pat, val, true);
            }
      }
        ast::pat_enum(_, sub_pats) => {
            let pat_def = ccx.tcx.def_map.get(pat.id);
            let vdefs = ast_util::variant_def_ids(pat_def);
            let args = extract_variant_args(bcx, pat.id, vdefs, val);
            for sub_pats.each |sub_pat| {
                for vec::eachi(args.vals) |i, argval| {
                    bcx = bind_irrefutable_pat(bcx, sub_pat[i],
                                               argval, make_copy);
                }
            }
        }
        ast::pat_rec(fields, _) | ast::pat_struct(_, fields, _) => {
            let tcx = bcx.tcx();
            let pat_ty = node_id_type(bcx, pat.id);
            do expr::with_field_tys(tcx, pat_ty) |_has_dtor, field_tys| {
                for vec::each(fields) |f| {
                    let ix = ty::field_idx_strict(tcx, f.ident, field_tys);
                    let fldptr = GEPi(bcx, val, struct_field(ix));
                    bcx = bind_irrefutable_pat(bcx, f.pat, fldptr, make_copy);
                }
            }
        }
        ast::pat_tup(elems) => {
            for vec::eachi(elems) |i, elem| {
                let fldptr = GEPi(bcx, val, [0u, i]);
                bcx = bind_irrefutable_pat(bcx, elem, fldptr, make_copy);
            }
        }
        ast::pat_box(inner) | ast::pat_uniq(inner) |
        ast::pat_region(inner) => {
            let llbox = Load(bcx, val);
            let unboxed = GEPi(bcx, llbox, [0u, abi::box_field_body]);
            bcx = bind_irrefutable_pat(bcx, inner, unboxed, true);
        }
        ast::pat_wild | ast::pat_lit(_) | ast::pat_range(_, _) => ()
    }
    return bcx;
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
