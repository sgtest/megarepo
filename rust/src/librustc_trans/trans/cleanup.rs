// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Code pertaining to cleanup of temporaries as well as execution of
//! drop glue. See discussion in `doc.rs` for a high-level summary.

pub use self::ScopeId::*;
pub use self::CleanupScopeKind::*;
pub use self::EarlyExitLabel::*;
pub use self::Heap::*;

use llvm::{BasicBlockRef, ValueRef};
use trans::base;
use trans::build;
use trans::callee;
use trans::common;
use trans::common::{Block, FunctionContext, ExprId, NodeIdAndSpan};
use trans::debuginfo::{DebugLoc, ToDebugLoc};
use trans::glue;
use middle::region;
use trans::type_::Type;
use middle::ty::{self, Ty};
use std::fmt;
use syntax::ast;
use util::ppaux::Repr;

pub struct CleanupScope<'blk, 'tcx: 'blk> {
    // The id of this cleanup scope. If the id is None,
    // this is a *temporary scope* that is pushed during trans to
    // cleanup miscellaneous garbage that trans may generate whose
    // lifetime is a subset of some expression.  See module doc for
    // more details.
    kind: CleanupScopeKind<'blk, 'tcx>,

    // Cleanups to run upon scope exit.
    cleanups: Vec<CleanupObj<'tcx>>,

    // The debug location any drop calls generated for this scope will be
    // associated with.
    debug_loc: DebugLoc,

    cached_early_exits: Vec<CachedEarlyExit>,
    cached_landing_pad: Option<BasicBlockRef>,
}

#[derive(Copy, Debug)]
pub struct CustomScopeIndex {
    index: uint
}

pub const EXIT_BREAK: uint = 0;
pub const EXIT_LOOP: uint = 1;
pub const EXIT_MAX: uint = 2;

pub enum CleanupScopeKind<'blk, 'tcx: 'blk> {
    CustomScopeKind,
    AstScopeKind(ast::NodeId),
    LoopScopeKind(ast::NodeId, [Block<'blk, 'tcx>; EXIT_MAX])
}

impl<'blk, 'tcx: 'blk> fmt::Debug for CleanupScopeKind<'blk, 'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CustomScopeKind => write!(f, "CustomScopeKind"),
            AstScopeKind(nid) => write!(f, "AstScopeKind({})", nid),
            LoopScopeKind(nid, ref blks) => {
                try!(write!(f, "LoopScopeKind({}, [", nid));
                for blk in blks.iter() {
                    try!(write!(f, "{:p}, ", blk));
                }
                write!(f, "])")
            }
        }
    }
}

#[derive(Copy, PartialEq, Debug)]
pub enum EarlyExitLabel {
    UnwindExit,
    ReturnExit,
    LoopExit(ast::NodeId, uint)
}

#[derive(Copy)]
pub struct CachedEarlyExit {
    label: EarlyExitLabel,
    cleanup_block: BasicBlockRef,
}

pub trait Cleanup<'tcx> {
    fn must_unwind(&self) -> bool;
    fn clean_on_unwind(&self) -> bool;
    fn is_lifetime_end(&self) -> bool;
    fn trans<'blk>(&self,
                   bcx: Block<'blk, 'tcx>,
                   debug_loc: DebugLoc)
                   -> Block<'blk, 'tcx>;
}

pub type CleanupObj<'tcx> = Box<Cleanup<'tcx>+'tcx>;

#[derive(Copy, Debug)]
pub enum ScopeId {
    AstScope(ast::NodeId),
    CustomScope(CustomScopeIndex)
}

impl<'blk, 'tcx> CleanupMethods<'blk, 'tcx> for FunctionContext<'blk, 'tcx> {
    /// Invoked when we start to trans the code contained within a new cleanup scope.
    fn push_ast_cleanup_scope(&self, debug_loc: NodeIdAndSpan) {
        debug!("push_ast_cleanup_scope({})",
               self.ccx.tcx().map.node_to_string(debug_loc.id));

        // FIXME(#2202) -- currently closure bodies have a parent
        // region, which messes up the assertion below, since there
        // are no cleanup scopes on the stack at the start of
        // trans'ing a closure body.  I think though that this should
        // eventually be fixed by closure bodies not having a parent
        // region, though that's a touch unclear, and it might also be
        // better just to narrow this assertion more (i.e., by
        // excluding id's that correspond to closure bodies only). For
        // now we just say that if there is already an AST scope on the stack,
        // this new AST scope had better be its immediate child.
        let top_scope = self.top_ast_scope();
        if top_scope.is_some() {
            assert_eq!(self.ccx
                           .tcx()
                           .region_maps
                           .opt_encl_scope(region::CodeExtent::from_node_id(debug_loc.id))
                           .map(|s|s.node_id()),
                       top_scope);
        }

        self.push_scope(CleanupScope::new(AstScopeKind(debug_loc.id),
                                          debug_loc.debug_loc()));
    }

