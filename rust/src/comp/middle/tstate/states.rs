
import std::bitv;
import std::vec;
import std::vec::plus_option;
import std::vec::cat_options;
import std::option;
import std::option::get;
import std::option::is_none;
import std::option::none;
import std::option::some;
import std::option::maybe;
import tstate::ann::pre_and_post;
import tstate::ann::get_post;
import tstate::ann::postcond;
import tstate::ann::empty_pre_post;
import tstate::ann::empty_poststate;
import tstate::ann::require_and_preserve;
import tstate::ann::union;
import tstate::ann::intersect;
import tstate::ann::empty_prestate;
import tstate::ann::prestate;
import tstate::ann::poststate;
import tstate::ann::false_postcond;
import tstate::ann::ts_ann;
import tstate::ann::extend_prestate;
import tstate::ann::extend_poststate;
import aux::crate_ctxt;
import aux::fn_ctxt;
import aux::num_constraints;
import aux::expr_pp;
import aux::stmt_pp;
import aux::block_pp;
import aux::set_pre_and_post;
import aux::expr_prestate;
import aux::stmt_poststate;
import aux::expr_poststate;
import aux::block_poststate;
import aux::fn_info;
import aux::log_pp;
import aux::extend_prestate_ann;
import aux::extend_poststate_ann;
import aux::set_prestate_ann;
import aux::set_poststate_ann;
import aux::pure_exp;
import aux::log_bitv;
import aux::stmt_to_ann;
import aux::log_states;
import aux::block_states;
import aux::controlflow_expr;
import aux::ann_to_def;
import aux::expr_to_constr;
import aux::ninit;
import aux::npred;
import aux::path_to_ident;
import bitvectors::seq_preconds;
import bitvectors::union_postconds;
import bitvectors::intersect_postconds;
import bitvectors::declare_var;
import bitvectors::bit_num;
import bitvectors::gen_poststate;
import bitvectors::kill_poststate;
import front::ast;
import front::ast::*;
import middle::ty::expr_ann;
import middle::ty::expr_ty;
import middle::ty::type_is_nil;
import middle::ty::type_is_bot;
import util::common::new_def_hash;
import util::common::uistr;
import util::common::log_expr;
import util::common::log_block;
import util::common::log_fn;
import util::common::elt_exprs;
import util::common::field_exprs;
import util::common::has_nonlocal_exits;
import util::common::log_stmt;
import util::common::log_expr_err;

fn seq_states(&fn_ctxt fcx, prestate pres, vec[@expr] exprs) ->
   tup(bool, poststate) {
    auto changed = false;
    auto post = pres;
    for (@expr e in exprs) {
        changed = find_pre_post_state_expr(fcx, post, e) || changed;
        post = expr_poststate(fcx.ccx, e);
    }
    ret tup(changed, post);
}

fn find_pre_post_state_exprs(&fn_ctxt fcx, &prestate pres, &ann a,
                             &vec[@expr] es) -> bool {
    auto res = seq_states(fcx, pres, es);
    auto changed = res._0;
    changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
    changed = extend_poststate_ann(fcx.ccx, a, res._1) || changed;
    ret changed;
}

fn find_pre_post_state_loop(&fn_ctxt fcx, prestate pres, &@local l,
                            &@expr index, &block body, &ann a) -> bool {
    auto changed = false;
    /* same issues as while */

    // FIXME: also want to set l as initialized, no?

    changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
    changed = find_pre_post_state_expr(fcx, pres, index) || changed;
    /* in general, would need the intersection of
       (poststate of index, poststate of body) */

    changed =
        find_pre_post_state_block(fcx, expr_poststate(fcx.ccx, index), body)
            || changed;
    auto res_p =
        intersect_postconds([expr_poststate(fcx.ccx, index),
                             block_poststate(fcx.ccx, body)]);
    changed = extend_poststate_ann(fcx.ccx, a, res_p) || changed;
    ret changed;
}

fn gen_if_local(&fn_ctxt fcx, &ann a_new_var, &ann a, &path p) -> bool {
    alt (ann_to_def(fcx.ccx, a_new_var)) {
        case (some(def_local(?loc))) {
            ret gen_poststate(fcx, a,
                              rec(id=loc,
                                  c=ninit(path_to_ident(fcx.ccx.tcx, p))));
        }
        case (_) { ret false; }
    }
}

