import driver::session::session;
import lib::llvm::llvm;
import lib::llvm::{ValueRef, BasicBlockRef};
import pat_util::*;
import build::*;
import base::*;
import syntax::ast;
import syntax::ast_util;
import syntax::ast_util::{dummy_sp};
import syntax::ast::def_id;
import syntax::codemap::span;
import syntax::print::pprust::pat_to_str;
import back::abi;

import common::*;

// An option identifying a branch (either a literal, a enum variant or a
// range)
enum opt {
    lit(@ast::expr),
    var(/* disr val */int, /* variant dids */{enm: def_id, var: def_id}),
    range(@ast::expr, @ast::expr)
}
fn opt_eq(a: opt, b: opt) -> bool {
    alt (a, b) {
      (lit(a), lit(b)) { ast_util::compare_lit_exprs(a, b) == 0 }
      (range(a1, a2), range(b1, b2)) {
        ast_util::compare_lit_exprs(a1, b1) == 0 &&
        ast_util::compare_lit_exprs(a2, b2) == 0
      }
      (var(a, _), var(b, _)) { a == b }
      _ { false }
    }
}

enum opt_result {
    single_result(result),
    range_result(result, result),
}
fn trans_opt(bcx: block, o: opt) -> opt_result {
    let ccx = bcx.ccx(), bcx = bcx;
    alt o {
      lit(l) {
        alt l.node {
          ast::expr_lit(@{node: ast::lit_str(s), _}) {
            let strty = ty::mk_str(bcx.tcx());
            let cell = empty_dest_cell();
            bcx = tvec::trans_str(bcx, s, by_val(cell));
            add_clean_temp(bcx, *cell, strty);
            ret single_result(rslt(bcx, *cell));
          }
          _ {
            ret single_result(
                rslt(bcx, trans_const_expr(ccx, l)));
          }
        }
      }
      var(disr_val, _) { ret single_result(rslt(bcx, C_int(ccx, disr_val))); }
      range(l1, l2) {
        ret range_result(rslt(bcx, trans_const_expr(ccx, l1)),
                         rslt(bcx, trans_const_expr(ccx, l2)));
      }
    }
}

fn variant_opt(ccx: crate_ctxt, pat_id: ast::node_id) -> opt {
    let vdef = ast_util::variant_def_ids(ccx.tcx.def_map.get(pat_id));
    let variants = ty::enum_variants(ccx.tcx, vdef.enm);
    for v: ty::variant_info in *variants {
        if vdef.var == v.id { ret var(v.disr_val, vdef); }
    }
    fail;
}

type bind_map = [{ident: ast::ident, val: ValueRef}];
fn assoc(key: str, list: bind_map) -> option<ValueRef> {
    for elt: {ident: ast::ident, val: ValueRef} in list {
        if str::eq(elt.ident, key) { ret some(elt.val); }
    }
    ret none;
}

type match_branch =
    @{pats: [@ast::pat],
      bound: bind_map,
      data: @{body: BasicBlockRef,
              guard: option<@ast::expr>,
              id_map: pat_id_map}};
type match = [match_branch];

fn has_nested_bindings(m: match, col: uint) -> bool {
    for br in m {
        alt br.pats[col].node {
          ast::pat_ident(_, some(_)) { ret true; }
          _ {}
        }
    }
    ret false;
}

fn expand_nested_bindings(m: match, col: uint, val: ValueRef) -> match {
    let result = [];
    for br in m {
      alt br.pats[col].node {
          ast::pat_ident(name, some(inner)) {
            let pats = vec::slice(br.pats, 0u, col) + [inner] +
                vec::slice(br.pats, col + 1u, br.pats.len());
            result += [@{pats: pats,
                        bound: br.bound + [{ident: path_to_ident(name),
                                val: val}]
                         with *br}];
          }
          _ { result += [br]; }
        }
    }
    result
}

type enter_pat = fn@(@ast::pat) -> option<[@ast::pat]>;

fn enter_match(m: match, col: uint, val: ValueRef, e: enter_pat) -> match {
    let result = [];
    for br: match_branch in m {
        alt e(br.pats[col]) {
          some(sub) {
            let pats = sub + vec::slice(br.pats, 0u, col) +
                vec::slice(br.pats, col + 1u, br.pats.len());
            let new_br = @{pats: pats,
                           bound: alt br.pats[col].node {
                             ast::pat_ident(name, none) {
                                 br.bound + [{ident: path_to_ident(name),
                                              val: val}]
                             }
                             _ { br.bound }
                           } with *br};
            result += [new_br];
          }
          none { }
        }
    }
    ret result;
}

