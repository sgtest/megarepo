// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::traits;
use middle::traits::project::Normalized;
use middle::ty::{HasTypeFlags, TypeFlags, RegionEscape};
use middle::ty::fold::{TypeFoldable, TypeFolder};

use std::fmt;

// structural impls for the structs in middle::traits

impl<'tcx, T: fmt::Debug> fmt::Debug for Normalized<'tcx, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Normalized({:?},{:?})",
               self.value,
               self.obligations)
    }
}

impl<'tcx> fmt::Debug for traits::RegionObligation<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RegionObligation(sub_region={:?}, sup_type={:?})",
               self.sub_region,
               self.sup_type)
    }
}
impl<'tcx, O: fmt::Debug> fmt::Debug for traits::Obligation<'tcx, O> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Obligation(predicate={:?},depth={})",
               self.predicate,
               self.recursion_depth)
    }
}

impl<'tcx, N: fmt::Debug> fmt::Debug for traits::Vtable<'tcx, N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            super::VtableImpl(ref v) =>
                write!(f, "{:?}", v),

            super::VtableDefaultImpl(ref t) =>
                write!(f, "{:?}", t),

            super::VtableClosure(ref d) =>
                write!(f, "{:?}", d),

            super::VtableFnPointer(ref d) =>
                write!(f, "VtableFnPointer({:?})", d),

            super::VtableObject(ref d) =>
                write!(f, "{:?}", d),

            super::VtableParam(ref n) =>
                write!(f, "VtableParam({:?})", n),

            super::VtableBuiltin(ref d) =>
                write!(f, "{:?}", d)
        }
    }
}

impl<'tcx, N: fmt::Debug> fmt::Debug for traits::VtableImplData<'tcx, N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VtableImpl(impl_def_id={:?}, substs={:?}, nested={:?})",
               self.impl_def_id,
               self.substs,
               self.nested)
    }
}

impl<'tcx, N: fmt::Debug> fmt::Debug for traits::VtableClosureData<'tcx, N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VtableClosure(closure_def_id={:?}, substs={:?}, nested={:?})",
               self.closure_def_id,
               self.substs,
               self.nested)
    }
}

impl<'tcx, N: fmt::Debug> fmt::Debug for traits::VtableBuiltinData<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VtableBuiltin(nested={:?})", self.nested)
    }
}

impl<'tcx, N: fmt::Debug> fmt::Debug for traits::VtableDefaultImplData<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VtableDefaultImplData(trait_def_id={:?}, nested={:?})",
               self.trait_def_id,
               self.nested)
    }
}

impl<'tcx> fmt::Debug for traits::VtableObjectData<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VtableObject(upcast={:?}, vtable_base={})",
               self.upcast_trait_ref,
               self.vtable_base)
    }
}

impl<'tcx> fmt::Debug for traits::FulfillmentError<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FulfillmentError({:?},{:?})",
               self.obligation,
               self.code)
    }
}

impl<'tcx> fmt::Debug for traits::FulfillmentErrorCode<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            super::CodeSelectionError(ref e) => write!(f, "{:?}", e),
            super::CodeProjectionError(ref e) => write!(f, "{:?}", e),
            super::CodeAmbiguity => write!(f, "Ambiguity")
        }
    }
}

impl<'tcx> fmt::Debug for traits::MismatchedProjectionTypes<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MismatchedProjectionTypes({:?})", self.err)
    }
}

impl<'tcx, P: RegionEscape> RegionEscape for traits::Obligation<'tcx,P> {
    fn has_regions_escaping_depth(&self, depth: u32) -> bool {
        self.predicate.has_regions_escaping_depth(depth)
    }
}

impl<'tcx, T: HasTypeFlags> HasTypeFlags for traits::Obligation<'tcx, T> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.predicate.has_type_flags(flags)
    }
}

impl<'tcx, T: HasTypeFlags> HasTypeFlags for Normalized<'tcx, T> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.value.has_type_flags(flags) ||
            self.obligations.has_type_flags(flags)
    }
}

impl<'tcx, N: HasTypeFlags> HasTypeFlags for traits::VtableImplData<'tcx, N> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.substs.has_type_flags(flags) ||
            self.nested.has_type_flags(flags)
    }
}

impl<'tcx, N: HasTypeFlags> HasTypeFlags for traits::VtableClosureData<'tcx, N> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.substs.has_type_flags(flags) ||
            self.nested.has_type_flags(flags)
    }
}

impl<'tcx, N: HasTypeFlags> HasTypeFlags for traits::VtableDefaultImplData<N> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.nested.has_type_flags(flags)
    }
}

