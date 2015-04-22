// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_camel_case_types)]

use middle::subst;
use trans::adt;
use trans::common::*;
use trans::foreign;
use trans::machine;
use middle::ty::{self, RegionEscape, Ty};
use util::ppaux;
use util::ppaux::Repr;

use trans::type_::Type;

use syntax::abi;
use syntax::ast;

// LLVM doesn't like objects that are too big. Issue #17913
fn ensure_array_fits_in_address_space<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                                llet: Type,
                                                size: machine::llsize,
                                                scapegoat: Ty<'tcx>) {
    let esz = machine::llsize_of_alloc(ccx, llet);
    match esz.checked_mul(size) {
        Some(n) if n < ccx.obj_size_bound() => {}
        _ => { ccx.report_overbig_object(scapegoat) }
    }
}

pub fn arg_is_indirect<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                 arg_ty: Ty<'tcx>) -> bool {
    !type_is_immediate(ccx, arg_ty)
}

pub fn return_uses_outptr<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                    ty: Ty<'tcx>) -> bool {
    !type_is_immediate(ccx, ty)
}

pub fn type_of_explicit_arg<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                      arg_ty: Ty<'tcx>) -> Type {
    let llty = arg_type_of(ccx, arg_ty);
    if arg_is_indirect(ccx, arg_ty) {
        llty.ptr_to()
    } else {
        llty
    }
}

/// Yields the types of the "real" arguments for this function. For most
/// functions, these are simply the types of the arguments. For functions with
/// the `RustCall` ABI, however, this untuples the arguments of the function.
pub fn untuple_arguments_if_necessary<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                                inputs: &[Ty<'tcx>],
                                                abi: abi::Abi)
                                                -> Vec<Ty<'tcx>> {
    if abi != abi::RustCall {
        return inputs.iter().cloned().collect()
    }

    if inputs.is_empty() {
        return Vec::new()
    }

    let mut result = Vec::new();
    for (i, &arg_prior_to_tuple) in inputs.iter().enumerate() {
        if i < inputs.len() - 1 {
            result.push(arg_prior_to_tuple);
        }
    }

    match inputs[inputs.len() - 1].sty {
        ty::ty_tup(ref tupled_arguments) => {
            debug!("untuple_arguments_if_necessary(): untupling arguments");
            for &tupled_argument in tupled_arguments {
                result.push(tupled_argument);
            }
        }
        _ => {
            ccx.tcx().sess.bug("argument to function with \"rust-call\" ABI \
                                is neither a tuple nor unit")
        }
    }

    result
}

pub fn type_of_rust_fn<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>,
                                 llenvironment_type: Option<Type>,
                                 sig: &ty::Binder<ty::FnSig<'tcx>>,
                                 abi: abi::Abi)
                                 -> Type
{
    debug!("type_of_rust_fn(sig={},abi={:?})",
           sig.repr(cx.tcx()),
           abi);

    let sig = ty::erase_late_bound_regions(cx.tcx(), sig);
    assert!(!sig.variadic); // rust fns are never variadic

    let mut atys: Vec<Type> = Vec::new();

    // First, munge the inputs, if this has the `rust-call` ABI.
    let inputs = untuple_arguments_if_necessary(cx, &sig.inputs, abi);

    // Arg 0: Output pointer.
    // (if the output type is non-immediate)
    let lloutputtype = match sig.output {
        ty::FnConverging(output) => {
            let use_out_pointer = return_uses_outptr(cx, output);
            let lloutputtype = arg_type_of(cx, output);
            // Use the output as the actual return value if it's immediate.
            if use_out_pointer {
                atys.push(lloutputtype.ptr_to());
                Type::void(cx)
            } else if return_type_is_void(cx, output) {
                Type::void(cx)
            } else {
                lloutputtype
            }
        }
        ty::FnDiverging => Type::void(cx)
    };

    // Arg 1: Environment
    match llenvironment_type {
        None => {}
        Some(llenvironment_type) => atys.push(llenvironment_type),
    }

    // ... then explicit args.
    let input_tys = inputs.iter().map(|&arg_ty| type_of_explicit_arg(cx, arg_ty));
    atys.extend(input_tys);

    Type::func(&atys[..], &lloutputtype)
}