fn enter_default(m: match, col: uint, val: ValueRef) -> match {
    fn matches_always(p: @ast::pat) -> bool {
        alt p.node {
                ast::pat_wild | ast::pat_rec(_, _) |
                ast::pat_ident(_, none) | ast::pat_tup(_) { true }
                _ { false }
        }
    }
    fn e(p: @ast::pat) -> option<[@ast::pat]> {
        ret if matches_always(p) { some([]) } else { none };
    }
    ret enter_match(m, col, val, e);
}

fn enter_opt(ccx: crate_ctxt, m: match, opt: opt, col: uint, enum_size: uint,
             val: ValueRef) -> match {
    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    fn e(ccx: crate_ctxt, dummy: @ast::pat, opt: opt, size: uint,
         p: @ast::pat) -> option<[@ast::pat]> {
        alt p.node {
          ast::pat_enum(ctor, subpats) {
            ret if opt_eq(variant_opt(ccx, p.id), opt) {
                    some(subpats)
                } else { none };
          }
          ast::pat_lit(l) {
            ret if opt_eq(lit(l), opt) { some([]) } else { none };
          }
          ast::pat_range(l1, l2) {
            ret if opt_eq(range(l1, l2), opt) { some([]) } else { none };
          }
          _ { ret some(vec::init_elt(size, dummy)); }
        }
    }
    ret enter_match(m, col, val, bind e(ccx, dummy, opt, enum_size, _));
}

fn enter_rec(m: match, col: uint, fields: [ast::ident], val: ValueRef) ->
   match {
    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    fn e(dummy: @ast::pat, fields: [ast::ident], p: @ast::pat) ->
       option<[@ast::pat]> {
        alt p.node {
          ast::pat_rec(fpats, _) {
            let pats = [];
            for fname: ast::ident in fields {
                let pat = dummy;
                for fpat: ast::field_pat in fpats {
                    if str::eq(fpat.ident, fname) { pat = fpat.pat; break; }
                }
                pats += [pat];
            }
            ret some(pats);
          }
          _ { ret some(vec::init_elt(fields.len(), dummy)); }
        }
    }
    ret enter_match(m, col, val, bind e(dummy, fields, _));
}

fn enter_tup(m: match, col: uint, val: ValueRef, n_elts: uint) -> match {
    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    fn e(dummy: @ast::pat, n_elts: uint, p: @ast::pat) ->
       option<[@ast::pat]> {
        alt p.node {
          ast::pat_tup(elts) { ret some(elts); }
          _ { ret some(vec::init_elt(n_elts, dummy)); }
        }
    }
    ret enter_match(m, col, val, bind e(dummy, n_elts, _));
}

fn enter_box(m: match, col: uint, val: ValueRef) -> match {
    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    fn e(dummy: @ast::pat, p: @ast::pat) -> option<[@ast::pat]> {
        alt p.node {
          ast::pat_box(sub) { ret some([sub]); }
          _ { ret some([dummy]); }
        }
    }
    ret enter_match(m, col, val, bind e(dummy, _));
}

fn enter_uniq(m: match, col: uint, val: ValueRef) -> match {
    let dummy = @{id: 0, node: ast::pat_wild, span: dummy_sp()};
    fn e(dummy: @ast::pat, p: @ast::pat) -> option<[@ast::pat]> {
        alt p.node {
          ast::pat_uniq(sub) { ret some([sub]); }
          _ { ret some([dummy]); }
        }
    }
    ret enter_match(m, col, val, bind e(dummy, _));
}

fn get_options(ccx: crate_ctxt, m: match, col: uint) -> [opt] {
    fn add_to_set(&set: [opt], val: opt) {
        for l: opt in set { if opt_eq(l, val) { ret; } }
        set += [val];
    }

    let found = [];
    for br: match_branch in m {
        alt br.pats[col].node {
          ast::pat_lit(l) { add_to_set(found, lit(l)); }
          ast::pat_range(l1, l2) {
            add_to_set(found, range(l1, l2));
          }
          ast::pat_enum(_, _) {
            add_to_set(found, variant_opt(ccx, br.pats[col].id));
          }
          _ { }
        }
    }
    ret found;
}

