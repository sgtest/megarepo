import tstate::ann::*;
import aux::*;
import bitvectors::{bit_num, seq_preconds, seq_postconds,
                    intersect_states,
                    relax_precond_block, gen};
import tritv::*;

import pat_util::*;
import syntax::ast::*;
import syntax::ast_util::*;
import syntax::print::pprust::{expr_to_str, stmt_to_str};
import syntax::visit;
import util::common::{field_exprs, has_nonlocal_exits};
import syntax::codemap::span;
import driver::session::session;
import std::map::hashmap;

fn find_pre_post_mod(_m: _mod) -> _mod {
    debug!{"implement find_pre_post_mod!"};
    fail;
}

fn find_pre_post_foreign_mod(_m: foreign_mod) -> foreign_mod {
    debug!{"implement find_pre_post_foreign_mod"};
    fail;
}

fn find_pre_post_method(ccx: crate_ctxt, m: @method) {
    assert (ccx.fm.contains_key(m.id));
    let fcx: fn_ctxt =
        {enclosing: ccx.fm.get(m.id),
         id: m.id,
         name: m.ident,
         ccx: ccx};
    find_pre_post_fn(fcx, m.body);
}

fn find_pre_post_item(ccx: crate_ctxt, i: item) {
    alt i.node {
      item_const(_, e) {
          // do nothing -- item_consts don't refer to local vars
      }
      item_fn(_, _, body) {
        assert (ccx.fm.contains_key(i.id));
        let fcx =
            {enclosing: ccx.fm.get(i.id), id: i.id, name: i.ident, ccx: ccx};
        find_pre_post_fn(fcx, body);
      }
      item_mod(m) { find_pre_post_mod(m); }
      item_foreign_mod(nm) { find_pre_post_foreign_mod(nm); }
      item_ty(*) | item_enum(*) | item_trait(*) { ret; }
      item_class(*) {
          fail ~"find_pre_post_item: shouldn't be called on item_class";
      }
      item_impl(_, _, _, ms) {
        for ms.each |m| { find_pre_post_method(ccx, m); }
      }
      item_mac(*) { fail ~"item macros unimplemented" }
    }
}


/* Finds the pre and postcondition for each expr in <args>;
   sets the precondition in a to be the result of combining
   the preconditions for <args>, and the postcondition in a to
   be the union of all postconditions for <args> */
fn find_pre_post_exprs(fcx: fn_ctxt, args: ~[@expr], id: node_id) {
    if vec::len::<@expr>(args) > 0u {
        debug!{"find_pre_post_exprs: oper = %s", expr_to_str(args[0])};
    }
    fn do_one(fcx: fn_ctxt, e: @expr) { find_pre_post_expr(fcx, e); }
    for args.each |e| { do_one(fcx, e); }

    fn get_pp(ccx: crate_ctxt, &&e: @expr) -> pre_and_post {
        ret expr_pp(ccx, e);
    }
    let pps = vec::map(args, |a| get_pp(fcx.ccx, a) );

    set_pre_and_post(fcx.ccx, id, seq_preconds(fcx, pps),
                     seq_postconds(fcx, vec::map(pps, get_post)));
}

fn find_pre_post_loop(fcx: fn_ctxt, index: @expr, body: blk, id: node_id) {
    find_pre_post_expr(fcx, index);
    find_pre_post_block(fcx, body);

    let loop_precond =
        seq_preconds(fcx, ~[expr_pp(fcx.ccx, index),
                           block_pp(fcx.ccx, body)]);
    let loop_postcond =
        intersect_states(expr_postcond(fcx.ccx, index),
                         block_postcond(fcx.ccx, body));
    copy_pre_post_(fcx.ccx, id, loop_precond, loop_postcond);
}