    fn push_loop_cleanup_scope(&self,
                               id: ast::NodeId,
                               exits: [Block<'blk, 'tcx>; EXIT_MAX]) {
        debug!("push_loop_cleanup_scope({})",
               self.ccx.tcx().map.node_to_string(id));
        assert_eq!(Some(id), self.top_ast_scope());

        // Just copy the debuginfo source location from the enclosing scope
        let debug_loc = self.scopes
                            .borrow()
                            .last()
                            .unwrap()
                            .debug_loc;

        self.push_scope(CleanupScope::new(LoopScopeKind(id, exits), debug_loc));
    }

    fn push_custom_cleanup_scope(&self) -> CustomScopeIndex {
        let index = self.scopes_len();
        debug!("push_custom_cleanup_scope(): {}", index);

        // Just copy the debuginfo source location from the enclosing scope
        let debug_loc = self.scopes
                            .borrow()
                            .last()
                            .map(|opt_scope| opt_scope.debug_loc)
                            .unwrap_or(DebugLoc::None);

        self.push_scope(CleanupScope::new(CustomScopeKind, debug_loc));
        CustomScopeIndex { index: index }
    }

    fn push_custom_cleanup_scope_with_debug_loc(&self,
                                                debug_loc: NodeIdAndSpan)
                                                -> CustomScopeIndex {
        let index = self.scopes_len();
        debug!("push_custom_cleanup_scope(): {}", index);

        self.push_scope(CleanupScope::new(CustomScopeKind,
                                          debug_loc.debug_loc()));
        CustomScopeIndex { index: index }
    }

    /// Removes the cleanup scope for id `cleanup_scope`, which must be at the top of the cleanup
    /// stack, and generates the code to do its cleanups for normal exit.
    fn pop_and_trans_ast_cleanup_scope(&self,
                                       bcx: Block<'blk, 'tcx>,
                                       cleanup_scope: ast::NodeId)
                                       -> Block<'blk, 'tcx> {
        debug!("pop_and_trans_ast_cleanup_scope({})",
               self.ccx.tcx().map.node_to_string(cleanup_scope));

        assert!(self.top_scope(|s| s.kind.is_ast_with_id(cleanup_scope)));

        let scope = self.pop_scope();
        self.trans_scope_cleanups(bcx, &scope)
    }

    /// Removes the loop cleanup scope for id `cleanup_scope`, which must be at the top of the
    /// cleanup stack. Does not generate any cleanup code, since loop scopes should exit by
    /// branching to a block generated by `normal_exit_block`.
    fn pop_loop_cleanup_scope(&self,
                              cleanup_scope: ast::NodeId) {
        debug!("pop_loop_cleanup_scope({})",
               self.ccx.tcx().map.node_to_string(cleanup_scope));

        assert!(self.top_scope(|s| s.kind.is_loop_with_id(cleanup_scope)));

        let _ = self.pop_scope();
    }

    /// Removes the top cleanup scope from the stack without executing its cleanups. The top
    /// cleanup scope must be the temporary scope `custom_scope`.
    fn pop_custom_cleanup_scope(&self,
                                custom_scope: CustomScopeIndex) {
        debug!("pop_custom_cleanup_scope({})", custom_scope.index);
        assert!(self.is_valid_to_pop_custom_scope(custom_scope));
        let _ = self.pop_scope();
    }

    /// Removes the top cleanup scope from the stack, which must be a temporary scope, and
    /// generates the code to do its cleanups for normal exit.
    fn pop_and_trans_custom_cleanup_scope(&self,
                                          bcx: Block<'blk, 'tcx>,
                                          custom_scope: CustomScopeIndex)
                                          -> Block<'blk, 'tcx> {
        debug!("pop_and_trans_custom_cleanup_scope({:?})", custom_scope);
        assert!(self.is_valid_to_pop_custom_scope(custom_scope));

        let scope = self.pop_scope();
        self.trans_scope_cleanups(bcx, &scope)
    }

    /// Returns the id of the top-most loop scope
    fn top_loop_scope(&self) -> ast::NodeId {
        for scope in self.scopes.borrow().iter().rev() {
            if let LoopScopeKind(id, _) = scope.kind {
                return id;
            }
        }
        self.ccx.sess().bug("no loop scope found");
    }

    /// Returns a block to branch to which will perform all pending cleanups and then
    /// break/continue (depending on `exit`) out of the loop with id `cleanup_scope`
    fn normal_exit_block(&'blk self,
                         cleanup_scope: ast::NodeId,
                         exit: uint) -> BasicBlockRef {
        self.trans_cleanups_to_exit_scope(LoopExit(cleanup_scope, exit))
    }

    /// Returns a block to branch to which will perform all pending cleanups and then return from
    /// this function
    fn return_exit_block(&'blk self) -> BasicBlockRef {
        self.trans_cleanups_to_exit_scope(ReturnExit)
    }

