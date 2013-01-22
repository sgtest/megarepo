// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
# Borrow check

This pass is in job of enforcing *memory safety* and *purity*.  As
memory safety is by far the more complex topic, I'll focus on that in
this description, but purity will be covered later on. In the context
of Rust, memory safety means three basic things:

- no writes to immutable memory;
- all pointers point to non-freed memory;
- all pointers point to memory of the same type as the pointer.

The last point might seem confusing: after all, for the most part,
this condition is guaranteed by the type check.  However, there are
two cases where the type check effectively delegates to borrow check.

The first case has to do with enums.  If there is a pointer to the
interior of an enum, and the enum is in a mutable location (such as a
local variable or field declared to be mutable), it is possible that
the user will overwrite the enum with a new value of a different
variant, and thus effectively change the type of the memory that the
pointer is pointing at.

The second case has to do with mutability.  Basically, the type
checker has only a limited understanding of mutability.  It will allow
(for example) the user to get an immutable pointer with the address of
a mutable local variable.  It will also allow a `@mut T` or `~mut T`
pointer to be borrowed as a `&r.T` pointer.  These seeming oversights
are in fact intentional; they allow the user to temporarily treat a
mutable value as immutable.  It is up to the borrow check to guarantee
that the value in question is not in fact mutated during the lifetime
`r` of the reference.

# Definition of unstable memory

The primary danger to safety arises due to *unstable memory*.
Unstable memory is memory whose validity or type may change as a
result of an assignment, move, or a variable going out of scope.
There are two cases in Rust where memory is unstable: the contents of
unique boxes and enums.

Unique boxes are unstable because when the variable containing the
unique box is re-assigned, moves, or goes out of scope, the unique box
is freed or---in the case of a move---potentially given to another
task.  In either case, if there is an extant and usable pointer into
the box, then safety guarantees would be compromised.

Enum values are unstable because they are reassigned the types of
their contents may change if they are assigned with a different
variant than they had previously.

# Safety criteria that must be enforced

Whenever a piece of memory is borrowed for lifetime L, there are two
things which the borrow checker must guarantee.  First, it must
guarantee that the memory address will remain allocated (and owned by
the current task) for the entirety of the lifetime L.  Second, it must
guarantee that the type of the data will not change for the entirety
of the lifetime L.  In exchange, the region-based type system will
guarantee that the pointer is not used outside the lifetime L.  These
guarantees are to some extent independent but are also inter-related.

In some cases, the type of a pointer cannot be invalidated but the
lifetime can.  For example, imagine a pointer to the interior of
a shared box like:

    let mut x = @mut {f: 5, g: 6};
    let y = &mut x.f;

Here, a pointer was created to the interior of a shared box which
contains a record.  Even if `*x` were to be mutated like so:

    *x = {f: 6, g: 7};

This would cause `*y` to change from 5 to 6, but the pointer pointer
`y` remains valid.  It still points at an integer even if that integer
has been overwritten.

However, if we were to reassign `x` itself, like so:

    x = @{f: 6, g: 7};

This could potentially invalidate `y`, because if `x` were the final
reference to the shared box, then that memory would be released and
now `y` points at freed memory.  (We will see that to prevent this
scenario we will *root* shared boxes that reside in mutable memory
whose contents are borrowed; rooting means that we create a temporary
to ensure that the box is not collected).

In other cases, like an enum on the stack, the memory cannot be freed
but its type can change:

    let mut x = Some(5);
    match x {
      Some(ref y) => { ... }
      None => { ... }
    }

Here as before, the pointer `y` would be invalidated if we were to
reassign `x` to `none`.  (We will see that this case is prevented
because borrowck tracks data which resides on the stack and prevents
variables from reassigned if there may be pointers to their interior)

Finally, in some cases, both dangers can arise.  For example, something
like the following:

    let mut x = ~some(5);
    match x {
      ~some(ref y) => { ... }
      ~none => { ... }
    }

In this case, if `x` to be reassigned or `*x` were to be mutated, then
the pointer `y` would be invalided.  (This case is also prevented by
borrowck tracking data which is owned by the current stack frame)

# Summary of the safety check

In order to enforce mutability, the borrow check has a few tricks up
its sleeve:

- When data is owned by the current stack frame, we can identify every
  possible assignment to a local variable and simply prevent
  potentially dangerous assignments directly.

- If data is owned by a shared box, we can root the box to increase
  its lifetime.

- If data is found within a borrowed pointer, we can assume that the
  data will remain live for the entirety of the borrowed pointer.

- We can rely on the fact that pure actions (such as calling pure
  functions) do not mutate data which is not owned by the current
  stack frame.

# Possible future directions

There are numerous ways that the `borrowck` could be strengthened, but
these are the two most likely:

- flow-sensitivity: we do not currently consider flow at all but only
  block-scoping.  This means that innocent code like the following is
  rejected:

      let mut x: int;
      ...
      x = 5;
      let y: &int = &x; // immutable ptr created
      ...

  The reason is that the scope of the pointer `y` is the entire
  enclosing block, and the assignment `x = 5` occurs within that
  block.  The analysis is not smart enough to see that `x = 5` always
  happens before the immutable pointer is created.  This is relatively
  easy to fix and will surely be fixed at some point.

- finer-grained purity checks: currently, our fallback for
  guaranteeing random references into mutable, aliasable memory is to
  require *total purity*.  This is rather strong.  We could use local
  type-based alias analysis to distinguish writes that could not
  possibly invalid the references which must be guaranteed.  This
  would only work within the function boundaries; function calls would
  still require total purity.  This seems less likely to be
  implemented in the short term as it would make the code
  significantly more complex; there is currently no code to analyze
  the types and determine the possible impacts of a write.

# How the code works

The borrow check code is divided into several major modules, each of
which is documented in its own file.

The `gather_loans` and `check_loans` are the two major passes of the
analysis.  The `gather_loans` pass runs over the IR once to determine
what memory must remain valid and for how long.  Its name is a bit of
a misnomer; it does in fact gather up the set of loans which are
granted, but it also determines when @T pointers must be rooted and
for which scopes purity must be required.

The `check_loans` pass walks the IR and examines the loans and purity
requirements computed in `gather_loans`.  It checks to ensure that (a)
the conditions of all loans are honored; (b) no contradictory loans
were granted (for example, loaning out the same memory as mutable and
immutable simultaneously); and (c) any purity requirements are
honored.

The remaining modules are helper modules used by `gather_loans` and
`check_loans`:

- `categorization` has the job of analyzing an expression to determine
  what kind of memory is used in evaluating it (for example, where
  dereferences occur and what kind of pointer is dereferenced; whether
  the memory is mutable; etc)
- `loan` determines when data uniquely tied to the stack frame can be
  loaned out.
- `preserve` determines what actions (if any) must be taken to preserve
  aliasable data.  This is the code which decides when to root
  an @T pointer or to require purity.

# Maps that are created

Borrowck results in two maps.

- `root_map`: identifies those expressions or patterns whose result
  needs to be rooted.  Conceptually the root_map maps from an
  expression or pattern node to a `node_id` identifying the scope for
  which the expression must be rooted (this `node_id` should identify
  a block or call).  The actual key to the map is not an expression id,
  however, but a `root_map_key`, which combines an expression id with a
  deref count and is used to cope with auto-deref.

- `mutbl_map`: identifies those local variables which are modified or
  moved. This is used by trans to guarantee that such variables are
  given a memory location and not used as immediates.
 */

#[legacy_exports];

use core::prelude::*;

use middle::liveness;
use middle::mem_categorization::*;
use middle::region;
use middle::ty;
use util::common::{indenter, stmt_set};
use util::ppaux::{expr_repr, note_and_explain_region};
use util::ppaux::{ty_to_str, region_to_str, explain_region};

use core::cmp;
use core::dvec::DVec;
use core::io;
use core::result::{Result, Ok, Err};
use std::list::{List, Cons, Nil};
use std::list;
use std::map::{HashMap, Set};
use syntax::ast::{mutability, m_mutbl, m_imm, m_const};
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util;
use syntax::codemap::span;
use syntax::print::pprust;
use syntax::visit;

#[legacy_exports]
pub mod check_loans;
#[legacy_exports]
pub mod gather_loans;
#[legacy_exports]
pub mod loan;
#[legacy_exports]
pub mod preserve;