// Generates a pre/post assuming that a is the
// annotation for an if-expression with consequent conseq
// and alternative maybe_alt
fn join_then_else(fcx: fn_ctxt, antec: @expr, conseq: blk,
                  maybe_alt: option<@expr>, id: node_id, chck: if_ty) {
    find_pre_post_expr(fcx, antec);
    find_pre_post_block(fcx, conseq);
    alt maybe_alt {
      none {
        alt chck {
          if_check {
            let c: sp_constr = expr_to_constr(fcx.ccx.tcx, antec);
            gen(fcx, antec.id, c.node);
          }
          _ { }
        }

        let precond_res =
            seq_preconds(fcx,
                         ~[expr_pp(fcx.ccx, antec),
                          block_pp(fcx.ccx, conseq)]);
        set_pre_and_post(fcx.ccx, id, precond_res,
                         expr_poststate(fcx.ccx, antec));
      }
      some(altern) {
        /*
          if check = if_check, then
          be sure that the predicate implied by antec
          is *not* true in the alternative
         */
        find_pre_post_expr(fcx, altern);
        let precond_false_case =
            seq_preconds(fcx,
                         ~[expr_pp(fcx.ccx, antec),
                          expr_pp(fcx.ccx, altern)]);
        let postcond_false_case =
            seq_postconds(fcx,
                          ~[expr_postcond(fcx.ccx, antec),
                           expr_postcond(fcx.ccx, altern)]);

        /* Be sure to set the bit for the check condition here,
         so that it's *not* set in the alternative. */
        alt chck {
          if_check {
            let c: sp_constr = expr_to_constr(fcx.ccx.tcx, antec);
            gen(fcx, antec.id, c.node);
          }
          _ { }
        }
        let precond_true_case =
            seq_preconds(fcx,
                         ~[expr_pp(fcx.ccx, antec),
                          block_pp(fcx.ccx, conseq)]);
        let postcond_true_case =
            seq_postconds(fcx,
                          ~[expr_postcond(fcx.ccx, antec),
                           block_postcond(fcx.ccx, conseq)]);

        let precond_res =
            seq_postconds(fcx, ~[precond_true_case, precond_false_case]);
        let postcond_res =
            intersect_states(postcond_true_case, postcond_false_case);
        set_pre_and_post(fcx.ccx, id, precond_res, postcond_res);
      }
    }
}

fn gen_if_local(fcx: fn_ctxt, lhs: @expr, rhs: @expr, larger_id: node_id,
                new_var: node_id) {
    alt node_id_to_def(fcx.ccx, new_var) {
      some(d) {
        alt d {
          def_local(nid, _) {
            find_pre_post_expr(fcx, rhs);
            let p = expr_pp(fcx.ccx, rhs);
            set_pre_and_post(fcx.ccx, larger_id, p.precondition,
                             p.postcondition);
          }
          _ { find_pre_post_exprs(fcx, ~[lhs, rhs], larger_id); }
        }
      }
      _ { find_pre_post_exprs(fcx, ~[lhs, rhs], larger_id); }
    }
}

fn handle_update(fcx: fn_ctxt, parent: @expr, lhs: @expr, rhs: @expr,
                 ty: oper_type) {
    find_pre_post_expr(fcx, rhs);
    alt lhs.node {
      expr_path(p) {
        let post = expr_postcond(fcx.ccx, parent);
        let tmp = post.clone();

        alt ty {
          oper_move {
            if is_path(rhs) { forget_in_postcond(fcx, parent.id, rhs.id); }
          }
          oper_swap {
            forget_in_postcond(fcx, parent.id, lhs.id);
            forget_in_postcond(fcx, parent.id, rhs.id);
          }
          oper_assign {
            forget_in_postcond(fcx, parent.id, lhs.id);
          }
          _ { }
        }

        gen_if_local(fcx, lhs, rhs, parent.id, lhs.id);
        alt rhs.node {
          expr_path(p1) {
            let d = local_node_id_to_local_def_id(fcx, lhs.id);
            let d1 = local_node_id_to_local_def_id(fcx, rhs.id);
            alt d {
              some(id) {
                alt d1 {
                  some(id1) {
                    let instlhs =
                        {ident: path_to_ident(p), node: id};
                    let instrhs =
                        {ident: path_to_ident(p1), node: id1};
                    copy_in_poststate_two(fcx, tmp, post, instlhs, instrhs,
                                          ty);
                  }
                  _ { }
                }
              }
              _ { }
            }
          }
          _ {/* do nothing */ }
        }
      }
      _ { find_pre_post_expr(fcx, lhs); }
    }
}