    fn schedule_lifetime_end(&self,
                             cleanup_scope: ScopeId,
                             val: ValueRef) {
        let drop = box LifetimeEnd {
            ptr: val,
        };

        debug!("schedule_lifetime_end({:?}, val={})",
               cleanup_scope,
               self.ccx.tn().val_to_string(val));

        self.schedule_clean(cleanup_scope, drop as CleanupObj);
    }

    /// Schedules a (deep) drop of `val`, which is a pointer to an instance of `ty`
    fn schedule_drop_mem(&self,
                         cleanup_scope: ScopeId,
                         val: ValueRef,
                         ty: Ty<'tcx>) {
        if !common::type_needs_drop(self.ccx.tcx(), ty) { return; }
        let drop = box DropValue {
            is_immediate: false,
            must_unwind: common::type_needs_unwind_cleanup(self.ccx, ty),
            val: val,
            ty: ty,
            zero: false
        };

        debug!("schedule_drop_mem({:?}, val={}, ty={})",
               cleanup_scope,
               self.ccx.tn().val_to_string(val),
               ty.repr(self.ccx.tcx()));

        self.schedule_clean(cleanup_scope, drop as CleanupObj);
    }

    /// Schedules a (deep) drop and zero-ing of `val`, which is a pointer to an instance of `ty`
    fn schedule_drop_and_zero_mem(&self,
                                  cleanup_scope: ScopeId,
                                  val: ValueRef,
                                  ty: Ty<'tcx>) {
        if !common::type_needs_drop(self.ccx.tcx(), ty) { return; }
        let drop = box DropValue {
            is_immediate: false,
            must_unwind: common::type_needs_unwind_cleanup(self.ccx, ty),
            val: val,
            ty: ty,
            zero: true
        };

        debug!("schedule_drop_and_zero_mem({:?}, val={}, ty={}, zero={})",
               cleanup_scope,
               self.ccx.tn().val_to_string(val),
               ty.repr(self.ccx.tcx()),
               true);

        self.schedule_clean(cleanup_scope, drop as CleanupObj);
    }

    /// Schedules a (deep) drop of `val`, which is an instance of `ty`
    fn schedule_drop_immediate(&self,
                               cleanup_scope: ScopeId,
                               val: ValueRef,
                               ty: Ty<'tcx>) {

        if !common::type_needs_drop(self.ccx.tcx(), ty) { return; }
        let drop = box DropValue {
            is_immediate: true,
            must_unwind: common::type_needs_unwind_cleanup(self.ccx, ty),
            val: val,
            ty: ty,
            zero: false
        };

        debug!("schedule_drop_immediate({:?}, val={}, ty={:?})",
               cleanup_scope,
               self.ccx.tn().val_to_string(val),
               ty.repr(self.ccx.tcx()));

        self.schedule_clean(cleanup_scope, drop as CleanupObj);
    }

    /// Schedules a call to `free(val)`. Note that this is a shallow operation.
    fn schedule_free_value(&self,
                           cleanup_scope: ScopeId,
                           val: ValueRef,
                           heap: Heap,
                           content_ty: Ty<'tcx>) {
        let drop = box FreeValue { ptr: val, heap: heap, content_ty: content_ty };

        debug!("schedule_free_value({:?}, val={}, heap={:?})",
               cleanup_scope,
               self.ccx.tn().val_to_string(val),
               heap);

        self.schedule_clean(cleanup_scope, drop as CleanupObj);
    }

    /// Schedules a call to `free(val)`. Note that this is a shallow operation.
    fn schedule_free_slice(&self,
                           cleanup_scope: ScopeId,
                           val: ValueRef,
                           size: ValueRef,
                           align: ValueRef,
                           heap: Heap) {
        let drop = box FreeSlice { ptr: val, size: size, align: align, heap: heap };

        debug!("schedule_free_slice({:?}, val={}, heap={:?})",
               cleanup_scope,
               self.ccx.tn().val_to_string(val),
               heap);

        self.schedule_clean(cleanup_scope, drop as CleanupObj);
    }

    fn schedule_clean(&self,
                      cleanup_scope: ScopeId,
                      cleanup: CleanupObj<'tcx>) {
        match cleanup_scope {
            AstScope(id) => self.schedule_clean_in_ast_scope(id, cleanup),
            CustomScope(id) => self.schedule_clean_in_custom_scope(id, cleanup),
        }
    }

    /// Schedules a cleanup to occur upon exit from `cleanup_scope`. If `cleanup_scope` is not
    /// provided, then the cleanup is scheduled in the topmost scope, which must be a temporary
    /// scope.
    fn schedule_clean_in_ast_scope(&self,
                                   cleanup_scope: ast::NodeId,
                                   cleanup: CleanupObj<'tcx>) {
        debug!("schedule_clean_in_ast_scope(cleanup_scope={})",
               cleanup_scope);

        for scope in self.scopes.borrow_mut().iter_mut().rev() {
            if scope.kind.is_ast_with_id(cleanup_scope) {
                scope.cleanups.push(cleanup);
                scope.clear_cached_exits();
                return;
            } else {
                // will be adding a cleanup to some enclosing scope
                scope.clear_cached_exits();
            }
        }

        self.ccx.sess().bug(
            &format!("no cleanup scope {} found",
                    self.ccx.tcx().map.node_to_string(cleanup_scope))[]);
    }