fn check_crate(tcx: ty::ctxt,
               method_map: typeck::method_map,
               last_use_map: liveness::last_use_map,
               crate: @ast::crate)
            -> (root_map, mutbl_map, write_guard_map) {

    let bccx = borrowck_ctxt_(@{tcx: tcx,
                                method_map: method_map,
                                last_use_map: last_use_map,
                                root_map: root_map(),
                                mutbl_map: HashMap(),
                                write_guard_map: HashMap(),
                                stmt_map: HashMap(),
                                mut loaned_paths_same: 0,
                                mut loaned_paths_imm: 0,
                                mut stable_paths: 0,
                                mut req_pure_paths: 0,
                                mut guaranteed_paths: 0});

    let req_maps = gather_loans::gather_loans(bccx, crate);
    check_loans::check_loans(bccx, req_maps, crate);

    if tcx.sess.borrowck_stats() {
        io::println(~"--- borrowck stats ---");
        io::println(fmt!("paths requiring guarantees: %u",
                        bccx.guaranteed_paths));
        io::println(fmt!("paths requiring loans     : %s",
                         make_stat(bccx, bccx.loaned_paths_same)));
        io::println(fmt!("paths requiring imm loans : %s",
                         make_stat(bccx, bccx.loaned_paths_imm)));
        io::println(fmt!("stable paths              : %s",
                         make_stat(bccx, bccx.stable_paths)));
        io::println(fmt!("paths requiring purity    : %s",
                         make_stat(bccx, bccx.req_pure_paths)));
    }

    return (bccx.root_map, bccx.mutbl_map, bccx.write_guard_map);

    fn make_stat(bccx: borrowck_ctxt, stat: uint) -> ~str {
        let stat_f = stat as float;
        let total = bccx.guaranteed_paths as float;
        fmt!("%u (%.0f%%)", stat  , stat_f * 100f / total)
    }
}

// ----------------------------------------------------------------------
// Type definitions

type borrowck_ctxt_ = {tcx: ty::ctxt,
                       method_map: typeck::method_map,
                       last_use_map: liveness::last_use_map,
                       root_map: root_map,
                       mutbl_map: mutbl_map,
                       write_guard_map: write_guard_map,
                       stmt_map: stmt_set,

                       // Statistics:
                       mut loaned_paths_same: uint,
                       mut loaned_paths_imm: uint,
                       mut stable_paths: uint,
                       mut req_pure_paths: uint,
                       mut guaranteed_paths: uint};

enum borrowck_ctxt {
    borrowck_ctxt_(@borrowck_ctxt_)
}

struct RootInfo {
    scope: ast::node_id,
    // This will be true if we need to freeze this box at runtime. This will
    // result in a call to `borrow_as_imm()` and `return_to_mut()`.
    freezes: bool   // True if we need to freeze this box at runtime.
}

// a map mapping id's of expressions of gc'd type (@T, @[], etc) where
// the box needs to be kept live to the id of the scope for which they
// must stay live.
type root_map = HashMap<root_map_key, RootInfo>;

// the keys to the root map combine the `id` of the expression with
// the number of types that it is autodereferenced.  So, for example,
// if you have an expression `x.f` and x has type ~@T, we could add an
// entry {id:x, derefs:0} to refer to `x` itself, `{id:x, derefs:1}`
// to refer to the deref of the unique pointer, and so on.
type root_map_key = {id: ast::node_id, derefs: uint};

// set of ids of local vars / formal arguments that are modified / moved.
// this is used in trans for optimization purposes.
type mutbl_map = HashMap<ast::node_id, ()>;

// A set containing IDs of expressions of gc'd type that need to have a write
// guard.
type write_guard_map = HashMap<root_map_key, ()>;

// Errors that can occur"]
enum bckerr_code {
    err_mut_uniq,
    err_mut_variant,
    err_root_not_permitted,
    err_mutbl(ast::mutability),
    err_out_of_root_scope(ty::Region, ty::Region), // superscope, subscope
    err_out_of_scope(ty::Region, ty::Region) // superscope, subscope
}