fn forget_args_moved_in(fcx: fn_ctxt, parent: @expr, modes: ~[mode],
                        operands: ~[@expr]) {
    do vec::iteri(modes) |i,mode| {
        alt ty::resolved_mode(fcx.ccx.tcx, mode) {
          by_move { forget_in_postcond(fcx, parent.id, operands[i].id); }
          by_ref | by_val | by_mutbl_ref | by_copy { }
        }
    }
}

fn find_pre_post_expr_fn_upvars(fcx: fn_ctxt, e: @expr) {
    let rslt = expr_pp(fcx.ccx, e);
    clear_pp(rslt);
}

/* Fills in annotations as a side effect. Does not rebuild the expr */
fn find_pre_post_expr(fcx: fn_ctxt, e: @expr) {
    let enclosing = fcx.enclosing;
    let num_local_vars = num_constraints(enclosing);
    fn do_rand_(fcx: fn_ctxt, e: @expr) { find_pre_post_expr(fcx, e); }


    alt e.node {
      expr_call(operator, operands, _) {
        /* copy */

        let mut args = operands;
        vec::push(args, operator);

        find_pre_post_exprs(fcx, args, e.id);
        /* see if the call has any constraints on its type */
        for constraints_expr(fcx.ccx.tcx, operator).each |c| {
            let i =
                bit_num(fcx, substitute_constr_args(fcx.ccx.tcx, args, c));
            require(i, expr_pp(fcx.ccx, e));
        }

        forget_args_moved_in(fcx, e, callee_modes(fcx, operator.id),
                             operands);

        /* if this is a failing call, its postcondition sets everything */
        alt controlflow_expr(fcx.ccx, operator) {
          noreturn { set_postcond_false(fcx.ccx, e.id); }
          _ { }
        }
      }
      expr_vstore(ee, _) {
        find_pre_post_expr(fcx, ee);
        let p = expr_pp(fcx.ccx, ee);
        set_pre_and_post(fcx.ccx, e.id, p.precondition, p.postcondition);
      }
      expr_vec(args, _) {
        find_pre_post_exprs(fcx, args, e.id);
      }
      expr_path(p) {
        let rslt = expr_pp(fcx.ccx, e);
        clear_pp(rslt);
      }
      expr_new(p, _, v) {
        find_pre_post_exprs(fcx, ~[p, v], e.id);
      }
      expr_log(_, lvl, arg) {
        find_pre_post_exprs(fcx, ~[lvl, arg], e.id);
      }
      expr_fn(_, _, _, cap_clause) | expr_fn_block(_, _, cap_clause) {
        find_pre_post_expr_fn_upvars(fcx, e);

        for (*cap_clause).each |cap_item| {
            let d = local_node_id_to_local_def_id(fcx, cap_item.id);
            option::iter(d, |id| use_var(fcx, id) );
        }

        for (*cap_clause).each |cap_item| {
            if cap_item.is_move {
                log(debug, (~"forget_in_postcond: ", cap_item));
                forget_in_postcond(fcx, e.id, cap_item.id);
            }
        }
      }
      expr_block(b) {
        find_pre_post_block(fcx, b);
        let p = block_pp(fcx.ccx, b);
        set_pre_and_post(fcx.ccx, e.id, p.precondition, p.postcondition);
      }
      expr_rec(fields, maybe_base) {
        let mut es = field_exprs(fields);
        alt maybe_base { none {/* no-op */ } some(b) { vec::push(es, b); } }
        find_pre_post_exprs(fcx, es, e.id);
      }
      expr_tup(elts) { find_pre_post_exprs(fcx, elts, e.id); }
      expr_move(lhs, rhs) { handle_update(fcx, e, lhs, rhs, oper_move); }
      expr_swap(lhs, rhs) { handle_update(fcx, e, lhs, rhs, oper_swap); }
      expr_assign(lhs, rhs) { handle_update(fcx, e, lhs, rhs, oper_assign); }
      expr_assign_op(_, lhs, rhs) {
        /* Different from expr_assign in that the lhs *must*
           already be initialized */

        find_pre_post_exprs(fcx, ~[lhs, rhs], e.id);
        forget_in_postcond(fcx, e.id, lhs.id);
      }
      expr_lit(_) { clear_pp(expr_pp(fcx.ccx, e)); }
      expr_ret(maybe_val) {
        alt maybe_val {
          none {
            clear_precond(fcx.ccx, e.id);
            set_postcond_false(fcx.ccx, e.id);
          }
          some(ret_val) {
            find_pre_post_expr(fcx, ret_val);
            set_precondition(node_id_to_ts_ann(fcx.ccx, e.id),
                             expr_precond(fcx.ccx, ret_val));
            set_postcond_false(fcx.ccx, e.id);
          }
        }
      }
      expr_if(antec, conseq, maybe_alt) {
        join_then_else(fcx, antec, conseq, maybe_alt, e.id, plain_if);
      }
      expr_binary(bop, l, r) {
        if lazy_binop(bop) {
            find_pre_post_expr(fcx, l);
            find_pre_post_expr(fcx, r);
            let overall_pre =
                seq_preconds(fcx,
                             ~[expr_pp(fcx.ccx, l), expr_pp(fcx.ccx, r)]);
            set_precondition(node_id_to_ts_ann(fcx.ccx, e.id), overall_pre);
            set_postcondition(node_id_to_ts_ann(fcx.ccx, e.id),
                              expr_postcond(fcx.ccx, l));
        } else { find_pre_post_exprs(fcx, ~[l, r], e.id); }
      }
      expr_addr_of(_, x) | expr_cast(x, _) | expr_unary(_, x) |
      expr_loop_body(x) | expr_do_body(x) | expr_assert(x) | expr_copy(x) {
        find_pre_post_expr(fcx, x);
        copy_pre_post(fcx.ccx, e.id, x);
      }
      expr_while(test, body) {
        find_pre_post_expr(fcx, test);
        find_pre_post_block(fcx, body);
        set_pre_and_post(fcx.ccx, e.id,
                         seq_preconds(fcx,
                                      ~[expr_pp(fcx.ccx, test),
                                       block_pp(fcx.ccx, body)]),
                         intersect_states(expr_postcond(fcx.ccx, test),
                                          block_postcond(fcx.ccx, body)));
      }
      expr_loop(body) {
        find_pre_post_block(fcx, body);
        /* Infinite loop: if control passes it, everything is true. */
        let mut loop_postcond = false_postcond(num_local_vars);
        /* Conservative approximation: if the body has any nonlocal exits,
         the poststate is blank since we don't know what parts of it
          execute. */
        if has_nonlocal_exits(body) {
            loop_postcond = empty_poststate(num_local_vars);
        }
        set_pre_and_post(fcx.ccx, e.id, block_precond(fcx.ccx, body),
                         loop_postcond);
      }
      expr_index(val, sub) { find_pre_post_exprs(fcx, ~[val, sub], e.id); }
      expr_alt(ex, alts, _) {
        find_pre_post_expr(fcx, ex);
        fn do_an_alt(fcx: fn_ctxt, an_alt: arm) -> pre_and_post {
            alt an_alt.guard {
              some(e) { find_pre_post_expr(fcx, e); }
              _ {}
            }
            find_pre_post_block(fcx, an_alt.body);
            ret block_pp(fcx.ccx, an_alt.body);
        }
        let mut alt_pps = ~[];
        for alts.each |a| { vec::push(alt_pps, do_an_alt(fcx, a)); }
        fn combine_pp(antec: pre_and_post, fcx: fn_ctxt, &&pp: pre_and_post,
                      &&next: pre_and_post) -> pre_and_post {
            union(pp.precondition, seq_preconds(fcx, ~[antec, next]));
            intersect(pp.postcondition, next.postcondition);
            ret pp;
        }
        let antec_pp = pp_clone(expr_pp(fcx.ccx, ex));
        let e_pp =
            {precondition: empty_prestate(num_local_vars),
             postcondition: false_postcond(num_local_vars)};
        let g = |a,b| combine_pp(antec_pp, fcx, a, b);
        let alts_overall_pp =
            vec::foldl(e_pp, alt_pps, g);
        set_pre_and_post(fcx.ccx, e.id, alts_overall_pp.precondition,
                         alts_overall_pp.postcondition);
      }
      expr_field(operator, _, _) {
        find_pre_post_expr(fcx, operator);
        copy_pre_post(fcx.ccx, e.id, operator);
      }
      expr_fail(maybe_val) {
        let mut prestate;
        alt maybe_val {
          none { prestate = empty_prestate(num_local_vars); }
          some(fail_val) {
            find_pre_post_expr(fcx, fail_val);
            prestate = expr_precond(fcx.ccx, fail_val);
          }
        }
        set_pre_and_post(fcx.ccx, e.id,
                         /* if execution continues after fail,
                            then everything is true! */
                         prestate, false_postcond(num_local_vars));
      }
      expr_check(_, p) {
        find_pre_post_expr(fcx, p);
        copy_pre_post(fcx.ccx, e.id, p);
        /* predicate p holds after this expression executes */

        let c: sp_constr = expr_to_constr(fcx.ccx.tcx, p);
        gen(fcx, e.id, c.node);
      }
      expr_if_check(p, conseq, maybe_alt) {
        join_then_else(fcx, p, conseq, maybe_alt, e.id, if_check);
      }
      expr_break { clear_pp(expr_pp(fcx.ccx, e)); }
      expr_again { clear_pp(expr_pp(fcx.ccx, e)); }
      expr_mac(_) { fcx.ccx.tcx.sess.bug(~"unexpanded macro"); }
    }
}