    /// Schedules a cleanup to occur in the top-most scope, which must be a temporary scope.
    fn schedule_clean_in_custom_scope(&self,
                                      custom_scope: CustomScopeIndex,
                                      cleanup: CleanupObj<'tcx>) {
        debug!("schedule_clean_in_custom_scope(custom_scope={})",
               custom_scope.index);

        assert!(self.is_valid_custom_scope(custom_scope));

        let mut scopes = self.scopes.borrow_mut();
        let scope = &mut (*scopes)[custom_scope.index];
        scope.cleanups.push(cleanup);
        scope.clear_cached_exits();
    }

    /// Returns true if there are pending cleanups that should execute on panic.
    fn needs_invoke(&self) -> bool {
        self.scopes.borrow().iter().rev().any(|s| s.needs_invoke())
    }

    /// Returns a basic block to branch to in the event of a panic. This block will run the panic
    /// cleanups and eventually invoke the LLVM `Resume` instruction.
    fn get_landing_pad(&'blk self) -> BasicBlockRef {
        let _icx = base::push_ctxt("get_landing_pad");

        debug!("get_landing_pad");

        let orig_scopes_len = self.scopes_len();
        assert!(orig_scopes_len > 0);

        // Remove any scopes that do not have cleanups on panic:
        let mut popped_scopes = vec!();
        while !self.top_scope(|s| s.needs_invoke()) {
            debug!("top scope does not need invoke");
            popped_scopes.push(self.pop_scope());
        }

        // Check for an existing landing pad in the new topmost scope:
        let llbb = self.get_or_create_landing_pad();

        // Push the scopes we removed back on:
        loop {
            match popped_scopes.pop() {
                Some(scope) => self.push_scope(scope),
                None => break
            }
        }

        assert_eq!(self.scopes_len(), orig_scopes_len);

        return llbb;
    }
}

impl<'blk, 'tcx> CleanupHelperMethods<'blk, 'tcx> for FunctionContext<'blk, 'tcx> {
    /// Returns the id of the current top-most AST scope, if any.
    fn top_ast_scope(&self) -> Option<ast::NodeId> {
        for scope in self.scopes.borrow().iter().rev() {
            match scope.kind {
                CustomScopeKind | LoopScopeKind(..) => {}
                AstScopeKind(i) => {
                    return Some(i);
                }
            }
        }
        None
    }

    fn top_nonempty_cleanup_scope(&self) -> Option<uint> {
        self.scopes.borrow().iter().rev().position(|s| !s.cleanups.is_empty())
    }

    fn is_valid_to_pop_custom_scope(&self, custom_scope: CustomScopeIndex) -> bool {
        self.is_valid_custom_scope(custom_scope) &&
            custom_scope.index == self.scopes.borrow().len() - 1
    }

    fn is_valid_custom_scope(&self, custom_scope: CustomScopeIndex) -> bool {
        let scopes = self.scopes.borrow();
        custom_scope.index < scopes.len() &&
            (*scopes)[custom_scope.index].kind.is_temp()
    }

    /// Generates the cleanups for `scope` into `bcx`
    fn trans_scope_cleanups(&self, // cannot borrow self, will recurse
                            bcx: Block<'blk, 'tcx>,
                            scope: &CleanupScope<'blk, 'tcx>) -> Block<'blk, 'tcx> {

        let mut bcx = bcx;
        if !bcx.unreachable.get() {
            for cleanup in scope.cleanups.iter().rev() {
                bcx = cleanup.trans(bcx, scope.debug_loc);
            }
        }
        bcx
    }

    fn scopes_len(&self) -> uint {
        self.scopes.borrow().len()
    }

    fn push_scope(&self, scope: CleanupScope<'blk, 'tcx>) {
        self.scopes.borrow_mut().push(scope)
    }

    fn pop_scope(&self) -> CleanupScope<'blk, 'tcx> {
        debug!("popping cleanup scope {}, {} scopes remaining",
               self.top_scope(|s| s.block_name("")),
               self.scopes_len() - 1);

        self.scopes.borrow_mut().pop().unwrap()
    }

    fn top_scope<R, F>(&self, f: F) -> R where F: FnOnce(&CleanupScope<'blk, 'tcx>) -> R {
        f(self.scopes.borrow().last().unwrap())
    }