// Given a function type and a count of ty params, construct an llvm type
pub fn type_of_fn_from_ty<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, fty: Ty<'tcx>) -> Type {
    match fty.sty {
        ty::ty_bare_fn(_, ref f) => {
            // FIXME(#19925) once fn item types are
            // zero-sized, we'll need to do something here
            if f.abi == abi::Rust || f.abi == abi::RustCall {
                type_of_rust_fn(cx, None, &f.sig, f.abi)
            } else {
                foreign::lltype_for_foreign_fn(cx, fty)
            }
        }
        _ => {
            cx.sess().bug("type_of_fn_from_ty given non-closure, non-bare-fn")
        }
    }
}

// A "sizing type" is an LLVM type, the size and alignment of which are
// guaranteed to be equivalent to what you would get out of `type_of()`. It's
// useful because:
//
// (1) It may be cheaper to compute the sizing type than the full type if all
//     you're interested in is the size and/or alignment;
//
// (2) It won't make any recursive calls to determine the structure of the
//     type behind pointers. This can help prevent infinite loops for
//     recursive types. For example, enum types rely on this behavior.

pub fn sizing_type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> Type {
    match cx.llsizingtypes().borrow().get(&t).cloned() {
        Some(t) => return t,
        None => ()
    }

    let llsizingty = match t.sty {
        _ if !type_is_sized(cx.tcx(), t) => {
            Type::struct_(cx, &[Type::i8p(cx), Type::i8p(cx)], false)
        }

        ty::ty_bool => Type::bool(cx),
        ty::ty_char => Type::char(cx),
        ty::ty_int(t) => Type::int_from_ty(cx, t),
        ty::ty_uint(t) => Type::uint_from_ty(cx, t),
        ty::ty_float(t) => Type::float_from_ty(cx, t),

        ty::ty_uniq(ty) | ty::ty_rptr(_, ty::mt{ty, ..}) | ty::ty_ptr(ty::mt{ty, ..}) => {
            if type_is_sized(cx.tcx(), ty) {
                Type::i8p(cx)
            } else {
                Type::struct_(cx, &[Type::i8p(cx), Type::i8p(cx)], false)
            }
        }

        ty::ty_bare_fn(..) => Type::i8p(cx),

        ty::ty_vec(ty, Some(size)) => {
            let llty = sizing_type_of(cx, ty);
            let size = size as u64;
            ensure_array_fits_in_address_space(cx, llty, size, t);
            Type::array(&llty, size)
        }

        ty::ty_tup(ref tys) if tys.is_empty() => {
            Type::nil(cx)
        }

        ty::ty_tup(..) | ty::ty_enum(..) | ty::ty_closure(..) => {
            let repr = adt::represent_type(cx, t);
            adt::sizing_type_of(cx, &*repr, false)
        }

        ty::ty_struct(..) => {
            if ty::type_is_simd(cx.tcx(), t) {
                let llet = type_of(cx, ty::simd_type(cx.tcx(), t));
                let n = ty::simd_size(cx.tcx(), t) as u64;
                ensure_array_fits_in_address_space(cx, llet, n, t);
                Type::vector(&llet, n)
            } else {
                let repr = adt::represent_type(cx, t);
                adt::sizing_type_of(cx, &*repr, false)
            }
        }

        ty::ty_projection(..) | ty::ty_infer(..) | ty::ty_param(..) | ty::ty_err(..) => {
            cx.sess().bug(&format!("fictitious type {} in sizing_type_of()",
                                  ppaux::ty_to_string(cx.tcx(), t)))
        }
        ty::ty_vec(_, None) | ty::ty_trait(..) | ty::ty_str => unreachable!()
    };

    cx.llsizingtypes().borrow_mut().insert(t, llsizingty);
    llsizingty
}

pub fn foreign_arg_type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> Type {
    if ty::type_is_bool(t) {
        Type::i1(cx)
    } else {
        type_of(cx, t)
    }
}

pub fn arg_type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> Type {
    if ty::type_is_bool(t) {
        Type::i1(cx)
    } else if type_is_immediate(cx, t) && type_of(cx, t).is_aggregate() {
        // We want to pass small aggregates as immediate values, but using an aggregate LLVM type
        // for this leads to bad optimizations, so its arg type is an appropriately sized integer
        match machine::llsize_of_alloc(cx, sizing_type_of(cx, t)) {
            0 => type_of(cx, t),
            n => Type::ix(cx, n * 8),
        }
    } else {
        type_of(cx, t)
    }
}