impl<'tcx, N: HasTypeFlags> HasTypeFlags for traits::VtableBuiltinData<N> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.nested.has_type_flags(flags)
    }
}

impl<'tcx> HasTypeFlags for traits::VtableObjectData<'tcx> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        self.upcast_trait_ref.has_type_flags(flags)
    }
}

impl<'tcx, N: HasTypeFlags> HasTypeFlags for traits::Vtable<'tcx, N> {
    fn has_type_flags(&self, flags: TypeFlags) -> bool {
        match *self {
            traits::VtableImpl(ref v) => v.has_type_flags(flags),
            traits::VtableDefaultImpl(ref t) => t.has_type_flags(flags),
            traits::VtableClosure(ref d) => d.has_type_flags(flags),
            traits::VtableFnPointer(ref d) => d.has_type_flags(flags),
            traits::VtableParam(ref n) => n.has_type_flags(flags),
            traits::VtableBuiltin(ref d) => d.has_type_flags(flags),
            traits::VtableObject(ref d) => d.has_type_flags(flags)
        }
    }
}

impl<'tcx, O: TypeFoldable<'tcx>> TypeFoldable<'tcx> for traits::Obligation<'tcx, O>
{
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::Obligation<'tcx, O> {
        traits::Obligation {
            cause: self.cause.clone(),
            recursion_depth: self.recursion_depth,
            predicate: self.predicate.fold_with(folder),
        }
    }
}

impl<'tcx, N: TypeFoldable<'tcx>> TypeFoldable<'tcx> for traits::VtableImplData<'tcx, N> {
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::VtableImplData<'tcx, N> {
        traits::VtableImplData {
            impl_def_id: self.impl_def_id,
            substs: self.substs.fold_with(folder),
            nested: self.nested.fold_with(folder),
        }
    }
}

impl<'tcx, N: TypeFoldable<'tcx>> TypeFoldable<'tcx> for traits::VtableClosureData<'tcx, N> {
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::VtableClosureData<'tcx, N> {
        traits::VtableClosureData {
            closure_def_id: self.closure_def_id,
            substs: self.substs.fold_with(folder),
            nested: self.nested.fold_with(folder),
        }
    }
}

impl<'tcx, N: TypeFoldable<'tcx>> TypeFoldable<'tcx> for traits::VtableDefaultImplData<N> {
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::VtableDefaultImplData<N> {
        traits::VtableDefaultImplData {
            trait_def_id: self.trait_def_id,
            nested: self.nested.fold_with(folder),
        }
    }
}

impl<'tcx, N: TypeFoldable<'tcx>> TypeFoldable<'tcx> for traits::VtableBuiltinData<N> {
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::VtableBuiltinData<N> {
        traits::VtableBuiltinData {
            nested: self.nested.fold_with(folder),
        }
    }
}

impl<'tcx> TypeFoldable<'tcx> for traits::VtableObjectData<'tcx> {
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::VtableObjectData<'tcx> {
        traits::VtableObjectData {
            upcast_trait_ref: self.upcast_trait_ref.fold_with(folder),
            vtable_base: self.vtable_base
        }
    }
}

impl<'tcx, N: TypeFoldable<'tcx>> TypeFoldable<'tcx> for traits::Vtable<'tcx, N> {
    fn fold_with<F:TypeFolder<'tcx>>(&self, folder: &mut F) -> traits::Vtable<'tcx, N> {
        match *self {
            traits::VtableImpl(ref v) => traits::VtableImpl(v.fold_with(folder)),
            traits::VtableDefaultImpl(ref t) => traits::VtableDefaultImpl(t.fold_with(folder)),
            traits::VtableClosure(ref d) => {
                traits::VtableClosure(d.fold_with(folder))
            }
            traits::VtableFnPointer(ref d) => {
                traits::VtableFnPointer(d.fold_with(folder))
            }
            traits::VtableParam(ref n) => traits::VtableParam(n.fold_with(folder)),
            traits::VtableBuiltin(ref d) => traits::VtableBuiltin(d.fold_with(folder)),
            traits::VtableObject(ref d) => traits::VtableObject(d.fold_with(folder)),
        }
    }
}

impl<'tcx, T: TypeFoldable<'tcx>> TypeFoldable<'tcx> for Normalized<'tcx, T> {
    fn fold_with<F: TypeFolder<'tcx>>(&self, folder: &mut F) -> Normalized<'tcx, T> {
        Normalized {
            value: self.value.fold_with(folder),
            obligations: self.obligations.fold_with(folder),
        }
    }
}