    /// Used when the caller wishes to jump to an early exit, such as a return, break, continue, or
    /// unwind. This function will generate all cleanups between the top of the stack and the exit
    /// `label` and return a basic block that the caller can branch to.
    ///
    /// For example, if the current stack of cleanups were as follows:
    ///
    ///      AST 22
    ///      Custom 1
    ///      AST 23
    ///      Loop 23
    ///      Custom 2
    ///      AST 24
    ///
    /// and the `label` specifies a break from `Loop 23`, then this function would generate a
    /// series of basic blocks as follows:
    ///
    ///      Cleanup(AST 24) -> Cleanup(Custom 2) -> break_blk
    ///
    /// where `break_blk` is the block specified in `Loop 23` as the target for breaks. The return
    /// value would be the first basic block in that sequence (`Cleanup(AST 24)`). The caller could
    /// then branch to `Cleanup(AST 24)` and it will perform all cleanups and finally branch to the
    /// `break_blk`.
    fn trans_cleanups_to_exit_scope(&'blk self,
                                    label: EarlyExitLabel)
                                    -> BasicBlockRef {
        debug!("trans_cleanups_to_exit_scope label={:?} scopes={}",
               label, self.scopes_len());

        let orig_scopes_len = self.scopes_len();
        let mut prev_llbb;
        let mut popped_scopes = vec!();

        // First we pop off all the cleanup stacks that are
        // traversed until the exit is reached, pushing them
        // onto the side vector `popped_scopes`. No code is
        // generated at this time.
        //
        // So, continuing the example from above, we would wind up
        // with a `popped_scopes` vector of `[AST 24, Custom 2]`.
        // (Presuming that there are no cached exits)
        loop {
            if self.scopes_len() == 0 {
                match label {
                    UnwindExit => {
                        // Generate a block that will `Resume`.
                        let prev_bcx = self.new_block(true, "resume", None);
                        let personality = self.personality.get().expect(
                            "create_landing_pad() should have set this");
                        build::Resume(prev_bcx,
                                      build::Load(prev_bcx, personality));
                        prev_llbb = prev_bcx.llbb;
                        break;
                    }

                    ReturnExit => {
                        prev_llbb = self.get_llreturn();
                        break;
                    }

                    LoopExit(id, _) => {
                        self.ccx.sess().bug(&format!(
                                "cannot exit from scope {}, \
                                not in scope", id)[]);
                    }
                }
            }

            // Check if we have already cached the unwinding of this
            // scope for this label. If so, we can stop popping scopes
            // and branch to the cached label, since it contains the
            // cleanups for any subsequent scopes.
            match self.top_scope(|s| s.cached_early_exit(label)) {
                Some(cleanup_block) => {
                    prev_llbb = cleanup_block;
                    break;
                }
                None => { }
            }

            // Pop off the scope, since we will be generating
            // unwinding code for it. If we are searching for a loop exit,
            // and this scope is that loop, then stop popping and set
            // `prev_llbb` to the appropriate exit block from the loop.
            popped_scopes.push(self.pop_scope());
            let scope = popped_scopes.last().unwrap();
            match label {
                UnwindExit | ReturnExit => { }
                LoopExit(id, exit) => {
                    match scope.kind.early_exit_block(id, exit) {
                        Some(exitllbb) => {
                            prev_llbb = exitllbb;
                            break;
                        }

                        None => { }
                    }
                }
            }
        }

        debug!("trans_cleanups_to_exit_scope: popped {} scopes",
               popped_scopes.len());

        // Now push the popped scopes back on. As we go,
        // we track in `prev_llbb` the exit to which this scope
        // should branch when it's done.
        //
        // So, continuing with our example, we will start out with
        // `prev_llbb` being set to `break_blk` (or possibly a cached
        // early exit). We will then pop the scopes from `popped_scopes`
        // and generate a basic block for each one, prepending it in the
        // series and updating `prev_llbb`. So we begin by popping `Custom 2`
        // and generating `Cleanup(Custom 2)`. We make `Cleanup(Custom 2)`
        // branch to `prev_llbb == break_blk`, giving us a sequence like:
        //
        //     Cleanup(Custom 2) -> prev_llbb
        //
        // We then pop `AST 24` and repeat the process, giving us the sequence:
        //
        //     Cleanup(AST 24) -> Cleanup(Custom 2) -> prev_llbb
        //
        // At this point, `popped_scopes` is empty, and so the final block
        // that we return to the user is `Cleanup(AST 24)`.
        while !popped_scopes.is_empty() {
            let mut scope = popped_scopes.pop().unwrap();

            if scope.cleanups.iter().any(|c| cleanup_is_suitable_for(&**c, label))
            {
                let name = scope.block_name("clean");
                debug!("generating cleanups for {}", name);
                let bcx_in = self.new_block(label.is_unwind(),
                                            &name[],
                                            None);
                let mut bcx_out = bcx_in;
                for cleanup in scope.cleanups.iter().rev() {
                    if cleanup_is_suitable_for(&**cleanup, label) {
                        bcx_out = cleanup.trans(bcx_out,
                                                scope.debug_loc);
                    }
                }
                build::Br(bcx_out, prev_llbb, DebugLoc::None);
                prev_llbb = bcx_in.llbb;
            } else {
                debug!("no suitable cleanups in {}",
                       scope.block_name("clean"));
            }

            scope.add_cached_early_exit(label, prev_llbb);
            self.push_scope(scope);
        }

        debug!("trans_cleanups_to_exit_scope: prev_llbb={:?}", prev_llbb);

        assert_eq!(self.scopes_len(), orig_scopes_len);
        prev_llbb
    }

