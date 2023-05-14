//! This module contains implementations of the `TypeFoldable` and `TypeVisitable`
//! traits for various types in the Rust compiler. Most are written by hand, though
//! we've recently added some macros and proc-macros to help with the tedium.

use crate::fold::{FallibleTypeFolder, TypeFoldable};
use crate::visit::{TypeVisitable, TypeVisitor};
use crate::Interner;
use rustc_data_structures::functor::IdFunctor;
use rustc_data_structures::sync::Lrc;
use rustc_index::{Idx, IndexVec};

use std::ops::ControlFlow;

///////////////////////////////////////////////////////////////////////////
// Atomic structs
//
// For things that don't carry any arena-allocated data (and are
// copy...), just add them to this list.

TrivialTypeTraversalImpls! {
    (),
    bool,
    usize,
    u8,
    u16,
    u32,
    u64,
    String,
    crate::DebruijnIndex,
}

///////////////////////////////////////////////////////////////////////////
// Traversal implementations.

impl<I: Interner, T: TypeFoldable<I>, U: TypeFoldable<I>> TypeFoldable<I> for (T, U) {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<(T, U), F::Error> {
        Ok((self.0.try_fold_with(folder)?, self.1.try_fold_with(folder)?))
    }
}

impl<I: Interner, T: TypeVisitable<I>, U: TypeVisitable<I>> TypeVisitable<I> for (T, U) {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        self.0.visit_with(visitor)?;
        self.1.visit_with(visitor)
    }
}

impl<I: Interner, A: TypeFoldable<I>, B: TypeFoldable<I>, C: TypeFoldable<I>> TypeFoldable<I>
    for (A, B, C)
{
    fn try_fold_with<F: FallibleTypeFolder<I>>(
        self,
        folder: &mut F,
    ) -> Result<(A, B, C), F::Error> {
        Ok((
            self.0.try_fold_with(folder)?,
            self.1.try_fold_with(folder)?,
            self.2.try_fold_with(folder)?,
        ))
    }
}

impl<I: Interner, A: TypeVisitable<I>, B: TypeVisitable<I>, C: TypeVisitable<I>> TypeVisitable<I>
    for (A, B, C)
{
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        self.0.visit_with(visitor)?;
        self.1.visit_with(visitor)?;
        self.2.visit_with(visitor)
    }
}

impl<I: Interner, T: TypeFoldable<I>> TypeFoldable<I> for Option<T> {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<Self, F::Error> {
        Ok(match self {
            Some(v) => Some(v.try_fold_with(folder)?),
            None => None,
        })
    }
}

impl<I: Interner, T: TypeVisitable<I>> TypeVisitable<I> for Option<T> {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        match self {
            Some(v) => v.visit_with(visitor),
            None => ControlFlow::Continue(()),
        }
    }
}

impl<I: Interner, T: TypeFoldable<I>, E: TypeFoldable<I>> TypeFoldable<I> for Result<T, E> {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<Self, F::Error> {
        Ok(match self {
            Ok(v) => Ok(v.try_fold_with(folder)?),
            Err(e) => Err(e.try_fold_with(folder)?),
        })
    }
}

impl<I: Interner, T: TypeVisitable<I>, E: TypeVisitable<I>> TypeVisitable<I> for Result<T, E> {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        match self {
            Ok(v) => v.visit_with(visitor),
            Err(e) => e.visit_with(visitor),
        }
    }
}

impl<I: Interner, T: TypeFoldable<I>> TypeFoldable<I> for Lrc<T> {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<Self, F::Error> {
        self.try_map_id(|value| value.try_fold_with(folder))
    }
}

impl<I: Interner, T: TypeVisitable<I>> TypeVisitable<I> for Lrc<T> {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        (**self).visit_with(visitor)
    }
}

impl<I: Interner, T: TypeFoldable<I>> TypeFoldable<I> for Box<T> {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<Self, F::Error> {
        self.try_map_id(|value| value.try_fold_with(folder))
    }
}

impl<I: Interner, T: TypeVisitable<I>> TypeVisitable<I> for Box<T> {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        (**self).visit_with(visitor)
    }
}

impl<I: Interner, T: TypeFoldable<I>> TypeFoldable<I> for Vec<T> {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<Self, F::Error> {
        self.try_map_id(|t| t.try_fold_with(folder))
    }
}

impl<I: Interner, T: TypeVisitable<I>> TypeVisitable<I> for Vec<T> {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        self.iter().try_for_each(|t| t.visit_with(visitor))
    }
}

// `TypeFoldable` isn't impl'd for `&[T]`. It doesn't make sense in the general
// case, because we can't return a new slice. But note that there are a couple
// of trivial impls of `TypeFoldable` for specific slice types elsewhere.

impl<I: Interner, T: TypeVisitable<I>> TypeVisitable<I> for &[T] {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        self.iter().try_for_each(|t| t.visit_with(visitor))
    }
}

impl<I: Interner, T: TypeFoldable<I>, Ix: Idx> TypeFoldable<I> for IndexVec<Ix, T> {
    fn try_fold_with<F: FallibleTypeFolder<I>>(self, folder: &mut F) -> Result<Self, F::Error> {
        self.try_map_id(|x| x.try_fold_with(folder))
    }
}

impl<I: Interner, T: TypeVisitable<I>, Ix: Idx> TypeVisitable<I> for IndexVec<Ix, T> {
    fn visit_with<V: TypeVisitor<I>>(&self, visitor: &mut V) -> ControlFlow<V::BreakTy> {
        self.iter().try_for_each(|t| t.visit_with(visitor))
    }
}