fn extract_variant_args(bcx: block, pat_id: ast::node_id,
                        vdefs: {enm: def_id, var: def_id}, val: ValueRef) ->
   {vals: [ValueRef], bcx: block} {
    let ccx = bcx.fcx.ccx, bcx = bcx;
    // invariant:
    // pat_id must have the same length ty_param_substs as vdefs?
    let ty_param_substs = node_id_type_params(bcx, pat_id);
    let blobptr = val;
    let variants = ty::enum_variants(ccx.tcx, vdefs.enm);
    let args = [];
    let size = ty::enum_variant_with_id(ccx.tcx, vdefs.enm,
                                        vdefs.var).args.len();
    if size > 0u && (*variants).len() != 1u {
        let enumptr =
            PointerCast(bcx, val, T_opaque_enum_ptr(ccx));
        blobptr = GEPi(bcx, enumptr, [0, 1]);
    }
    let i = 0u;
    let vdefs_tg = vdefs.enm;
    let vdefs_var = vdefs.var;
    while i < size {
        check (valid_variant_index(i, bcx, vdefs_tg, vdefs_var));
        let r =
            // invariant needed:
            // how do we know it even makes sense to pass in ty_param_substs
            // here? What if it's [] and the enum type has variables in it?
            GEP_enum(bcx, blobptr, vdefs_tg, vdefs_var,
                            ty_param_substs, i);
        bcx = r.bcx;
        args += [r.val];
        i += 1u;
    }
    ret {vals: args, bcx: bcx};
}

fn collect_record_fields(m: match, col: uint) -> [ast::ident] {
    let fields = [];
    for br: match_branch in m {
        alt br.pats[col].node {
          ast::pat_rec(fs, _) {
            for f: ast::field_pat in fs {
                if !vec::any(fields, bind str::eq(f.ident, _)) {
                    fields += [f.ident];
                }
            }
          }
          _ { }
        }
    }
    ret fields;
}

fn any_box_pat(m: match, col: uint) -> bool {
    for br: match_branch in m {
        alt br.pats[col].node { ast::pat_box(_) { ret true; } _ { } }
    }
    ret false;
}

fn any_uniq_pat(m: match, col: uint) -> bool {
    for br: match_branch in m {
        alt br.pats[col].node { ast::pat_uniq(_) { ret true; } _ { } }
    }
    ret false;
}

fn any_tup_pat(m: match, col: uint) -> bool {
    for br: match_branch in m {
        alt br.pats[col].node { ast::pat_tup(_) { ret true; } _ { } }
    }
    ret false;
}

type exit_node = {bound: bind_map, from: BasicBlockRef, to: BasicBlockRef};
type mk_fail = fn@() -> BasicBlockRef;

fn pick_col(m: match) -> uint {
    fn score(p: @ast::pat) -> uint {
        alt p.node {
          ast::pat_lit(_) | ast::pat_enum(_, _) | ast::pat_range(_, _) { 1u }
          ast::pat_ident(_, some(p)) { score(p) }
          _ { 0u }
        }
    }
    let scores = vec::init_elt_mut(m[0].pats.len(), 0u);
    for br: match_branch in m {
        let i = 0u;
        for p: @ast::pat in br.pats { scores[i] += score(p); i += 1u; }
    }
    let max_score = 0u;
    let best_col = 0u;
    let i = 0u;
    for score: uint in scores {
        // Irrefutable columns always go first, they'd only be duplicated in
        // the branches.
        if score == 0u { ret i; }
        // If no irrefutable ones are found, we pick the one with the biggest
        // branching factor.
        if score > max_score { max_score = score; best_col = i; }
        i += 1u;
    }
    ret best_col;
}

