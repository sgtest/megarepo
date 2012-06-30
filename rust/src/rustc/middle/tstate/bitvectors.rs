import syntax::ast::*;
import syntax::visit;
import option::*;
import aux::*;
import tstate::ann::{pre_and_post, precond, postcond, prestate, poststate,
                     relax_prestate, relax_precond, relax_poststate,
                     pps_len, true_precond,
                     difference, union, clone,
                     set_in_postcond, set_in_poststate, set_in_poststate_,
                     clear_in_poststate, clear_in_prestate,
                     clear_in_poststate_};
import tritv::*;
import driver::session::session;
import std::map::hashmap;

fn bit_num(fcx: fn_ctxt, c: tsconstr) -> uint {
    let d = c.def_id;
    assert (fcx.enclosing.constrs.contains_key(d));
    let rslt = fcx.enclosing.constrs.get(d);
    match_args(fcx, rslt.descs, c.args)
}

fn promises(fcx: fn_ctxt, p: poststate, c: tsconstr) -> bool {
    ret promises_(bit_num(fcx, c), p);
}

fn promises_(n: uint, p: poststate) -> bool {
    ret tritv_get(p, n) == ttrue;
}

// v "happens after" u
fn seq_trit(u: trit, v: trit) -> trit {
    alt v { ttrue { ttrue } tfalse { tfalse } dont_care { u } }
}

// idea: q "happens after" p -- so if something is
// 1 in q and 0 in p, it's 1 in the result; however,
// if it's 0 in q and 1 in p, it's 0 in the result
fn seq_tritv(p: postcond, q: postcond) {
    let mut i = 0u;
    assert (p.nbits == q.nbits);
    while i < p.nbits {
        tritv_set(i, p, seq_trit(tritv_get(p, i), tritv_get(q, i)));
        i += 1u;
    }
}

fn seq_postconds(fcx: fn_ctxt, ps: ~[postcond]) -> postcond {
    let sz = vec::len(ps);
    if sz >= 1u {
        let prev = tritv_clone(ps[0]);
        vec::iter_between(ps, 1u, sz, {|p| seq_tritv(prev, p); });
        ret prev;
    } else { ret ann::empty_poststate(num_constraints(fcx.enclosing)); }
}

// Given a list of pres and posts for exprs e0 ... en,
// return the precondition for evaluating each expr in order.
// So, if e0's post is {x} and e1's pre is {x, y, z}, the entire
// precondition shouldn't include x.
fn seq_preconds(fcx: fn_ctxt, pps: ~[pre_and_post]) -> precond {
    let sz: uint = vec::len(pps);
    let num_vars: uint = num_constraints(fcx.enclosing);

    fn seq_preconds_go(fcx: fn_ctxt, pps: ~[pre_and_post],
                       idx: uint, first: pre_and_post)
       -> precond {
        let mut idx = idx;
        let mut first = first;
        loop {
            let sz: uint = vec::len(pps) - idx;
            if sz >= 1u {
                let second = pps[0];
                assert (pps_len(second) == num_constraints(fcx.enclosing));
                let second_pre = clone(second.precondition);
                difference(second_pre, first.postcondition);
                let next_first = clone(first.precondition);
                union(next_first, second_pre);
                let next_first_post = clone(first.postcondition);
                seq_tritv(next_first_post, second.postcondition);
                idx += 1u;
                first = {precondition: next_first,
                         postcondition: next_first_post};
            } else { ret first.precondition; }
        }
    }

    if sz >= 1u {
        let first = pps[0];
        assert (pps_len(first) == num_vars);
        ret seq_preconds_go(fcx, pps, 1u, first);
    } else { ret true_precond(num_vars); }
}

fn intersect_states(p: prestate, q: prestate) -> prestate {
    let rslt = tritv_clone(p);
    tritv_intersect(rslt, q);
    ret rslt;
}

fn gen(fcx: fn_ctxt, id: node_id, c: tsconstr) -> bool {
    ret set_in_postcond(bit_num(fcx, c),
                        node_id_to_ts_ann(fcx.ccx, id).conditions);
}

fn declare_var(fcx: fn_ctxt, c: tsconstr, pre: prestate) -> prestate {
    let rslt = clone(pre);
    relax_prestate(bit_num(fcx, c), rslt);
    // idea is this is scoped
    relax_poststate(bit_num(fcx, c), rslt);
    ret rslt;
}

fn relax_precond_expr(e: @expr, cx: relax_ctxt, vt: visit::vt<relax_ctxt>) {
    relax_precond(cx.i as uint, expr_precond(cx.fcx.ccx, e));
    visit::visit_expr(e, cx, vt);
}

fn relax_precond_stmt(s: @stmt, cx: relax_ctxt, vt: visit::vt<relax_ctxt>) {
    relax_precond(cx.i as uint, stmt_precond(cx.fcx.ccx, *s));
    visit::visit_stmt(s, cx, vt);
}

type relax_ctxt = {fcx: fn_ctxt, i: node_id};

fn relax_precond_block_inner(b: blk, cx: relax_ctxt,
                             vt: visit::vt<relax_ctxt>) {
    relax_precond(cx.i as uint, block_precond(cx.fcx.ccx, b));
    visit::visit_block(b, cx, vt);
}

fn relax_precond_block(fcx: fn_ctxt, i: node_id, b: blk) {
    let cx = {fcx: fcx, i: i};
    let visitor = visit::default_visitor::<relax_ctxt>();
    let visitor =
        @{visit_block: relax_precond_block_inner,
          visit_expr: relax_precond_expr,
          visit_stmt: relax_precond_stmt,
          visit_item:
              fn@(_i: @item, _cx: relax_ctxt, _vt: visit::vt<relax_ctxt>) { },
          visit_fn: do_nothing
             with *visitor};
    let v1 = visit::mk_vt(visitor);
    v1.visit_block(b, cx, v1);
}

fn gen_poststate(fcx: fn_ctxt, id: node_id, c: tsconstr) -> bool {
    #debug("gen_poststate");
    ret set_in_poststate(bit_num(fcx, c),
                         node_id_to_ts_ann(fcx.ccx, id).states);
}

fn kill_prestate(fcx: fn_ctxt, id: node_id, c: tsconstr) -> bool {
    ret clear_in_prestate(bit_num(fcx, c),
                          node_id_to_ts_ann(fcx.ccx, id).states);
}

fn kill_all_prestate(fcx: fn_ctxt, id: node_id) {
    tritv::tritv_kill(node_id_to_ts_ann(fcx.ccx, id).states.prestate);
}


fn kill_poststate(fcx: fn_ctxt, id: node_id, c: tsconstr) -> bool {
    #debug("kill_poststate");
    ret clear_in_poststate(bit_num(fcx, c),
                           node_id_to_ts_ann(fcx.ccx, id).states);
}

fn kill_poststate_(fcx: fn_ctxt, c: tsconstr, post: poststate) -> bool {
    #debug("kill_poststate_");
    ret clear_in_poststate_(bit_num(fcx, c), post);
}

fn set_in_prestate_constr(fcx: fn_ctxt, c: tsconstr, t: prestate) -> bool {
    ret set_in_poststate_(bit_num(fcx, c), t);
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