    /// Creates a landing pad for the top scope, if one does not exist.  The landing pad will
    /// perform all cleanups necessary for an unwind and then `resume` to continue error
    /// propagation:
    ///
    ///     landing_pad -> ... cleanups ... -> [resume]
    ///
    /// (The cleanups and resume instruction are created by `trans_cleanups_to_exit_scope()`, not
    /// in this function itself.)
    fn get_or_create_landing_pad(&'blk self) -> BasicBlockRef {
        let pad_bcx;

        debug!("get_or_create_landing_pad");

        // Check if a landing pad block exists; if not, create one.
        {
            let mut scopes = self.scopes.borrow_mut();
            let last_scope = scopes.last_mut().unwrap();
            match last_scope.cached_landing_pad {
                Some(llbb) => { return llbb; }
                None => {
                    let name = last_scope.block_name("unwind");
                    pad_bcx = self.new_block(true, &name[], None);
                    last_scope.cached_landing_pad = Some(pad_bcx.llbb);
                }
            }
        }

        // The landing pad return type (the type being propagated). Not sure what
        // this represents but it's determined by the personality function and
        // this is what the EH proposal example uses.
        let llretty = Type::struct_(self.ccx,
                                    &[Type::i8p(self.ccx), Type::i32(self.ccx)],
                                    false);

        // The exception handling personality function.
        //
        // If our compilation unit has the `eh_personality` lang item somewhere
        // within it, then we just need to translate that. Otherwise, we're
        // building an rlib which will depend on some upstream implementation of
        // this function, so we just codegen a generic reference to it. We don't
        // specify any of the types for the function, we just make it a symbol
        // that LLVM can later use.
        let llpersonality = match pad_bcx.tcx().lang_items.eh_personality() {
            Some(def_id) => {
                callee::trans_fn_ref(pad_bcx.ccx(), def_id, ExprId(0),
                                     pad_bcx.fcx.param_substs).val
            }
            None => {
                let mut personality = self.ccx.eh_personality().borrow_mut();
                match *personality {
                    Some(llpersonality) => llpersonality,
                    None => {
                        let fty = Type::variadic_func(&[], &Type::i32(self.ccx));
                        let f = base::decl_cdecl_fn(self.ccx,
                                                    "rust_eh_personality",
                                                    fty,
                                                    self.ccx.tcx().types.i32);
                        *personality = Some(f);
                        f
                    }
                }
            }
        };

        // The only landing pad clause will be 'cleanup'
        let llretval = build::LandingPad(pad_bcx, llretty, llpersonality, 1u);

        // The landing pad block is a cleanup
        build::SetCleanup(pad_bcx, llretval);

        // We store the retval in a function-central alloca, so that calls to
        // Resume can find it.
        match self.personality.get() {
            Some(addr) => {
                build::Store(pad_bcx, llretval, addr);
            }
            None => {
                let addr = base::alloca(pad_bcx, common::val_ty(llretval), "");
                self.personality.set(Some(addr));
                build::Store(pad_bcx, llretval, addr);
            }
        }

        // Generate the cleanup block and branch to it.
        let cleanup_llbb = self.trans_cleanups_to_exit_scope(UnwindExit);
        build::Br(pad_bcx, cleanup_llbb, DebugLoc::None);

        return pad_bcx.llbb;
    }
}

impl<'blk, 'tcx> CleanupScope<'blk, 'tcx> {
    fn new(kind: CleanupScopeKind<'blk, 'tcx>,
           debug_loc: DebugLoc)
        -> CleanupScope<'blk, 'tcx> {
        CleanupScope {
            kind: kind,
            debug_loc: debug_loc,
            cleanups: vec!(),
            cached_early_exits: vec!(),
            cached_landing_pad: None,
        }
    }

    fn clear_cached_exits(&mut self) {
        self.cached_early_exits = vec!();
        self.cached_landing_pad = None;
    }

    fn cached_early_exit(&self,
                         label: EarlyExitLabel)
                         -> Option<BasicBlockRef> {
        self.cached_early_exits.iter().
            find(|e| e.label == label).
            map(|e| e.cleanup_block)
    }

    fn add_cached_early_exit(&mut self,
                             label: EarlyExitLabel,
                             blk: BasicBlockRef) {
        self.cached_early_exits.push(
            CachedEarlyExit { label: label,
                              cleanup_block: blk });
    }

    /// True if this scope has cleanups that need unwinding
    fn needs_invoke(&self) -> bool {

        self.cached_landing_pad.is_some() ||
            self.cleanups.iter().any(|c| c.must_unwind())
    }

    /// Returns a suitable name to use for the basic block that handles this cleanup scope
    fn block_name(&self, prefix: &str) -> String {
        match self.kind {
            CustomScopeKind => format!("{}_custom_", prefix),
            AstScopeKind(id) => format!("{}_ast_{}_", prefix, id),
            LoopScopeKind(id, _) => format!("{}_loop_{}_", prefix, id),
        }
    }