fn compile_submatch(bcx: block, m: match, vals: [ValueRef], f: mk_fail,
                    &exits: [exit_node]) {
    let bcx = bcx;
    if m.len() == 0u { Br(bcx, f()); ret; }
    if m[0].pats.len() == 0u {
        let data = m[0].data;
        alt data.guard {
          some(e) {
            // Temporarily set bindings. They'll be rewritten to PHI nodes
            // for the actual arm block.
            data.id_map.items {|key, val|
                let loc = local_mem(option::get(assoc(key, m[0].bound)));
                bcx.fcx.lllocals.insert(val, loc);
            };
            let {bcx: guard_cx, val} = with_scope_result(bcx, "guard") {|bcx|
                trans_temp_expr(bcx, e)
            };
            bcx = with_cond(guard_cx, Not(guard_cx, val)) {|bcx|
                compile_submatch(bcx, vec::tail(m), vals, f, exits);
                bcx
            };
          }
          _ { }
        }
        if !bcx.unreachable {
            exits += [{bound: m[0].bound, from: bcx.llbb, to: data.body}];
        }
        Br(bcx, data.body);
        ret;
    }

    let col = pick_col(m);
    let val = vals[col];
    let m = if has_nested_bindings(m, col) {
                expand_nested_bindings(m, col, val)
            } else {
                m
            };

    let vals_left =
        vec::slice(vals, 0u, col) +
            vec::slice(vals, col + 1u, vals.len());
    let ccx = bcx.fcx.ccx;
    let pat_id = 0;
    for br: match_branch in m {
        // Find a real id (we're adding placeholder wildcard patterns, but
        // each column is guaranteed to have at least one real pattern)
        if pat_id == 0 { pat_id = br.pats[col].id; }
    }

    let rec_fields = collect_record_fields(m, col);
    // Separate path for extracting and binding record fields
    if rec_fields.len() > 0u {
        let rec_ty = node_id_type(bcx, pat_id);
        let fields = ty::get_fields(rec_ty);
        let rec_vals = [];
        for field_name: ast::ident in rec_fields {
            let ix = option::get(ty::field_idx(field_name, fields));
            let r = GEP_tup_like(bcx, rec_ty, val, [0, ix as int]);
            rec_vals += [r.val];
            bcx = r.bcx;
        }
        compile_submatch(bcx, enter_rec(m, col, rec_fields, val),
                         rec_vals + vals_left, f, exits);
        ret;
    }

    if any_tup_pat(m, col) {
        let tup_ty = node_id_type(bcx, pat_id);
        let n_tup_elts = alt ty::get(tup_ty).struct {
          ty::ty_tup(elts) { elts.len() }
          _ { ccx.sess.bug("Non-tuple type in tuple pattern"); }
        };
        let tup_vals = [], i = 0u;
        while i < n_tup_elts {
            let r = GEP_tup_like(bcx, tup_ty, val, [0, i as int]);
            tup_vals += [r.val];
            bcx = r.bcx;
            i += 1u;
        }
        compile_submatch(bcx, enter_tup(m, col, val, n_tup_elts),
                         tup_vals + vals_left, f, exits);
        ret;
    }

    // Unbox in case of a box field
    if any_box_pat(m, col) {
        let box = Load(bcx, val);
        let unboxed = GEPi(bcx, box, [0, abi::box_field_body]);
        compile_submatch(bcx, enter_box(m, col, val), [unboxed] + vals_left,
                         f, exits);
        ret;
    }

    if any_uniq_pat(m, col) {
        let unboxed = Load(bcx, val);
        compile_submatch(bcx, enter_uniq(m, col, val),
                         [unboxed] + vals_left, f, exits);
        ret;
    }

    // Decide what kind of branch we need
    let opts = get_options(ccx, m, col);
    enum branch_kind { no_branch, single, switch, compare, }
    let kind = no_branch;
    let test_val = val;
    if opts.len() > 0u {
        alt opts[0] {
          var(_, vdef) {
            if (*ty::enum_variants(ccx.tcx, vdef.enm)).len() == 1u {
                kind = single;
            } else {
                let enumptr =
                    PointerCast(bcx, val, T_opaque_enum_ptr(ccx));
                let discrimptr = GEPi(bcx, enumptr, [0, 0]);
                test_val = Load(bcx, discrimptr);
                kind = switch;
            }
          }
          lit(l) {
            test_val = Load(bcx, val);
            let pty = node_id_type(bcx, pat_id);
            kind = if ty::type_is_integral(pty) { switch }
                   else { compare };
          }
          range(_, _) {
            test_val = Load(bcx, val);
            kind = compare;
          }
        }
    }
    for o: opt in opts {
        alt o {
          range(_, _) { kind = compare; break; }
          _ { }
        }
    }
    let else_cx = alt kind {
      no_branch | single { bcx }
      _ { sub_block(bcx, "match_else") }
    };
    let sw = if kind == switch {
        Switch(bcx, test_val, else_cx.llbb, opts.len())
    } else { C_int(ccx, 0) }; // Placeholder for when not using a switch

     // Compile subtrees for each option
    for opt: opt in opts {
        let opt_cx = sub_block(bcx, "match_case");
        alt kind {
          single { Br(bcx, opt_cx.llbb); }
          switch {
            let res = trans_opt(bcx, opt);
            alt res {
              single_result(r) {
                llvm::LLVMAddCase(sw, r.val, opt_cx.llbb);
                bcx = r.bcx;
              }
              _ { bcx.tcx().sess.bug("Someone forgot to\
                    document an invariant in compile_submatch"); }
            }
          }
          compare {
            let t = node_id_type(bcx, pat_id);
            let {bcx: after_cx, val: matches} =
                with_scope_result(bcx, "compare_scope") {|bcx|
                alt trans_opt(bcx, opt) {
                  single_result({bcx, val}) {
                    trans_compare(bcx, ast::eq, test_val, t, val, t)
                  }
                  range_result({val: vbegin, _}, {bcx, val: vend}) {
                    let {bcx, val: ge} = trans_compare(bcx, ast::ge, test_val,
                                                       t, vbegin, t);
                    let {bcx, val: le} = trans_compare(bcx, ast::le, test_val,
                                                       t, vend, t);
                    {bcx: bcx, val: And(bcx, ge, le)}
                  }
                }
            };
            bcx = sub_block(after_cx, "compare_next");
            CondBr(after_cx, matches, opt_cx.llbb, bcx.llbb);
          }
          _ { }
        }
        let size = 0u;
        let unpacked = [];
        alt opt {
          var(_, vdef) {
            let args = extract_variant_args(opt_cx, pat_id, vdef, val);
            size = args.vals.len();
            unpacked = args.vals;
            opt_cx = args.bcx;
          }
          lit(_) | range(_, _) { }
        }
        compile_submatch(opt_cx, enter_opt(ccx, m, opt, col, size, val),
                         unpacked + vals_left, f, exits);
    }

    // Compile the fall-through case
    if kind == compare { Br(bcx, else_cx.llbb); }
    if kind != single {
        compile_submatch(else_cx, enter_default(m, col, val), vals_left, f,
                         exits);
    }
}