/// Get the LLVM type corresponding to a Rust type, i.e. `middle::ty::Ty`.
/// This is the right LLVM type for an alloca containing a value of that type,
/// and the pointee of an Lvalue Datum (which is always a LLVM pointer).
/// For unsized types, the returned type is a fat pointer, thus the resulting
/// LLVM type for a `Trait` Lvalue is `{ i8*, void(i8*)** }*`, which is a double
/// indirection to the actual data, unlike a `i8` Lvalue, which is just `i8*`.
/// This is needed due to the treatment of immediate values, as a fat pointer
/// is too large for it to be placed in SSA value (by our rules).
/// For the raw type without far pointer indirection, see `in_memory_type_of`.
pub fn type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, ty: Ty<'tcx>) -> Type {
    let ty = if !type_is_sized(cx.tcx(), ty) {
        ty::mk_imm_ptr(cx.tcx(), ty)
    } else {
        ty
    };
    in_memory_type_of(cx, ty)
}

/// Get the LLVM type corresponding to a Rust type, i.e. `middle::ty::Ty`.
/// This is the right LLVM type for a field/array element of that type,
/// and is the same as `type_of` for all Sized types.
/// Unsized types, however, are represented by a "minimal unit", e.g.
/// `[T]` becomes `T`, while `str` and `Trait` turn into `i8` - this
/// is useful for indexing slices, as `&[T]`'s data pointer is `T*`.
/// If the type is an unsized struct, the regular layout is generated,
/// with the inner-most trailing unsized field using the "minimal unit"
/// of that field's type - this is useful for taking the address of
/// that field and ensuring the struct has the right alignment.
/// For the LLVM type of a value as a whole, see `type_of`.
/// NB: If you update this, be sure to update `sizing_type_of()` as well.
pub fn in_memory_type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> Type {
    // Check the cache.
    match cx.lltypes().borrow().get(&t) {
        Some(&llty) => return llty,
        None => ()
    }

    debug!("type_of {} {:?}", t.repr(cx.tcx()), t.sty);

    assert!(!t.has_escaping_regions());

    // Replace any typedef'd types with their equivalent non-typedef
    // type. This ensures that all LLVM nominal types that contain
    // Rust types are defined as the same LLVM types.  If we don't do
    // this then, e.g. `Option<{myfield: bool}>` would be a different
    // type than `Option<myrec>`.
    let t_norm = erase_regions(cx.tcx(), &t);

    if t != t_norm {
        let llty = in_memory_type_of(cx, t_norm);
        debug!("--> normalized {} {:?} to {} {:?} llty={}",
                t.repr(cx.tcx()),
                t,
                t_norm.repr(cx.tcx()),
                t_norm,
                cx.tn().type_to_string(llty));
        cx.lltypes().borrow_mut().insert(t, llty);
        return llty;
    }

    let mut llty = match t.sty {
      ty::ty_bool => Type::bool(cx),
      ty::ty_char => Type::char(cx),
      ty::ty_int(t) => Type::int_from_ty(cx, t),
      ty::ty_uint(t) => Type::uint_from_ty(cx, t),
      ty::ty_float(t) => Type::float_from_ty(cx, t),
      ty::ty_enum(did, ref substs) => {
          // Only create the named struct, but don't fill it in. We
          // fill it in *after* placing it into the type cache. This
          // avoids creating more than one copy of the enum when one
          // of the enum's variants refers to the enum itself.
          let repr = adt::represent_type(cx, t);
          let tps = substs.types.get_slice(subst::TypeSpace);
          let name = llvm_type_name(cx, did, tps);
          adt::incomplete_type_of(cx, &*repr, &name[..])
      }
      ty::ty_closure(..) => {
          // Only create the named struct, but don't fill it in. We
          // fill it in *after* placing it into the type cache.
          let repr = adt::represent_type(cx, t);
          // Unboxed closures can have substitutions in all spaces
          // inherited from their environment, so we use entire
          // contents of the VecPerParamSpace to to construct the llvm
          // name
          adt::incomplete_type_of(cx, &*repr, "closure")
      }

      ty::ty_uniq(ty) | ty::ty_rptr(_, ty::mt{ty, ..}) | ty::ty_ptr(ty::mt{ty, ..}) => {
          if !type_is_sized(cx.tcx(), ty) {
              if let ty::ty_str = ty.sty {
                  // This means we get a nicer name in the output (str is always
                  // unsized).
                  cx.tn().find_type("str_slice").unwrap()
              } else {
                  let ptr_ty = in_memory_type_of(cx, ty).ptr_to();
                  let unsized_part = ty::struct_tail(cx.tcx(), ty);
                  let info_ty = match unsized_part.sty {
                      ty::ty_str | ty::ty_vec(..) => {
                          Type::uint_from_ty(cx, ast::TyUs)
                      }
                      ty::ty_trait(_) => Type::vtable_ptr(cx),
                      _ => panic!("Unexpected type returned from \
                                   struct_tail: {} for ty={}",
                                  unsized_part.repr(cx.tcx()), ty.repr(cx.tcx()))
                  };
                  Type::struct_(cx, &[ptr_ty, info_ty], false)
              }
          } else {
              in_memory_type_of(cx, ty).ptr_to()
          }
      }

      ty::ty_vec(ty, Some(size)) => {
          let size = size as u64;
          let llty = in_memory_type_of(cx, ty);
          ensure_array_fits_in_address_space(cx, llty, size, t);
          Type::array(&llty, size)
      }

      // Unsized slice types (and str) have the type of their element, and
      // traits have the type of u8. This is so that the data pointer inside
      // fat pointers is of the right type (e.g. for array accesses), even
      // when taking the address of an unsized field in a struct.
      ty::ty_vec(ty, None) => in_memory_type_of(cx, ty),
      ty::ty_str | ty::ty_trait(..) => Type::i8(cx),

      ty::ty_bare_fn(..) => {
          type_of_fn_from_ty(cx, t).ptr_to()
      }
      ty::ty_tup(ref tys) if tys.is_empty() => Type::nil(cx),
      ty::ty_tup(..) => {
          let repr = adt::represent_type(cx, t);
          adt::type_of(cx, &*repr)
      }
      ty::ty_struct(did, ref substs) => {
          if ty::type_is_simd(cx.tcx(), t) {
              let llet = in_memory_type_of(cx, ty::simd_type(cx.tcx(), t));
              let n = ty::simd_size(cx.tcx(), t) as u64;
              ensure_array_fits_in_address_space(cx, llet, n, t);
              Type::vector(&llet, n)
          } else {
              // Only create the named struct, but don't fill it in. We fill it
              // in *after* placing it into the type cache. This prevents
              // infinite recursion with recursive struct types.
              let repr = adt::represent_type(cx, t);
              let tps = substs.types.get_slice(subst::TypeSpace);
              let name = llvm_type_name(cx, did, tps);
              adt::incomplete_type_of(cx, &*repr, &name[..])
          }
      }

      ty::ty_infer(..) => cx.sess().bug("type_of with ty_infer"),
      ty::ty_projection(..) => cx.sess().bug("type_of with ty_projection"),
      ty::ty_param(..) => cx.sess().bug("type_of with ty_param"),
      ty::ty_err(..) => cx.sess().bug("type_of with ty_err"),
    };

    debug!("--> mapped t={} {:?} to llty={}",
            t.repr(cx.tcx()),
            t,
            cx.tn().type_to_string(llty));

    cx.lltypes().borrow_mut().insert(t, llty);

    // If this was an enum or struct, fill in the type now.
    match t.sty {
        ty::ty_enum(..) | ty::ty_struct(..) | ty::ty_closure(..)
                if !ty::type_is_simd(cx.tcx(), t) => {
            let repr = adt::represent_type(cx, t);
            adt::finish_type_of(cx, &*repr, &mut llty);
        }
        _ => ()
    }

    llty
}

pub fn align_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>)
                          -> machine::llalign {
    let llty = sizing_type_of(cx, t);
    machine::llalign_of_min(cx, llty)
}

fn llvm_type_name<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>,
                            did: ast::DefId,
                            tps: &[Ty<'tcx>])
                            -> String {
    let base = ty::item_path_str(cx.tcx(), did);
    let strings: Vec<String> = tps.iter().map(|t| t.repr(cx.tcx())).collect();
    let tstr = if strings.is_empty() {
        base
    } else {
        format!("{}<{}>", base, strings.connect(", "))
    };

    if did.krate == 0 {
        tstr
    } else {
        format!("{}.{}", did.krate, tstr)
    }
}

pub fn type_of_dtor<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, self_ty: Ty<'tcx>) -> Type {
    let self_ty = type_of(ccx, self_ty).ptr_to();
    Type::func(&[self_ty], &Type::void(ccx))
}