    pub fn drop_non_lifetime_clean(&mut self) {
        self.cleanups.retain(|c| c.is_lifetime_end());
    }
}

impl<'blk, 'tcx> CleanupScopeKind<'blk, 'tcx> {
    fn is_temp(&self) -> bool {
        match *self {
            CustomScopeKind => true,
            LoopScopeKind(..) | AstScopeKind(..) => false,
        }
    }

    fn is_ast_with_id(&self, id: ast::NodeId) -> bool {
        match *self {
            CustomScopeKind | LoopScopeKind(..) => false,
            AstScopeKind(i) => i == id
        }
    }

    fn is_loop_with_id(&self, id: ast::NodeId) -> bool {
        match *self {
            CustomScopeKind | AstScopeKind(..) => false,
            LoopScopeKind(i, _) => i == id
        }
    }

    /// If this is a loop scope with id `id`, return the early exit block `exit`, else `None`
    fn early_exit_block(&self,
                        id: ast::NodeId,
                        exit: uint) -> Option<BasicBlockRef> {
        match *self {
            LoopScopeKind(i, ref exits) if id == i => Some(exits[exit].llbb),
            _ => None,
        }
    }
}

impl EarlyExitLabel {
    fn is_unwind(&self) -> bool {
        match *self {
            UnwindExit => true,
            _ => false
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Cleanup types

#[derive(Copy)]
pub struct DropValue<'tcx> {
    is_immediate: bool,
    must_unwind: bool,
    val: ValueRef,
    ty: Ty<'tcx>,
    zero: bool
}

impl<'tcx> Cleanup<'tcx> for DropValue<'tcx> {
    fn must_unwind(&self) -> bool {
        self.must_unwind
    }

    fn clean_on_unwind(&self) -> bool {
        self.must_unwind
    }

    fn is_lifetime_end(&self) -> bool {
        false
    }

    fn trans<'blk>(&self,
                   bcx: Block<'blk, 'tcx>,
                   debug_loc: DebugLoc)
                   -> Block<'blk, 'tcx> {
        let bcx = if self.is_immediate {
            glue::drop_ty_immediate(bcx, self.val, self.ty, debug_loc)
        } else {
            glue::drop_ty(bcx, self.val, self.ty, debug_loc)
        };
        if self.zero {
            base::zero_mem(bcx, self.val, self.ty);
        }
        bcx
    }
}

#[derive(Copy, Debug)]
pub enum Heap {
    HeapExchange
}

#[derive(Copy)]
pub struct FreeValue<'tcx> {
    ptr: ValueRef,
    heap: Heap,
    content_ty: Ty<'tcx>
}

impl<'tcx> Cleanup<'tcx> for FreeValue<'tcx> {
    fn must_unwind(&self) -> bool {
        true
    }

    fn clean_on_unwind(&self) -> bool {
        true
    }

    fn is_lifetime_end(&self) -> bool {
        false
    }

    fn trans<'blk>(&self,
                   bcx: Block<'blk, 'tcx>,
                   debug_loc: DebugLoc)
                   -> Block<'blk, 'tcx> {
        debug_loc.apply(bcx.fcx);

        match self.heap {
            HeapExchange => {
                glue::trans_exchange_free_ty(bcx, self.ptr, self.content_ty)
            }
        }
    }
}

#[derive(Copy)]
pub struct FreeSlice {
    ptr: ValueRef,
    size: ValueRef,
    align: ValueRef,
    heap: Heap,
}

impl<'tcx> Cleanup<'tcx> for FreeSlice {
    fn must_unwind(&self) -> bool {
        true
    }

    fn clean_on_unwind(&self) -> bool {
        true
    }

    fn is_lifetime_end(&self) -> bool {
        false
    }

    fn trans<'blk>(&self,
                   bcx: Block<'blk, 'tcx>,
                   debug_loc: DebugLoc)
                   -> Block<'blk, 'tcx> {
        debug_loc.apply(bcx.fcx);

        match self.heap {
            HeapExchange => {
                glue::trans_exchange_free_dyn(bcx, self.ptr, self.size, self.align)
            }
        }
    }
}

#[derive(Copy)]
pub struct LifetimeEnd {
    ptr: ValueRef,
}

impl<'tcx> Cleanup<'tcx> for LifetimeEnd {
    fn must_unwind(&self) -> bool {
        false
    }

    fn clean_on_unwind(&self) -> bool {
        true
    }

    fn is_lifetime_end(&self) -> bool {
        true
    }

    fn trans<'blk>(&self,
                   bcx: Block<'blk, 'tcx>,
                   debug_loc: DebugLoc)
                   -> Block<'blk, 'tcx> {
        debug_loc.apply(bcx.fcx);
        base::call_lifetime_end(bcx, self.ptr);
        bcx
    }
}