// Returns false for unreachable blocks
fn make_phi_bindings(bcx: block, map: [exit_node],
                     ids: pat_util::pat_id_map) -> bool {
    let our_block = bcx.llbb as uint;
    let success = true, bcx = bcx;
    ids.items {|name, node_id|
        let llbbs = [];
        let vals = [];
        for ex: exit_node in map {
            if ex.to as uint == our_block {
                alt assoc(name, ex.bound) {
                  some(val) { llbbs += [ex.from]; vals += [val]; }
                  none { }
                }
            }
        }
        if vals.len() > 0u {
            let local = Phi(bcx, val_ty(vals[0]), vals, llbbs);
            bcx.fcx.lllocals.insert(node_id, local_mem(local));
        } else { success = false; }
    };
    if success {
        // Copy references that the alias analysis considered unsafe
        ids.values {|node_id|
            if bcx.ccx().copy_map.contains_key(node_id) {
                let local = alt bcx.fcx.lllocals.find(node_id) {
                  some(local_mem(x)) { x }
                  _ { bcx.tcx().sess.bug("Someone \
                        forgot to document an invariant in \
                        make_phi_bindings"); }
                };
                let e_ty = node_id_type(bcx, node_id);
                let {bcx: abcx, val: alloc} = alloc_ty(bcx, e_ty);
                bcx = copy_val(abcx, INIT, alloc,
                                      load_if_immediate(abcx, local, e_ty),
                                      e_ty);
                add_clean(bcx, alloc, e_ty);
                bcx.fcx.lllocals.insert(node_id, local_mem(alloc));
            }
        };
    } else {
        Unreachable(bcx);
    }
    ret success;
}

fn trans_alt(bcx: block, expr: @ast::expr, arms: [ast::arm],
             dest: dest) -> block {
    with_scope(bcx, "alt") {|bcx| trans_alt_inner(bcx, expr, arms, dest)}
}

