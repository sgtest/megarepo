// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hair::{self, Hair};
use rustc_data_structures::fnv::FnvHashMap;
use repr::*;

struct Builder<H:Hair> {
    hir: H,
    extents: FnvHashMap<H::CodeExtent, Vec<GraphExtent>>,
    cfg: CFG<H>,
    scopes: Vec<scope::Scope<H>>,
    loop_scopes: Vec<scope::LoopScope<H>>,
    unit_temp: Lvalue<H>,
    var_decls: Vec<VarDecl<H>>,
    var_indices: FnvHashMap<H::VarId, u32>,
    temp_decls: Vec<TempDecl<H>>,
}

struct CFG<H:Hair> {
    basic_blocks: Vec<BasicBlockData<H>>
}

///////////////////////////////////////////////////////////////////////////
// The `BlockAnd` "monad" packages up the new basic block along with a
// produced value (sometimes just unit, of course). The `unpack!`
// macro (and methods below) makes working with `BlockAnd` much more
// convenient.

#[must_use] // if you don't use one of these results, you're leaving a dangling edge
struct BlockAnd<T>(BasicBlock, T);

impl BasicBlock {
    fn and<T>(self, v: T) -> BlockAnd<T> {
        BlockAnd(self, v)
    }

    fn unit(self) -> BlockAnd<()> {
        BlockAnd(self, ())
    }
}

/// Update a block pointer and return the value.
/// Use it like `let x = unpack!(block = self.foo(block, foo))`.
macro_rules! unpack {
    ($x:ident = $c:expr) => {
        {
            let BlockAnd(b, v) = $c;
            $x = b;
            v
        }
    };

    ($c:expr) => {
        {
            let BlockAnd(b, ()) = $c;
            b
        }
    };
}

///////////////////////////////////////////////////////////////////////////
// construct() -- the main entry point for building MIR for a function

pub fn construct<H:Hair>(mut hir: H,
                        _span: H::Span,
                        implicit_arguments: Vec<H::Ty>,
                        explicit_arguments: Vec<(H::Ty, H::Pattern)>,
                        argument_extent: H::CodeExtent,
                        ast_block: H::Block)
                        -> Mir<H> {
    let cfg = CFG { basic_blocks: vec![] };

    // it's handy to have a temporary of type `()` sometimes, so make
    // one from the start and keep it available
    let temp_decls = vec![TempDecl::<H> { ty: hir.unit_ty() }];
    let unit_temp = Lvalue::Temp(0);

    let mut builder = Builder {
        hir: hir,
        cfg: cfg,
        extents: FnvHashMap(),
        scopes: vec![],
        loop_scopes: vec![],
        temp_decls: temp_decls,
        var_decls: vec![],
        var_indices: FnvHashMap(),
        unit_temp: unit_temp,
    };

    assert_eq!(builder.cfg.start_new_block(), START_BLOCK);
    assert_eq!(builder.cfg.start_new_block(), END_BLOCK);
    assert_eq!(builder.cfg.start_new_block(), DIVERGE_BLOCK);

    let mut block = START_BLOCK;
    let arg_decls = unpack!(block = builder.args_and_body(block,
                                                          implicit_arguments,
                                                          explicit_arguments,
                                                          argument_extent,
                                                          ast_block));

    builder.cfg.terminate(block, Terminator::Goto { target: END_BLOCK });
    builder.cfg.terminate(END_BLOCK, Terminator::Return);

    Mir  {
        basic_blocks: builder.cfg.basic_blocks,
        extents: builder.extents,
        var_decls: builder.var_decls,
        arg_decls: arg_decls,
        temp_decls: builder.temp_decls,
    }
}

impl<H:Hair> Builder<H> {
    fn args_and_body(&mut self,
                     mut block: BasicBlock,
                     implicit_arguments: Vec<H::Ty>,
                     explicit_arguments: Vec<(H::Ty, H::Pattern)>,
                     argument_extent: H::CodeExtent,
                     ast_block: H::Block)
                     -> BlockAnd<Vec<ArgDecl<H>>>
    {
        self.in_scope(argument_extent, block, |this| {
            let arg_decls = {
                let implicit_arg_decls = implicit_arguments.into_iter()
                                                           .map(|ty| ArgDecl { ty: ty });

                // to start, translate the argument patterns and collect the
                // argument types.
                let explicit_arg_decls =
                    explicit_arguments
                    .into_iter()
                    .enumerate()
                    .map(|(index, (ty, pattern))| {
                        let lvalue = Lvalue::Arg(index as u32);
                        unpack!(block = this.lvalue_into_pattern(block,
                                                                 argument_extent,
                                                                 hair::PatternRef::Hair(pattern),
                                                                 &lvalue));
                        ArgDecl { ty: ty }
                    });

                implicit_arg_decls.chain(explicit_arg_decls).collect()
            };

            // start the first basic block and translate the body
            unpack!(block = this.ast_block(&Lvalue::ReturnPointer, block, ast_block));

            block.and(arg_decls)
        })
    }
}

///////////////////////////////////////////////////////////////////////////
// Builder methods are broken up into modules, depending on what kind
// of thing is being translated. Note that they use the `unpack` macro
// above extensively.

mod block;
mod cfg;
mod expr;
mod into;
mod matches;
mod misc;
mod scope;
mod stmt;