pub fn temporary_scope(tcx: &ty::ctxt,
                       id: ast::NodeId)
                       -> ScopeId {
    match tcx.region_maps.temporary_scope(id) {
        Some(scope) => {
            let r = AstScope(scope.node_id());
            debug!("temporary_scope({}) = {:?}", id, r);
            r
        }
        None => {
            tcx.sess.bug(&format!("no temporary scope available for expr {}",
                                 id)[])
        }
    }
}

pub fn var_scope(tcx: &ty::ctxt,
                 id: ast::NodeId)
                 -> ScopeId {
    let r = AstScope(tcx.region_maps.var_scope(id).node_id());
    debug!("var_scope({}) = {:?}", id, r);
    r
}

fn cleanup_is_suitable_for(c: &Cleanup,
                           label: EarlyExitLabel) -> bool {
    !label.is_unwind() || c.clean_on_unwind()
}

///////////////////////////////////////////////////////////////////////////
// These traits just exist to put the methods into this file.

pub trait CleanupMethods<'blk, 'tcx> {
    fn push_ast_cleanup_scope(&self, id: NodeIdAndSpan);
    fn push_loop_cleanup_scope(&self,
                               id: ast::NodeId,
                               exits: [Block<'blk, 'tcx>; EXIT_MAX]);
    fn push_custom_cleanup_scope(&self) -> CustomScopeIndex;
    fn push_custom_cleanup_scope_with_debug_loc(&self,
                                                debug_loc: NodeIdAndSpan)
                                                -> CustomScopeIndex;
    fn pop_and_trans_ast_cleanup_scope(&self,
                                       bcx: Block<'blk, 'tcx>,
                                       cleanup_scope: ast::NodeId)
                                       -> Block<'blk, 'tcx>;
    fn pop_loop_cleanup_scope(&self,
                              cleanup_scope: ast::NodeId);
    fn pop_custom_cleanup_scope(&self,
                                custom_scope: CustomScopeIndex);
    fn pop_and_trans_custom_cleanup_scope(&self,
                                          bcx: Block<'blk, 'tcx>,
                                          custom_scope: CustomScopeIndex)
                                          -> Block<'blk, 'tcx>;
    fn top_loop_scope(&self) -> ast::NodeId;
    fn normal_exit_block(&'blk self,
                         cleanup_scope: ast::NodeId,
                         exit: uint) -> BasicBlockRef;
    fn return_exit_block(&'blk self) -> BasicBlockRef;
    fn schedule_lifetime_end(&self,
                         cleanup_scope: ScopeId,
                         val: ValueRef);
    fn schedule_drop_mem(&self,
                         cleanup_scope: ScopeId,
                         val: ValueRef,
                         ty: Ty<'tcx>);
    fn schedule_drop_and_zero_mem(&self,
                                  cleanup_scope: ScopeId,
                                  val: ValueRef,
                                  ty: Ty<'tcx>);
    fn schedule_drop_immediate(&self,
                               cleanup_scope: ScopeId,
                               val: ValueRef,
                               ty: Ty<'tcx>);
    fn schedule_free_value(&self,
                           cleanup_scope: ScopeId,
                           val: ValueRef,
                           heap: Heap,
                           content_ty: Ty<'tcx>);
    fn schedule_free_slice(&self,
                           cleanup_scope: ScopeId,
                           val: ValueRef,
                           size: ValueRef,
                           align: ValueRef,
                           heap: Heap);
    fn schedule_clean(&self,
                      cleanup_scope: ScopeId,
                      cleanup: CleanupObj<'tcx>);
    fn schedule_clean_in_ast_scope(&self,
                                   cleanup_scope: ast::NodeId,
                                   cleanup: CleanupObj<'tcx>);
    fn schedule_clean_in_custom_scope(&self,
                                    custom_scope: CustomScopeIndex,
                                    cleanup: CleanupObj<'tcx>);
    fn needs_invoke(&self) -> bool;
    fn get_landing_pad(&'blk self) -> BasicBlockRef;
}

trait CleanupHelperMethods<'blk, 'tcx> {
    fn top_ast_scope(&self) -> Option<ast::NodeId>;
    fn top_nonempty_cleanup_scope(&self) -> Option<uint>;
    fn is_valid_to_pop_custom_scope(&self, custom_scope: CustomScopeIndex) -> bool;
    fn is_valid_custom_scope(&self, custom_scope: CustomScopeIndex) -> bool;
    fn trans_scope_cleanups(&self,
                            bcx: Block<'blk, 'tcx>,
                            scope: &CleanupScope<'blk, 'tcx>) -> Block<'blk, 'tcx>;
    fn trans_cleanups_to_exit_scope(&'blk self,
                                    label: EarlyExitLabel)
                                    -> BasicBlockRef;
    fn get_or_create_landing_pad(&'blk self) -> BasicBlockRef;
    fn scopes_len(&self) -> uint;
    fn push_scope(&self, scope: CleanupScope<'blk, 'tcx>);
    fn pop_scope(&self) -> CleanupScope<'blk, 'tcx>;
    fn top_scope<R, F>(&self, f: F) -> R where F: FnOnce(&CleanupScope<'blk, 'tcx>) -> R;
}