fn trans_alt_inner(scope_cx: block, expr: @ast::expr, arms: [ast::arm],
                   dest: dest) -> block {
    let bcx = scope_cx, tcx = bcx.tcx();
    let bodies = [], match = [];

    let {bcx, val, _} = trans_temp_expr(bcx, expr);
    if bcx.unreachable { ret bcx; }

    // n.b. nothing else in this module should need to normalize,
    // b/c of this call
    let arms = normalize_arms(tcx, arms);

    for a in arms {
        let body = scope_block(bcx, "case_body");
        body.block_span = some(a.body.span);
        let id_map = pat_util::pat_id_map(tcx, a.pats[0]);
        bodies += [body];
        for p in a.pats {
            match += [@{pats: [p],
                        bound: [],
                        data: @{body: body.llbb, guard: a.guard,
                                id_map: id_map}}];
        }
    }

    // Cached fail-on-fallthrough block
    let fail_cx = @mutable none;
    fn mk_fail(bcx: block, sp: span,
               done: @mutable option<BasicBlockRef>) -> BasicBlockRef {
        alt *done { some(bb) { ret bb; } _ { } }
        let fail_cx = sub_block(bcx, "case_fallthrough");
        trans_fail(fail_cx, some(sp), "non-exhaustive match failure");;
        *done = some(fail_cx.llbb);
        ret fail_cx.llbb;
    }

    let exit_map = [];
    let t = node_id_type(bcx, expr.id);
    let {bcx, val: spilled} = spill_if_immediate(bcx, val, t);
    compile_submatch(bcx, match, [spilled],
                     bind mk_fail(scope_cx, expr.span, fail_cx), exit_map);

    let arm_cxs = [], arm_dests = [], i = 0u;
    for a in arms {
        let body_cx = bodies[i];
        if make_phi_bindings(body_cx, exit_map,
                             pat_util::pat_id_map(tcx, a.pats[0])) {
            let arm_dest = dup_for_join(dest);
            arm_dests += [arm_dest];
            let arm_cx = trans_block(body_cx, a.body, arm_dest);
            arm_cx = trans_block_cleanups(arm_cx, body_cx);
            arm_cxs += [arm_cx];
        }
        i += 1u;
    }
    join_returns(scope_cx, arm_cxs, arm_dests, dest)
}

// Not alt-related, but similar to the pattern-munging code above
fn bind_irrefutable_pat(bcx: block, pat: @ast::pat, val: ValueRef,
                        make_copy: bool) -> block {
    let ccx = bcx.fcx.ccx, bcx = bcx;

    // Necessary since bind_irrefutable_pat is called outside trans_alt
    alt normalize_pat(bcx.tcx(), pat).node {
      ast::pat_ident(_,inner) {
        if make_copy || ccx.copy_map.contains_key(pat.id) {
            let ty = node_id_type(bcx, pat.id);
            let llty = type_of::type_of(ccx, ty);
            let alloc = alloca(bcx, llty);
            bcx = copy_val(bcx, INIT, alloc,
                                  load_if_immediate(bcx, val, ty), ty);
            bcx.fcx.lllocals.insert(pat.id, local_mem(alloc));
            add_clean(bcx, alloc, ty);
        } else { bcx.fcx.lllocals.insert(pat.id, local_mem(val)); }
        alt inner {
          some(pat) { bcx = bind_irrefutable_pat(bcx, pat, val, true); }
          _ {}
        }
      }
      ast::pat_enum(_, sub) {
        if sub.len() == 0u { ret bcx; }
        let vdefs = ast_util::variant_def_ids(ccx.tcx.def_map.get(pat.id));
        let args = extract_variant_args(bcx, pat.id, vdefs, val);
        let i = 0;
        for argval: ValueRef in args.vals {
            bcx = bind_irrefutable_pat(bcx, sub[i], argval, make_copy);
            i += 1;
        }
      }
      ast::pat_rec(fields, _) {
        let rec_ty = node_id_type(bcx, pat.id);
        let rec_fields = ty::get_fields(rec_ty);
        for f: ast::field_pat in fields {
            let ix = option::get(ty::field_idx(f.ident, rec_fields));
            // how to get rid of this check?
            let r = GEP_tup_like(bcx, rec_ty, val, [0, ix as int]);
            bcx = bind_irrefutable_pat(r.bcx, f.pat, r.val, make_copy);
        }
      }
      ast::pat_tup(elems) {
        let tup_ty = node_id_type(bcx, pat.id);
        let i = 0u;
        for elem in elems {
            let r = GEP_tup_like(bcx, tup_ty, val, [0, i as int]);
            bcx = bind_irrefutable_pat(r.bcx, elem, r.val, make_copy);
            i += 1u;
        }
      }
      ast::pat_box(inner) {
        let box = Load(bcx, val);
        let unboxed =
            GEPi(bcx, box, [0, abi::box_field_body]);
        bcx = bind_irrefutable_pat(bcx, inner, unboxed, true);
      }
      ast::pat_uniq(inner) {
        let val = Load(bcx, val);
        bcx = bind_irrefutable_pat(bcx, inner, val, true);
      }
      ast::pat_wild | ast::pat_lit(_) | ast::pat_range(_, _) { }
    }
    ret bcx;
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
