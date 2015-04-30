// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Type Names for Debug Info.

use super::namespace::crate_root_namespace;

use trans::common::CrateContext;
use middle::subst::{self, Substs};
use middle::ty::{self, Ty, ClosureTyper};
use syntax::ast;
use syntax::parse::token;
use util::ppaux;


// Compute the name of the type as it should be stored in debuginfo. Does not do
// any caching, i.e. calling the function twice with the same type will also do
// the work twice. The `qualified` parameter only affects the first level of the
// type name, further levels (i.e. type parameters) are always fully qualified.
pub fn compute_debuginfo_type_name<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>,
                                             t: Ty<'tcx>,
                                             qualified: bool)
                                             -> String {
    let mut result = String::with_capacity(64);
    push_debuginfo_type_name(cx, t, qualified, &mut result);
    result
}

// Pushes the name of the type as it should be stored in debuginfo on the
// `output` String. See also compute_debuginfo_type_name().
pub fn push_debuginfo_type_name<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>,
                                          t: Ty<'tcx>,
                                          qualified: bool,
                                          output: &mut String) {
    match t.sty {
        ty::ty_bool              => output.push_str("bool"),
        ty::ty_char              => output.push_str("char"),
        ty::ty_str               => output.push_str("str"),
        ty::ty_int(ast::TyIs)     => output.push_str("isize"),
        ty::ty_int(ast::TyI8)    => output.push_str("i8"),
        ty::ty_int(ast::TyI16)   => output.push_str("i16"),
        ty::ty_int(ast::TyI32)   => output.push_str("i32"),
        ty::ty_int(ast::TyI64)   => output.push_str("i64"),
        ty::ty_uint(ast::TyUs)    => output.push_str("usize"),
        ty::ty_uint(ast::TyU8)   => output.push_str("u8"),
        ty::ty_uint(ast::TyU16)  => output.push_str("u16"),
        ty::ty_uint(ast::TyU32)  => output.push_str("u32"),
        ty::ty_uint(ast::TyU64)  => output.push_str("u64"),
        ty::ty_float(ast::TyF32) => output.push_str("f32"),
        ty::ty_float(ast::TyF64) => output.push_str("f64"),
        ty::ty_struct(def_id, substs) |
        ty::ty_enum(def_id, substs) => {
            push_item_name(cx, def_id, qualified, output);
            push_type_params(cx, substs, output);
        },
        ty::ty_tup(ref component_types) => {
            output.push('(');
            for &component_type in component_types {
                push_debuginfo_type_name(cx, component_type, true, output);
                output.push_str(", ");
            }
            if !component_types.is_empty() {
                output.pop();
                output.pop();
            }
            output.push(')');
        },
        ty::ty_uniq(inner_type) => {
            output.push_str("Box<");
            push_debuginfo_type_name(cx, inner_type, true, output);
            output.push('>');
        },
        ty::ty_ptr(ty::mt { ty: inner_type, mutbl } ) => {
            output.push('*');
            match mutbl {
                ast::MutImmutable => output.push_str("const "),
                ast::MutMutable => output.push_str("mut "),
            }

            push_debuginfo_type_name(cx, inner_type, true, output);
        },
        ty::ty_rptr(_, ty::mt { ty: inner_type, mutbl }) => {
            output.push('&');
            if mutbl == ast::MutMutable {
                output.push_str("mut ");
            }

            push_debuginfo_type_name(cx, inner_type, true, output);
        },
        ty::ty_vec(inner_type, optional_length) => {
            output.push('[');
            push_debuginfo_type_name(cx, inner_type, true, output);

            match optional_length {
                Some(len) => {
                    output.push_str(&format!("; {}", len));
                }
                None => { /* nothing to do */ }
            };

            output.push(']');
        },
        ty::ty_trait(ref trait_data) => {
            let principal = ty::erase_late_bound_regions(cx.tcx(), &trait_data.principal);
            push_item_name(cx, principal.def_id, false, output);
            push_type_params(cx, principal.substs, output);
        },
        ty::ty_bare_fn(_, &ty::BareFnTy{ unsafety, abi, ref sig } ) => {
            if unsafety == ast::Unsafety::Unsafe {
                output.push_str("unsafe ");
            }

            if abi != ::syntax::abi::Rust {
                output.push_str("extern \"");
                output.push_str(abi.name());
                output.push_str("\" ");
            }

            output.push_str("fn(");

            let sig = ty::erase_late_bound_regions(cx.tcx(), sig);
            if !sig.inputs.is_empty() {
                for &parameter_type in &sig.inputs {
                    push_debuginfo_type_name(cx, parameter_type, true, output);
                    output.push_str(", ");
                }
                output.pop();
                output.pop();
            }

            if sig.variadic {
                if !sig.inputs.is_empty() {
                    output.push_str(", ...");
                } else {
                    output.push_str("...");
                }
            }

            output.push(')');

            match sig.output {
                ty::FnConverging(result_type) if ty::type_is_nil(result_type) => {}
                ty::FnConverging(result_type) => {
                    output.push_str(" -> ");
                    push_debuginfo_type_name(cx, result_type, true, output);
                }
                ty::FnDiverging => {
                    output.push_str(" -> !");
                }
            }
        },
        ty::ty_closure(..) => {
            output.push_str("closure");
        }
        ty::ty_err |
        ty::ty_infer(_) |
        ty::ty_projection(..) |
        ty::ty_param(_) => {
            cx.sess().bug(&format!("debuginfo: Trying to create type name for \
                unexpected type: {}", ppaux::ty_to_string(cx.tcx(), t)));
        }
    }

    fn push_item_name(cx: &CrateContext,
                      def_id: ast::DefId,
                      qualified: bool,
                      output: &mut String) {
        ty::with_path(cx.tcx(), def_id, |path| {
            if qualified {
                if def_id.krate == ast::LOCAL_CRATE {
                    output.push_str(crate_root_namespace(cx));
                    output.push_str("::");
                }

                let mut path_element_count = 0;
                for path_element in path {
                    let name = token::get_name(path_element.name());
                    output.push_str(&name);
                    output.push_str("::");
                    path_element_count += 1;
                }

                if path_element_count == 0 {
                    cx.sess().bug("debuginfo: Encountered empty item path!");
                }

                output.pop();
                output.pop();
            } else {
                let name = token::get_name(path.last()
                                               .expect("debuginfo: Empty item path?")
                                               .name());
                output.push_str(&name);
            }
        });
    }

    // Pushes the type parameters in the given `Substs` to the output string.
    // This ignores region parameters, since they can't reliably be
    // reconstructed for items from non-local crates. For local crates, this
    // would be possible but with inlining and LTO we have to use the least
    // common denominator - otherwise we would run into conflicts.
    fn push_type_params<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>,
                                  substs: &subst::Substs<'tcx>,
                                  output: &mut String) {
        if substs.types.is_empty() {
            return;
        }

        output.push('<');

        for &type_parameter in substs.types.iter() {
            push_debuginfo_type_name(cx, type_parameter, true, output);
            output.push_str(", ");
        }

        output.pop();
        output.pop();

        output.push('>');
    }
}