fn find_pre_post_stmt(fcx: fn_ctxt, s: stmt) {
    debug!{"stmt = %s", stmt_to_str(s)};
    alt s.node {
      stmt_decl(adecl, id) {
        alt adecl.node {
          decl_local(alocals) {
            let prev_pp = empty_pre_post(num_constraints(fcx.enclosing));
            for alocals.each |alocal| {
                alt alocal.node.init {
                  some(an_init) {
                    /* LHS always becomes initialized,
                     whether or not this is a move */
                    find_pre_post_expr(fcx, an_init.expr);
                    do pat_bindings(fcx.ccx.tcx.def_map, alocal.node.pat)
                        |p_id, _s, _n| {
                        copy_pre_post(fcx.ccx, p_id, an_init.expr);
                    };
                    /* Inherit ann from initializer, and add var being
                       initialized to the postcondition */
                    copy_pre_post(fcx.ccx, id, an_init.expr);

                    let mut p = none;
                    alt an_init.expr.node {
                      expr_path(_p) { p = some(_p); }
                      _ { }
                    }

                    do pat_bindings(fcx.ccx.tcx.def_map, alocal.node.pat)
                        |p_id, _s, n| {
                        let ident = path_to_ident(n);
                        alt p {
                          some(p) {
                            copy_in_postcond(fcx, id,
                                             {ident: ident, node: p_id},
                                             {ident:
                                                  path_to_ident(p),
                                              node: an_init.expr.id},
                                             op_to_oper_ty(an_init.op));
                          }
                          none { }
                        }
                    };

                    /* Clear out anything that the previous initializer
                    guaranteed */
                    let e_pp = expr_pp(fcx.ccx, an_init.expr);
                    prev_pp.precondition.become(
                               seq_preconds(fcx, ~[prev_pp, e_pp]));

                    /* Include the LHSs too, since those aren't in the
                     postconds of the RHSs themselves */
                    copy_pre_post_(fcx.ccx, id, prev_pp.precondition,
                                   prev_pp.postcondition);
                  }
                  none {
                    do pat_bindings(fcx.ccx.tcx.def_map, alocal.node.pat)
                        |p_id, _s, _n| {
                        clear_pp(node_id_to_ts_ann(fcx.ccx, p_id).conditions);
                    };
                    clear_pp(node_id_to_ts_ann(fcx.ccx, id).conditions);
                  }
                }
            }
          }
          decl_item(anitem) {
            clear_pp(node_id_to_ts_ann(fcx.ccx, id).conditions);
            find_pre_post_item(fcx.ccx, *anitem);
          }
        }
      }
      stmt_expr(e, id) | stmt_semi(e, id) {
        find_pre_post_expr(fcx, e);
        copy_pre_post(fcx.ccx, id, e);
      }
    }
}