fn join_then_else(&fn_ctxt fcx, &@expr antec, &block conseq,
                  &option::t[@expr] maybe_alt, &ann a) -> bool {
    auto changed = false;

    changed =
        find_pre_post_state_block(fcx, expr_poststate(fcx.ccx, antec),
                                  conseq) || changed;
    alt (maybe_alt) {
        case (none) {
            changed =
                extend_poststate_ann(fcx.ccx, a,
                                     expr_poststate(fcx.ccx, antec))
                || changed;
        }
        case (some(?altern)) {
            changed =
                find_pre_post_state_expr(fcx,
                                         expr_poststate(fcx.ccx,
                                                        antec),
                                         altern) || changed;
            auto poststate_res =
                intersect_postconds([block_poststate(fcx.ccx, conseq),
                                     expr_poststate(fcx.ccx,
                                                    altern)]);
            changed =
                extend_poststate_ann(fcx.ccx, a, poststate_res) ||
                changed;
        }
    }
    ret changed;
}

fn find_pre_post_state_expr(&fn_ctxt fcx, &prestate pres, @expr e) -> bool {
    auto changed = false;
    auto num_local_vars = num_constraints(fcx.enclosing);

    /*  
    log_err("states:");
    log_expr_err(*e);
    aux::log_bitv_err(fcx, expr_poststate(fcx.ccx, e));
    */

    /* FIXME could get rid of some of the copy/paste */
    alt (e.node) {
        case (expr_vec(?elts, _, _, ?a)) {
            ret find_pre_post_state_exprs(fcx, pres, a, elts);
        }
        case (expr_tup(?elts, ?a)) {
            ret find_pre_post_state_exprs(fcx, pres, a, elt_exprs(elts));
        }
        case (expr_call(?operator, ?operands, ?a)) {
            /* do the prestate for the rator */

            changed =
                find_pre_post_state_expr(fcx, pres, operator) || changed;
            /* rands go left-to-right */

            changed =
                find_pre_post_state_exprs(fcx,
                                          expr_poststate(fcx.ccx, operator),
                                          a, operands) || changed;
            /* if this is a failing call, it sets everything as initialized */

            alt (controlflow_expr(fcx.ccx, operator)) {
                case (noreturn) {
                    changed =
                        set_poststate_ann(fcx.ccx, a,
                                          false_postcond(num_local_vars)) ||
                            changed;
                }
                case (_) { }
            }
            ret changed;
        }
        case (expr_spawn(_, _, ?operator, ?operands, ?a)) {
            changed = find_pre_post_state_expr(fcx, pres, operator);
            ret find_pre_post_state_exprs(fcx,
                                          expr_poststate(fcx.ccx, operator),
                                          a, operands) || changed;
        }
        case (expr_bind(?operator, ?maybe_args, ?a)) {
            changed =
                find_pre_post_state_expr(fcx, pres, operator) || changed;
            ret find_pre_post_state_exprs(fcx,
                                          expr_poststate(fcx.ccx, operator),
                                          a, cat_options[@expr](maybe_args))
                    || changed;
        }
        case (expr_path(_, ?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (expr_log(_, ?e, ?a)) {
            /* factor out the "one exp" pattern */

            changed = find_pre_post_state_expr(fcx, pres, e);
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, expr_poststate(fcx.ccx, e))
                    || changed;
            ret changed;
        }
        case (expr_chan(?e, ?a)) {
            changed = find_pre_post_state_expr(fcx, pres, e);
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, expr_poststate(fcx.ccx, e))
                    || changed;
            ret changed;
        }
        case (expr_ext(_, _, _, ?expanded, ?a)) {
            changed = find_pre_post_state_expr(fcx, pres, expanded);
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a,
                                     expr_poststate(fcx.ccx, expanded)) ||
                    changed;
            ret changed;
        }
        case (expr_put(?maybe_e, ?a)) {
            alt (maybe_e) {
                case (some(?arg)) {
                    changed = find_pre_post_state_expr(fcx, pres, arg);
                    changed =
                        extend_prestate_ann(fcx.ccx, a, pres) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, arg)) ||
                            changed;
                    ret changed;
                }
                case (none) { ret pure_exp(fcx.ccx, a, pres); }
            }
        }
        case (expr_lit(?l, ?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (
             // FIXME This was just put in here as a placeholder
             expr_fn(?f, ?a)) {
            ret pure_exp(fcx.ccx, a, pres);
        }
        case (expr_block(?b, ?a)) {
            changed = find_pre_post_state_block(fcx, pres, b) || changed;
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, block_poststate(fcx.ccx, b))
                    || changed;
            ret changed;
        }
        case (expr_rec(?fields, ?maybe_base, ?a)) {
            changed =
                find_pre_post_state_exprs(fcx, pres, a, field_exprs(fields))
                    || changed;
            alt (maybe_base) {
                case (none) {/* do nothing */ }
                case (some(?base)) {
                    changed =
                        find_pre_post_state_expr(fcx, pres, base) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, base)) ||
                            changed;
                }
            }
            ret changed;
        }
        case (expr_move(?lhs, ?rhs, ?a)) {
            // FIXME: this needs to deinitialize the rhs

            extend_prestate_ann(fcx.ccx, a, pres);
            alt (lhs.node) {
                case (expr_path(?p, ?a_lhs)) {
                    // assignment to local var

                    changed = pure_exp(fcx.ccx, a_lhs, pres) || changed;
                    changed =
                        find_pre_post_state_expr(fcx, pres, rhs) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, rhs)) ||
                            changed;
                    changed = gen_if_local(fcx, a_lhs, a, p) || changed;
                }
                case (_) {
                    // assignment to something that must already have been
                    // init'd

                    changed =
                        find_pre_post_state_expr(fcx, pres, lhs) || changed;
                    changed =
                        find_pre_post_state_expr(fcx,
                                                 expr_poststate(fcx.ccx, lhs),
                                                 rhs) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, rhs)) ||
                            changed;
                }
            }
            ret changed;
        }
        case (expr_assign(?lhs, ?rhs, ?a)) {
            extend_prestate_ann(fcx.ccx, a, pres);
            alt (lhs.node) {
                case (expr_path(?p, ?a_lhs)) {
                    // assignment to local var

                    changed = pure_exp(fcx.ccx, a_lhs, pres) || changed;
                    changed =
                        find_pre_post_state_expr(fcx, pres, rhs) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, rhs)) ||
                            changed;
                    changed = gen_if_local(fcx, a_lhs, a, p) || changed;
                }
                case (_) {
                    // assignment to something that must already have been
                    // init'd

                    changed =
                        find_pre_post_state_expr(fcx, pres, lhs) || changed;
                    changed =
                        find_pre_post_state_expr(fcx,
                                                 expr_poststate(fcx.ccx, lhs),
                                                 rhs) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, rhs)) ||
                            changed;
                }
            }
            ret changed;
        }
        case (expr_swap(?lhs, ?rhs, ?a)) {
            /* quite similar to binary -- should abstract this */
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, lhs)
                || changed;
            changed =
                find_pre_post_state_expr(fcx,
                                         expr_poststate(fcx.ccx, lhs),
                                         rhs) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a,
                                     expr_poststate(fcx.ccx, rhs)) || changed;
        ret changed;
    }
        case (expr_recv(?lhs, ?rhs, ?a)) {
            extend_prestate_ann(fcx.ccx, a, pres);
            alt (lhs.node) {
                case (expr_path(?p, ?a_lhs)) {
                    // receive to local var

                    changed = pure_exp(fcx.ccx, a_lhs, pres) || changed;
                    changed =
                        find_pre_post_state_expr(fcx, pres, rhs) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, rhs)) ||
                            changed;
                    changed = gen_if_local(fcx, a_lhs, a, p) || changed;
                }
                case (_) {
                    // receive to something that must already have been init'd

                    changed =
                        find_pre_post_state_expr(fcx, pres, lhs) || changed;
                    changed =
                        find_pre_post_state_expr(fcx,
                                                 expr_poststate(fcx.ccx, lhs),
                                                 rhs) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, rhs)) ||
                            changed;
                }
            }
            ret changed;
        }
        case (expr_ret(?maybe_ret_val, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            /* normally, everything is true if execution continues after
               a ret expression (since execution never continues locally
               after a ret expression */

            set_poststate_ann(fcx.ccx, a, false_postcond(num_local_vars));
            /* return from an always-failing function clears the return bit */

            alt (fcx.enclosing.cf) {
                case (noreturn) {
                    kill_poststate(fcx, a, rec(id=fcx.id, c=ninit(fcx.name)));
                }
                case (_) { }
            }
            alt (maybe_ret_val) {
                case (none) {/* do nothing */ }
                case (some(?ret_val)) {
                    changed =
                        find_pre_post_state_expr(fcx, pres, ret_val) ||
                            changed;
                }
            }
            ret changed;
        }
        case (expr_be(?e, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            set_poststate_ann(fcx.ccx, a, false_postcond(num_local_vars));
            changed = find_pre_post_state_expr(fcx, pres, e) || changed;
            ret changed;
        }
        case (expr_if(?antec, ?conseq, ?maybe_alt, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, antec) || changed;
            changed = join_then_else(fcx, antec, conseq, maybe_alt, a)
                || changed;

            ret changed;
        }
        case (expr_binary(?bop, ?l, ?r, ?a)) {
            /* FIXME: what if bop is lazy? */

            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, l) || changed;
            changed =
                find_pre_post_state_expr(fcx, expr_poststate(fcx.ccx, l), r)
                    || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, expr_poststate(fcx.ccx, r))
                    || changed;
            ret changed;
        }
        case (expr_send(?l, ?r, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, l) || changed;
            changed =
                find_pre_post_state_expr(fcx, expr_poststate(fcx.ccx, l), r)
                    || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, expr_poststate(fcx.ccx, r))
                    || changed;
            ret changed;
        }
        case (expr_assign_op(?op, ?lhs, ?rhs, ?a)) {
            /* quite similar to binary -- should abstract this */

            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, lhs) || changed;
            changed =
                find_pre_post_state_expr(fcx, expr_poststate(fcx.ccx, lhs),
                                         rhs) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, expr_poststate(fcx.ccx, rhs))
                    || changed;
            ret changed;
        }
        case (expr_while(?test, ?body, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            /* to handle general predicates, we need to pass in
                pres `intersect` (poststate(a)) 
             like: auto test_pres = intersect_postconds(pres,
             expr_postcond(a)); However, this doesn't work right now because
             we would be passing in an all-zero prestate initially
               FIXME
               maybe need a "don't know" state in addition to 0 or 1?
            */

            changed = find_pre_post_state_expr(fcx, pres, test) || changed;
            changed =
                find_pre_post_state_block(fcx, expr_poststate(fcx.ccx, test),
                                          body) || changed;
            changed =
                {
                    auto e_post = expr_poststate(fcx.ccx, test);
                    auto b_post = block_poststate(fcx.ccx, body);
                    extend_poststate_ann(fcx.ccx, a,
                                         intersect_postconds([e_post,
                                                              b_post]))
                    || changed
                };
            ret changed;
        }
        case (expr_do_while(?body, ?test, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_block(fcx, pres, body) || changed;
            changed =
                find_pre_post_state_expr(fcx, block_poststate(fcx.ccx, body),
                                         test) || changed;
            /* conservative approximination: if the body of the loop
               could break or cont, we revert to the prestate
               (TODO: could treat cont differently from break, since
               if there's a cont, the test will execute) */

            if (has_nonlocal_exits(body)) {
                changed = set_poststate_ann(fcx.ccx, a, pres) || changed;
            } else {
                changed =
                    extend_poststate_ann(fcx.ccx, a,
                                         expr_poststate(fcx.ccx, test)) ||
                        changed;
            }
            ret changed;
        }
        case (expr_for(?d, ?index, ?body, ?a)) {
            ret find_pre_post_state_loop(fcx, pres, d, index, body, a);
        }
        case (expr_for_each(?d, ?index, ?body, ?a)) {
            ret find_pre_post_state_loop(fcx, pres, d, index, body, a);
        }
        case (expr_index(?e, ?sub, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, e) || changed;
            changed =
                find_pre_post_state_expr(fcx, expr_poststate(fcx.ccx, e), sub)
                    || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a,
                                     expr_poststate(fcx.ccx, sub));
            ret changed;
        }
        case (expr_alt(?e, ?alts, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, e) || changed;
            auto e_post = expr_poststate(fcx.ccx, e);
            auto a_post;
            if (vec::len[arm](alts) > 0u) {
                a_post = false_postcond(num_local_vars);
                for (arm an_alt in alts) {
                    changed =
                        find_pre_post_state_block(fcx, e_post, an_alt.block)
                            || changed;
                    intersect(a_post, block_poststate(fcx.ccx, an_alt.block));
                    // We deliberately do *not* update changed here, because
                    // we'd go into an infinite loop that way, and the change
                    // gets made after the if expression.

                }
            } else {
                // No alts; poststate is the poststate of the test

                a_post = e_post;
            }
            changed = extend_poststate_ann(fcx.ccx, a, a_post) || changed;
            ret changed;
        }
        case (expr_field(?e, _, ?a)) {
            changed = find_pre_post_state_expr(fcx, pres, e);
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a, expr_poststate(fcx.ccx, e))
                    || changed;
            ret changed;
        }
        case (expr_unary(_, ?operand, ?a)) {
            changed = find_pre_post_state_expr(fcx, pres, operand) || changed;
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a,
                                     expr_poststate(fcx.ccx, operand)) ||
                    changed;
            ret changed;
        }
        case (expr_cast(?operand, _, ?a)) {
            changed = find_pre_post_state_expr(fcx, pres, operand) || changed;
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed =
                extend_poststate_ann(fcx.ccx, a,
                                     expr_poststate(fcx.ccx, operand)) ||
                    changed;
            ret changed;
        }
        case (expr_fail(?a, _)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            /* if execution continues after fail, then everything is true!
               woo! */

            changed =
                set_poststate_ann(fcx.ccx, a, false_postcond(num_local_vars))
                    || changed;
            ret changed;
        }
        case (expr_assert(?p, ?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (expr_check(?p, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, p) || changed;
            changed = extend_poststate_ann(fcx.ccx, a, pres) || changed;
            /* predicate p holds after this expression executes */

            let aux::constr c = expr_to_constr(fcx.ccx.tcx, p);
            changed = gen_poststate(fcx, a, c.node) || changed;
            ret changed;
        }
        case (expr_if_check(?p, ?conseq, ?maybe_alt, ?a)) {
            changed = extend_prestate_ann(fcx.ccx, a, pres) || changed;
            changed = find_pre_post_state_expr(fcx, pres, p) || changed;
            let aux::constr c = expr_to_constr(fcx.ccx.tcx, p);
            changed = gen_poststate(fcx, expr_ann(p), c.node) || changed;
            
            changed = join_then_else(fcx, p, conseq, maybe_alt, a)
                || changed;
            ret changed;
        }
        case (expr_break(?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (expr_cont(?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (expr_port(?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (expr_self_method(_, ?a)) { ret pure_exp(fcx.ccx, a, pres); }
        case (expr_anon_obj(?anon_obj, _, _, ?a)) {
            alt (anon_obj.with_obj) {
                case (some(?e)) {
                    changed = find_pre_post_state_expr(fcx, pres, e);
                    changed =
                        extend_prestate_ann(fcx.ccx, a, pres) || changed;
                    changed =
                        extend_poststate_ann(fcx.ccx, a,
                                             expr_poststate(fcx.ccx, e)) ||
                            changed;
                    ret changed;
                }
                case (none) { ret pure_exp(fcx.ccx, a, pres); }
            }
        }
    }
}

fn find_pre_post_state_stmt(&fn_ctxt fcx, &prestate pres, @stmt s) -> bool {
    auto changed = false;
    auto stmt_ann = stmt_to_ann(fcx.ccx, *s);
    log "*At beginning: stmt = ";
    log_stmt(*s);
    log "*prestate = ";
    log bitv::to_str(stmt_ann.states.prestate);
    log "*poststate =";
    log bitv::to_str(stmt_ann.states.poststate);
    log "*changed =";
    log changed;
    alt (s.node) {
        case (stmt_decl(?adecl, ?a)) {
            alt (adecl.node) {
                case (decl_local(?alocal)) {
                    alt (alocal.init) {
                        case (some(?an_init)) {
                            changed =
                                extend_prestate(stmt_ann.states.prestate,
                                                pres) || changed;
                            changed =
                                find_pre_post_state_expr(fcx, pres,
                                                         an_init.expr) ||
                                    changed;
                            changed =
                                extend_poststate(stmt_ann.states.poststate,
                                                 expr_poststate(fcx.ccx,
                                                                an_init.expr))
                                    || changed;
                            changed =
                                gen_poststate(fcx, a,
                                              rec(id=alocal.id,
                                                  c=ninit(alocal.ident))) ||
                                    changed;
                            log "Summary: stmt = ";
                            log_stmt(*s);
                            log "prestate = ";
                            log bitv::to_str(stmt_ann.states.prestate);
                            log_bitv(fcx, stmt_ann.states.prestate);
                            log "poststate =";
                            log_bitv(fcx, stmt_ann.states.poststate);
                            log "changed =";
                            log changed;
                            ret changed;
                        }
                        case (none) {
                            changed =
                                extend_prestate(stmt_ann.states.prestate,
                                                pres) || changed;
                            changed =
                                extend_poststate(stmt_ann.states.poststate,
                                                 pres) || changed;
                            ret changed;
                        }
                    }
                }
                case (decl_item(?an_item)) {
                    changed =
                        extend_prestate(stmt_ann.states.prestate, pres) ||
                            changed;
                    changed =
                        extend_poststate(stmt_ann.states.poststate, pres) ||
                            changed;
                    /* the outer "walk" will recurse into the item */

                    ret changed;
                }
            }
        }
        case (stmt_expr(?e, _)) {
            changed = find_pre_post_state_expr(fcx, pres, e) || changed;
            changed =
                extend_prestate(stmt_ann.states.prestate,
                                expr_prestate(fcx.ccx, e)) || changed;
            changed =
                extend_poststate(stmt_ann.states.poststate,
                                 expr_poststate(fcx.ccx, e)) || changed;
            /*
              log("Summary: stmt = ");
              log_stmt(*s);
              log("prestate = ");
              log(bitv::to_str(stmt_ann.states.prestate));
              log_bitv(enclosing, stmt_ann.states.prestate);
              log("poststate =");
              log(bitv::to_str(stmt_ann.states.poststate));
              log_bitv(enclosing, stmt_ann.states.poststate);
              log("changed =");
              log(changed);
            */

            ret changed;
        }
        case (_) { ret false; }
    }
}


/* Updates the pre- and post-states of statements in the block,
   returns a boolean flag saying whether any pre- or poststates changed */
fn find_pre_post_state_block(&fn_ctxt fcx, &prestate pres0, &block b) ->
   bool {
    auto changed = false;
    auto num_local_vars = num_constraints(fcx.enclosing);
    /* First, set the pre-states and post-states for every expression */

    auto pres = pres0;
    /* Iterate over each stmt. The new prestate is <pres>. The poststate
     consist of improving <pres> with whatever variables this stmt
     initializes.  Then <pres> becomes the new poststate. */

    for (@stmt s in b.node.stmts) {
        changed = find_pre_post_state_stmt(fcx, pres, s) || changed;
        pres = stmt_poststate(fcx.ccx, *s);
    }
    auto post = pres;
    alt (b.node.expr) {
        case (none) { }
        case (some(?e)) {
            changed = find_pre_post_state_expr(fcx, pres, e) || changed;
            post = expr_poststate(fcx.ccx, e);
        }
    }
    /*
    log_err("block:");
    log_block_err(b);
    log_err("has non-local exits?");
    log_err(has_nonlocal_exits(b));
    */

    /* conservative approximation: if a block contains a break
       or cont, we assume nothing about the poststate */

    if (has_nonlocal_exits(b)) { post = pres0; }
    set_prestate_ann(fcx.ccx, b.node.a, pres0);
    set_poststate_ann(fcx.ccx, b.node.a, post);
    log "For block:";
    log_block(b);
    log "poststate = ";
    log_states(block_states(fcx.ccx, b));
    log "pres0:";
    log_bitv(fcx, pres0);
    log "post:";
    log_bitv(fcx, post);
    ret changed;
}

fn find_pre_post_state_fn(&fn_ctxt fcx, &_fn f) -> bool {
    auto num_local_vars = num_constraints(fcx.enclosing);
    auto changed =
        find_pre_post_state_block(fcx, empty_prestate(num_local_vars),
                                  f.body);
    // Treat the tail expression as a return statement

    alt (f.body.node.expr) {
        case (some(?tailexpr)) {
            auto tailann = expr_ann(tailexpr);
            auto tailty = expr_ty(fcx.ccx.tcx, tailexpr);

            // Since blocks and alts and ifs that don't have results
            // implicitly result in nil, we have to be careful to not
            // interpret nil-typed block results as the result of a
            // function with some other return type
            if (!type_is_nil(fcx.ccx.tcx, tailty) &&
                    !type_is_bot(fcx.ccx.tcx, tailty)) {
                set_poststate_ann(fcx.ccx, tailann,
                                  false_postcond(num_local_vars));
                alt (fcx.enclosing.cf) {
                    case (noreturn) {
                        kill_poststate(fcx, tailann,
                                       rec(id=fcx.id, c=ninit(fcx.name)));
                    }
                    case (_) { }
                }
            }
        }
        case (none) {/* fallthrough */ }
    }
    ret changed;
}
//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