impl bckerr_code : cmp::Eq {
    pure fn eq(&self, other: &bckerr_code) -> bool {
        match (*self) {
            err_mut_uniq => {
                match (*other) {
                    err_mut_uniq => true,
                    _ => false
                }
            }
            err_mut_variant => {
                match (*other) {
                    err_mut_variant => true,
                    _ => false
                }
            }
            err_root_not_permitted => {
                match (*other) {
                    err_root_not_permitted => true,
                    _ => false
                }
            }
            err_mutbl(e0a) => {
                match (*other) {
                    err_mutbl(e0b) => e0a == e0b,
                    _ => false
                }
            }
            err_out_of_root_scope(e0a, e1a) => {
                match (*other) {
                    err_out_of_root_scope(e0b, e1b) =>
                        e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
            err_out_of_scope(e0a, e1a) => {
                match (*other) {
                    err_out_of_scope(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
        }
    }
    pure fn ne(&self, other: &bckerr_code) -> bool { !(*self).eq(other) }
}

// Combination of an error code and the categorization of the expression
// that caused it
type bckerr = {cmt: cmt, code: bckerr_code};

impl bckerr : cmp::Eq {
    pure fn eq(&self, other: &bckerr) -> bool {
        (*self).cmt == (*other).cmt && (*self).code == (*other).code
    }
    pure fn ne(&self, other: &bckerr) -> bool { !(*self).eq(other) }
}

// shorthand for something that fails with `bckerr` or succeeds with `T`
type bckres<T> = Result<T, bckerr>;

/// a complete record of a loan that was granted
pub struct Loan {lp: @loan_path, cmt: cmt, mutbl: ast::mutability}

/// maps computed by `gather_loans` that are then used by `check_loans`
///
/// - `req_loan_map`: map from each block/expr to the required loans needed
///   for the duration of that block/expr
/// - `pure_map`: map from block/expr that must be pure to the error message
///   that should be reported if they are not pure
pub type req_maps = {
    req_loan_map: HashMap<ast::node_id, @DVec<Loan>>,
    pure_map: HashMap<ast::node_id, bckerr>
};

fn save_and_restore<T:Copy,U>(save_and_restore_t: &mut T, f: fn() -> U) -> U {
    let old_save_and_restore_t = *save_and_restore_t;
    let u = f();
    *save_and_restore_t = old_save_and_restore_t;
    move u
}

/// Creates and returns a new root_map

impl root_map_key : cmp::Eq {
    pure fn eq(&self, other: &root_map_key) -> bool {
        (*self).id == (*other).id && (*self).derefs == (*other).derefs
    }
    pure fn ne(&self, other: &root_map_key) -> bool {
        ! ((*self) == (*other))
    }
}

impl root_map_key : to_bytes::IterBytes {
    pure fn iter_bytes(&self, +lsb0: bool, f: to_bytes::Cb) {
        to_bytes::iter_bytes_2(&self.id, &self.derefs, lsb0, f);
    }
}

fn root_map() -> root_map {
    return HashMap();
}

// ___________________________________________________________________________
// Misc

impl borrowck_ctxt {
    fn is_subregion_of(r_sub: ty::Region, r_sup: ty::Region) -> bool {
        region::is_subregion_of(self.tcx.region_map, r_sub, r_sup)
    }

    fn cat_expr(expr: @ast::expr) -> cmt {
        cat_expr(self.tcx, self.method_map, expr)
    }

    fn cat_expr_unadjusted(expr: @ast::expr) -> cmt {
        cat_expr_unadjusted(self.tcx, self.method_map, expr)
    }

    fn cat_expr_autoderefd(expr: @ast::expr,
                           adj: @ty::AutoAdjustment)
                        -> cmt {
        cat_expr_autoderefd(self.tcx, self.method_map, expr, adj)
    }

    fn cat_def(id: ast::node_id,
               span: span,
               ty: ty::t,
               def: ast::def) -> cmt {
        cat_def(self.tcx, self.method_map, id, span, ty, def)
    }

    fn cat_variant<N: ast_node>(arg: N,
                                enum_did: ast::def_id,
                                cmt: cmt) -> cmt {
        cat_variant(self.tcx, self.method_map, arg, enum_did, cmt)
    }

    fn cat_discr(cmt: cmt, match_id: ast::node_id) -> cmt {
        return @{cat:cat_discr(cmt, match_id),.. *cmt};
    }

    fn cat_pattern(cmt: cmt, pat: @ast::pat, op: fn(cmt, @ast::pat)) {
        let mc = &mem_categorization_ctxt {tcx: self.tcx,
                                           method_map: self.method_map};
        mc.cat_pattern(cmt, pat, op);
    }

    fn report_if_err(bres: bckres<()>) {
        match bres {
          Ok(()) => (),
          Err(ref e) => self.report((*e))
        }
    }

    fn report(err: bckerr) {
        self.span_err(
            err.cmt.span,
            fmt!("illegal borrow: %s",
                 self.bckerr_to_str(err)));
        self.note_and_explain_bckerr(err);
    }

    fn span_err(s: span, +m: ~str) {
        self.tcx.sess.span_err(s, m);
    }

    fn span_note(s: span, +m: ~str) {
        self.tcx.sess.span_note(s, m);
    }

    fn add_to_mutbl_map(cmt: cmt) {
        match cmt.cat {
          cat_local(id) | cat_arg(id) => {
            self.mutbl_map.insert(id, ());
          }
          cat_stack_upvar(cmt) => {
            self.add_to_mutbl_map(cmt);
          }
          _ => ()
        }
    }

    fn bckerr_to_str(err: bckerr) -> ~str {
        match err.code {
            err_mutbl(req) => {
                fmt!("creating %s alias to %s",
                     self.mut_to_str(req),
                     self.cmt_to_str(err.cmt))
            }
            err_mut_uniq => {
                ~"unique value in aliasable, mutable location"
            }
            err_mut_variant => {
                ~"enum variant in aliasable, mutable location"
            }
            err_root_not_permitted => {
                // note: I don't expect users to ever see this error
                // message, reasons are discussed in attempt_root() in
                // preserve.rs.
                ~"rooting is not permitted"
            }
            err_out_of_root_scope(*) => {
                ~"cannot root managed value long enough"
            }
            err_out_of_scope(*) => {
                ~"borrowed value does not live long enough"
            }
        }
    }

    fn note_and_explain_bckerr(err: bckerr) {
        let code = err.code;
        match code {
            err_mutbl(*) | err_mut_uniq | err_mut_variant |
            err_root_not_permitted => {}

            err_out_of_root_scope(super_scope, sub_scope) => {
                note_and_explain_region(
                    self.tcx,
                    ~"managed value would have to be rooted for ",
                    sub_scope,
                    ~"...");
                note_and_explain_region(
                    self.tcx,
                    ~"...but can only be rooted for ",
                    super_scope,
                    ~"");
            }

            err_out_of_scope(super_scope, sub_scope) => {
                note_and_explain_region(
                    self.tcx,
                    ~"borrowed pointer must be valid for ",
                    sub_scope,
                    ~"...");
                note_and_explain_region(
                    self.tcx,
                    ~"...but borrowed value is only valid for ",
                    super_scope,
                    ~"");
          }
        }
    }


    fn cmt_to_str(cmt: cmt) -> ~str {
        let mc = &mem_categorization_ctxt {tcx: self.tcx,
                                           method_map: self.method_map};
        mc.cmt_to_str(cmt)
    }

    fn cmt_to_repr(cmt: cmt) -> ~str {
        let mc = &mem_categorization_ctxt {tcx: self.tcx,
                                           method_map: self.method_map};
        mc.cmt_to_repr(cmt)
    }

    fn mut_to_str(mutbl: ast::mutability) -> ~str {
        let mc = &mem_categorization_ctxt {tcx: self.tcx,
                                           method_map: self.method_map};
        mc.mut_to_str(mutbl)
    }

    fn loan_to_repr(loan: &Loan) -> ~str {
        fmt!("Loan(lp=%?, cmt=%s, mutbl=%?)",
             loan.lp, self.cmt_to_repr(loan.cmt), loan.mutbl)
    }
}

// The inherent mutability of a component is its default mutability
// assuming it is embedded in an immutable context.  In general, the
// mutability can be "overridden" if the component is embedded in a
// mutable structure.
fn inherent_mutability(ck: comp_kind) -> mutability {
    match ck {
      comp_tuple | comp_anon_field | comp_variant(_) => m_imm,
      comp_field(_, m) | comp_index(_, m)            => m
    }
}