fn find_pre_post_block(fcx: fn_ctxt, b: blk) {
    /* Want to say that if there is a break or cont in this
     block, then that invalidates the poststate upheld by
    any of the stmts after it.
    Given that the typechecker has run, we know any break will be in
    a block that forms a loop body. So that's ok. There'll never be an
    expr_break outside a loop body, therefore, no expr_break outside a block.
    */

    /* Conservative approximation for now: This says that if a block contains
     *any* breaks or conts, then its postcondition doesn't promise anything.
     This will mean that:
     x = 0;
     break;

     won't have a postcondition that says x is initialized, but that's ok.
     */

    let nv = num_constraints(fcx.enclosing);
    fn do_one_(fcx: fn_ctxt, s: @stmt) {
        find_pre_post_stmt(fcx, *s);
    }
    for b.node.stmts.each |s| { do_one_(fcx, s); }
    fn do_inner_(fcx: fn_ctxt, &&e: @expr) { find_pre_post_expr(fcx, e); }
    let do_inner = |a| do_inner_(fcx, a);
    option::map::<@expr, ()>(b.node.expr, do_inner);

    let mut pps: ~[pre_and_post] = ~[];
    for b.node.stmts.each |s| { vec::push(pps, stmt_pp(fcx.ccx, *s)); }
    alt b.node.expr {
      none {/* no-op */ }
      some(e) { vec::push(pps, expr_pp(fcx.ccx, e)); }
    }

    let block_precond = seq_preconds(fcx, pps);

    let mut postconds = ~[];
    for pps.each |pp| { vec::push(postconds, get_post(pp)); }

    /* A block may be empty, so this next line ensures that the postconds
       vector is non-empty. */
    vec::push(postconds, block_precond);

    let mut block_postcond = empty_poststate(nv);
    /* conservative approximation */

    if !has_nonlocal_exits(b) {
        block_postcond = seq_postconds(fcx, postconds);
    }
    set_pre_and_post(fcx.ccx, b.node.id, block_precond, block_postcond);
}

fn find_pre_post_fn(fcx: fn_ctxt, body: blk) {
    find_pre_post_block(fcx, body);

    // Treat the tail expression as a return statement
    alt body.node.expr {
      some(tailexpr) { set_postcond_false(fcx.ccx, tailexpr.id); }
      none {/* fallthrough */ }
    }
}

fn fn_pre_post(fk: visit::fn_kind, decl: fn_decl, body: blk, sp: span,
               id: node_id,
               ccx: crate_ctxt, v: visit::vt<crate_ctxt>) {

    visit::visit_fn(fk, decl, body, sp, id, ccx, v);
    assert (ccx.fm.contains_key(id));
    if !ccx.fm.get(id).ignore {
        let fcx =
            {enclosing: ccx.fm.get(id),
             id: id,
             name: visit::name_of_fn(fk),
             ccx: ccx};
        find_pre_post_fn(fcx, body);
    }
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
