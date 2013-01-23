// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Coherence phase
//
// The job of the coherence phase of typechecking is to ensure that each trait
// has at most one implementation for each type. Then we build a mapping from
// each trait in the system to its implementations.

use core::prelude::*;

use driver;
use metadata::csearch::{ProvidedTraitMethodInfo, each_path, get_impl_traits};
use metadata::csearch::{get_impls_for_mod};
use metadata::csearch;
use metadata::cstore::{CStore, iter_crate_data};
use metadata::decoder::{dl_def, dl_field, dl_impl};
use middle::resolve::{Impl, MethodInfo};
use middle::ty::{ProvidedMethodSource, ProvidedMethodInfo, bound_copy, get};
use middle::ty::{kind_can_be_copied, lookup_item_type, param_bounds, subst};
use middle::ty::{t, ty_bool, ty_bot, ty_box, ty_enum, ty_err, ty_estr};
use middle::ty::{ty_evec, ty_float, ty_fn, ty_infer, ty_int, ty_nil};
use middle::ty::{ty_opaque_box, ty_param, ty_param_bounds_and_ty, ty_ptr};
use middle::ty::{ty_rec, ty_rptr, ty_self, ty_struct, ty_trait, ty_tup};
use middle::ty::{ty_type, ty_uint, ty_uniq};
use middle::ty::{ty_opaque_closure_ptr, ty_unboxed_vec, type_kind_ext};
use middle::ty::{type_is_ty_var};
use middle::ty;
use middle::typeck::crate_ctxt;
use middle::typeck::infer::{InferCtxt, can_mk_subty};
use middle::typeck::infer::{new_infer_ctxt, resolve_ivar};
use middle::typeck::infer::{resolve_nested_tvar, resolve_type};
use syntax::ast::{crate, def_id, def_mod, def_ty};
use syntax::ast::{item, item_struct, item_const, item_enum, item_fn};
use syntax::ast::{item_foreign_mod, item_impl, item_mac, item_mod};
use syntax::ast::{item_trait, item_ty, local_crate, method, node_id};
use syntax::ast::{trait_ref};
use syntax::ast;
use syntax::ast_map::node_item;
use syntax::ast_map;
use syntax::ast_util::{def_id_of_def, dummy_sp, local_def};
use syntax::attr;
use syntax::codemap::span;
use syntax::parse;
use syntax::visit::{default_simple_visitor, default_visitor};
use syntax::visit::{mk_simple_visitor, mk_vt, visit_crate, visit_item};
use syntax::visit::{Visitor, SimpleVisitor};
use syntax::visit::{visit_mod};
use util::ppaux::ty_to_str;

use core::dvec::DVec;
use core::result::Ok;
use core::send_map;
use core::uint::range;
use core::uint;
use core::vec::{len, push};
use core::vec;
use std::map::HashMap;

struct UniversalQuantificationResult {
    monotype: t,
    type_variables: ~[ty::t],
    bounds: @~[param_bounds]
}

fn get_base_type(inference_context: @InferCtxt, span: span, original_type: t)
              -> Option<t> {

    let resolved_type;
    match resolve_type(inference_context,
                     original_type,
                     resolve_ivar) {
        Ok(resulting_type) if !type_is_ty_var(resulting_type) => {
            resolved_type = resulting_type;
        }
        _ => {
            inference_context.tcx.sess.span_fatal(span,
                                                  ~"the type of this value \
                                                    must be known in order \
                                                    to determine the base \
                                                    type");
        }
    }

    match get(resolved_type).sty {
        ty_box(base_mutability_and_type) |
        ty_uniq(base_mutability_and_type) |
        ty_ptr(base_mutability_and_type) |
        ty_rptr(_, base_mutability_and_type) => {
            debug!("(getting base type) recurring");
            get_base_type(inference_context, span,
                          base_mutability_and_type.ty)
        }

        ty_enum(*) | ty_trait(*) | ty_struct(*) => {
            debug!("(getting base type) found base type");
            Some(resolved_type)
        }

        ty_nil | ty_bot | ty_bool | ty_int(*) | ty_uint(*) | ty_float(*) |
        ty_estr(*) | ty_evec(*) | ty_rec(*) |
        ty_fn(*) | ty_tup(*) | ty_infer(*) |
        ty_param(*) | ty_self | ty_type | ty_opaque_box |
        ty_opaque_closure_ptr(*) | ty_unboxed_vec(*) | ty_err => {
            debug!("(getting base type) no base type; found %?",
                   get(original_type).sty);
            None
        }
    }
}

// Returns the def ID of the base type, if there is one.
fn get_base_type_def_id(inference_context: @InferCtxt,
                        span: span,
                        original_type: t)
                     -> Option<def_id> {

    match get_base_type(inference_context, span, original_type) {
        None => {
            return None;
        }
        Some(base_type) => {
            match get(base_type).sty {
                ty_enum(def_id, _) |
                ty_struct(def_id, _) |
                ty_trait(def_id, _, _) => {
                    return Some(def_id);
                }
                _ => {
                    fail ~"get_base_type() returned a type that wasn't an \
                           enum, class, or trait";
                }
            }
        }
    }
}


fn method_to_MethodInfo(ast_method: @method) -> @MethodInfo {
    @{
        did: local_def(ast_method.id),
        n_tps: ast_method.tps.len(),
        ident: ast_method.ident,
        self_type: ast_method.self_ty.node
    }
}

struct CoherenceInfo {
    // Contains implementations of methods that are inherent to a type.
    // Methods in these implementations don't need to be exported.
    inherent_methods: HashMap<def_id,@DVec<@Impl>>,

    // Contains implementations of methods associated with a trait. For these,
    // the associated trait must be imported at the call site.
    extension_methods: HashMap<def_id,@DVec<@Impl>>,

}

fn CoherenceInfo() -> CoherenceInfo {
    CoherenceInfo {
        inherent_methods: HashMap(),
        extension_methods: HashMap(),
    }
}

fn CoherenceChecker(crate_context: @crate_ctxt) -> CoherenceChecker {
    CoherenceChecker {
        crate_context: crate_context,
        inference_context: new_infer_ctxt(crate_context.tcx),

        base_type_def_ids: HashMap(),
        privileged_implementations: HashMap()
    }
}

struct CoherenceChecker {
    crate_context: @crate_ctxt,
    inference_context: @InferCtxt,

    // A mapping from implementations to the corresponding base type
    // definition ID.

    base_type_def_ids: HashMap<def_id,def_id>,

    // A set of implementations in privileged scopes; i.e. those
    // implementations that are defined in the same scope as their base types.

    privileged_implementations: HashMap<node_id,()>,
}

impl CoherenceChecker {
    fn check_coherence(crate: @crate) {
        // Check implementations and traits. This populates the tables
        // containing the inherent methods and extension methods. It also
        // builds up the trait inheritance table.
        visit_crate(*crate, (), mk_simple_visitor(@SimpleVisitor {
            visit_item: |item| {
                debug!("(checking coherence) item '%s'",
                       self.crate_context.tcx.sess.str_of(item.ident));

                match item.node {
                    item_impl(_, opt_trait, _, _) => {
                        self.check_implementation(item, opt_trait.to_vec());
                    }
                    _ => {
                        // Nothing to do.
                    }
                };
            },
            .. *default_simple_visitor()
        }));

        // Check that there are no overlapping trait instances
        self.check_implementation_coherence();

        // Check whether traits with base types are in privileged scopes.
        self.check_privileged_scopes(crate);

        // Bring in external crates. It's fine for this to happen after the
        // coherence checks, because we ensure by construction that no errors
        // can happen at link time.
        self.add_external_crates();

        // Populate the table of destructors. It might seem a bit strange to
        // do this here, but it's actually the most convenient place, since
        // the coherence tables contain the trait -> type mappings.
        self.populate_destructor_table();
    }

    fn check_implementation(item: @item, associated_traits: ~[@trait_ref]) {
        let self_type = self.crate_context.tcx.tcache.get(local_def(item.id));

        // If there are no traits, then this implementation must have a
        // base type.

        if associated_traits.len() == 0 {
            debug!("(checking implementation) no associated traits for item \
                    '%s'",
                   self.crate_context.tcx.sess.str_of(item.ident));

            match get_base_type_def_id(self.inference_context,
                                       item.span,
                                       self_type.ty) {
                None => {
                    let session = self.crate_context.tcx.sess;
                    session.span_err(item.span,
                                     ~"no base type found for inherent \
                                       implementation; implement a \
                                       trait or new type instead");
                }
                Some(_) => {
                    // Nothing to do.
                }
            }
        }

        // We only want to generate one Impl structure. When we generate one,
        // we store it here so that we don't recreate it.
        let mut implementation_opt = None;
        for associated_traits.each |associated_trait| {
            let trait_did =
                self.trait_ref_to_trait_def_id(*associated_trait);
            debug!("(checking implementation) adding impl for trait \
                    '%s', item '%s'",
                    ast_map::node_id_to_str(
                        self.crate_context.tcx.items, trait_did.node,
                        self.crate_context.tcx.sess.parse_sess.interner),
                    self.crate_context.tcx.sess.str_of(item.ident));

            self.instantiate_default_methods(item.id, trait_did);

            let implementation;
            if implementation_opt.is_none() {
                implementation = self.create_impl_from_item(item);
                implementation_opt = Some(implementation);
            }

            self.add_trait_method(trait_did, implementation_opt.get());
        }

        // Add the implementation to the mapping from implementation to base
        // type def ID, if there is a base type for this implementation.

        match get_base_type_def_id(self.inference_context,
                                   item.span,
                                   self_type.ty) {
            None => {
                // Nothing to do.
            }
            Some(base_type_def_id) => {
                // XXX: Gather up default methods?
                let implementation;
                match implementation_opt {
                    None => {
                        implementation = self.create_impl_from_item(item);
                    }
                    Some(copy existing_implementation) => {
                        implementation = existing_implementation;
                    }
                }
                self.add_inherent_method(base_type_def_id, implementation);

                self.base_type_def_ids.insert(local_def(item.id),
                                              base_type_def_id);
            }
        }
    }

    // Creates default method IDs and performs type substitutions for an impl
    // and trait pair. Then, for each provided method in the trait, inserts a
    // `ProvidedMethodInfo` instance into the `provided_method_sources` map.
    fn instantiate_default_methods(impl_id: ast::node_id,
                                   trait_did: ast::def_id) {
        for self.each_provided_trait_method(trait_did) |trait_method| {
            // Synthesize an ID.
            let tcx = self.crate_context.tcx;
            let new_id = parse::next_node_id(tcx.sess.parse_sess);
            let new_did = local_def(new_id);

            // XXX: Perform substitutions.
            let new_polytype = ty::lookup_item_type(tcx, trait_method.def_id);
            tcx.tcache.insert(new_did, new_polytype);

            // Pair the new synthesized ID up with the
            // ID of the method.
            let source = ProvidedMethodSource {
                method_id: trait_method.def_id,
                impl_id: local_def(impl_id)
            };

            self.crate_context.tcx.provided_method_sources.insert(new_did,
                                                                  source);

            let provided_method_info =
                @ProvidedMethodInfo {
                    method_info: @{
                        did: new_did,
                        n_tps: trait_method.tps.len(),
                        ident: trait_method.ident,
                        self_type: trait_method.self_ty
                    },
                    trait_method_def_id: trait_method.def_id
                };

            let pmm = self.crate_context.tcx.provided_methods;
            match pmm.find(local_def(impl_id)) {
                Some(mis) => {
                    // If the trait already has an entry in the
                    // provided_methods_map, we just need to add this
                    // method to that entry.
                    debug!("(checking implementation) adding method `%s` \
                            to entry for existing trait",
                            self.crate_context.tcx.sess.str_of(
                                provided_method_info.method_info.ident));
                    mis.push(provided_method_info);
                }
                None => {
                    // If the trait doesn't have an entry yet, create one.
                    debug!("(checking implementation) creating new entry \
                            for method `%s`",
                            self.crate_context.tcx.sess.str_of(
                                provided_method_info.method_info.ident));
                    let method_infos = @DVec();
                    method_infos.push(provided_method_info);
                    pmm.insert(local_def(impl_id), method_infos);
                }
            }
        }
    }

    fn add_inherent_method(base_def_id: def_id, implementation: @Impl) {
        let implementation_list;
        match self.crate_context.coherence_info.inherent_methods
                  .find(base_def_id) {
            None => {
                implementation_list = @DVec();
                self.crate_context.coherence_info.inherent_methods
                    .insert(base_def_id, implementation_list);
            }
            Some(existing_implementation_list) => {
                implementation_list = existing_implementation_list;
            }
        }

        implementation_list.push(implementation);
    }

    fn add_trait_method(trait_id: def_id, implementation: @Impl) {
        let implementation_list;
        match self.crate_context.coherence_info.extension_methods
                  .find(trait_id) {
            None => {
                implementation_list = @DVec();
                self.crate_context.coherence_info.extension_methods
                    .insert(trait_id, implementation_list);
            }
            Some(existing_implementation_list) => {
                implementation_list = existing_implementation_list;
            }
        }

        implementation_list.push(implementation);
    }

    fn check_implementation_coherence() {
        let coherence_info = &self.crate_context.coherence_info;
        let extension_methods = &coherence_info.extension_methods;

        for extension_methods.each_key |trait_id| {
            self.check_implementation_coherence_of(trait_id);
        }
    }

    fn check_implementation_coherence_of(trait_def_id: def_id) {

        // Unify pairs of polytypes.
        do self.iter_impls_of_trait(trait_def_id) |a| {
            let implementation_a = a;
            let polytype_a =
                self.get_self_type_for_implementation(implementation_a);
            do self.iter_impls_of_trait(trait_def_id) |b| {
                let implementation_b = b;

                // An impl is coherent with itself
                if a.did != b.did {
                    let polytype_b = self.get_self_type_for_implementation(
                            implementation_b);

                    if self.polytypes_unify(polytype_a, polytype_b) {
                        let session = self.crate_context.tcx.sess;
                        session.span_err(self.span_of_impl(implementation_b),
                                         ~"conflicting implementations for a \
                                           trait");
                        session.span_note(self.span_of_impl(implementation_a),
                                          ~"note conflicting implementation \
                                            here");
                    }
                }
            }
        }
    }

    fn iter_impls_of_trait(trait_def_id: def_id,
                           f: &fn(@Impl)) {

        let coherence_info = &self.crate_context.coherence_info;
        let extension_methods = &coherence_info.extension_methods;

        match extension_methods.find(trait_def_id) {
            Some(impls) => {
                for uint::range(0, impls.len()) |i| {
                    f(impls[i]);
                }
            }
            None => { /* no impls? */ }
        }
    }

    fn each_provided_trait_method(
            trait_did: ast::def_id,
            f: &fn(x: &ty::method) -> bool) {
        // Make a list of all the names of the provided methods.
        // XXX: This is horrible.
        let provided_method_idents = HashMap();
        let tcx = self.crate_context.tcx;
        for ty::provided_trait_methods(tcx, trait_did).each |ident| {
            provided_method_idents.insert(*ident, ());
        }

        for ty::trait_methods(tcx, trait_did).each |method| {
            if provided_method_idents.contains_key(method.ident) {
                if !f(method) {
                    break;
                }
            }
        }
    }

    fn polytypes_unify(polytype_a: ty_param_bounds_and_ty,
                       polytype_b: ty_param_bounds_and_ty)
                    -> bool {
        let universally_quantified_a =
            self.universally_quantify_polytype(polytype_a);
        let universally_quantified_b =
            self.universally_quantify_polytype(polytype_b);

        return self.can_unify_universally_quantified(
            &universally_quantified_a, &universally_quantified_b) ||
            self.can_unify_universally_quantified(
            &universally_quantified_b, &universally_quantified_a);
    }

    // Converts a polytype to a monotype by replacing all parameters with
    // type variables. Returns the monotype and the type variables created.
    fn universally_quantify_polytype(polytype: ty_param_bounds_and_ty)
                                  -> UniversalQuantificationResult {
        // NDM--this span is bogus.
        let self_region =
            polytype.region_param.map(
                |_r| self.inference_context.next_region_var_nb(dummy_sp()));

        let bounds_count = polytype.bounds.len();
        let type_parameters =
            self.inference_context.next_ty_vars(bounds_count);

        let substitutions = {
            self_r: self_region,
            self_ty: None,
            tps: type_parameters
        };
        let monotype = subst(self.crate_context.tcx,
                             &substitutions,
                             polytype.ty);

        // Get our type parameters back.
        let { self_r: _, self_ty: _, tps: type_parameters } = substitutions;

        UniversalQuantificationResult {
            monotype: monotype,
            type_variables: move type_parameters,
            bounds: polytype.bounds
        }
    }

    fn can_unify_universally_quantified(a: &a/UniversalQuantificationResult,
                                        b: &a/UniversalQuantificationResult)
                                     -> bool {
        let mut might_unify = true;
        let _ = do self.inference_context.probe {
            let result = self.inference_context.sub(true, dummy_sp())
                                               .tys(a.monotype, b.monotype);
            if result.is_ok() {
                // Check to ensure that each parameter binding respected its
                // kind bounds.
                for [ a, b ].each |result| {
                    for vec::each2(result.type_variables, *result.bounds)
                            |ty_var, bounds| {
                        match resolve_type(self.inference_context,
                                           *ty_var,
                                           resolve_nested_tvar) {
                            Ok(resolved_ty) => {
                                for bounds.each |bound| {
                                    match *bound {
                                        bound_copy => {
                                            let kind = type_kind_ext(
                                                self.inference_context.tcx,
                                                resolved_ty,
                                                true);
                                            if !kind_can_be_copied(kind) {
                                                might_unify = false;
                                                break;
                                            }
                                        }

                                        // XXX: We could be smarter here.
                                        // Check to see whether owned, send,
                                        // const, trait param bounds could
                                        // possibly unify.
                                        _ => {}
                                    }
                                }
                            }
                            Err(*) => {
                                // Conservatively assume it might unify.
                            }
                        }
                    }
                }
            } else {
                might_unify = false;
            }

            result
        };
        might_unify
    }

    fn get_self_type_for_implementation(implementation: @Impl)
                                     -> ty_param_bounds_and_ty {
        return self.crate_context.tcx.tcache.get(implementation.did);
    }

    // Privileged scope checking
    fn check_privileged_scopes(crate: @crate) {
        visit_crate(*crate, (), mk_vt(@Visitor {
            visit_item: |item, _context, visitor| {
                match /*bad*/copy item.node {
                    item_mod(module_) => {
                        // Then visit the module items.
                        visit_mod(module_, item.span, item.id, (), visitor);
                    }
                    item_impl(_, opt_trait, _, _) => {
                        match self.base_type_def_ids.find(
                            local_def(item.id)) {

                            None => {
                                // Nothing to do.
                            }
                            Some(base_type_def_id) => {
                                // Check to see whether the implementation is
                                // in the same crate as its base type.

                                if base_type_def_id.crate == local_crate {
                                    // Record that this implementation is OK.
                                    self.privileged_implementations.insert
                                        (item.id, ());
                                } else {
                                    // This implementation is not in scope of
                                    // its base type. This still might be OK
                                    // if the traits are defined in the same
                                    // crate.

                                  match opt_trait {
                                    None => {
                                        // There is no trait to implement, so
                                        // this is an error.

                                        let session =
                                            self.crate_context.tcx.sess;
                                        session.span_err(item.span,
                                                         ~"cannot implement \
                                                          inherent methods \
                                                          for a type outside \
                                                          the crate the type \
                                                          was defined in; \
                                                          define and \
                                                          implement a trait \
                                                          or new type \
                                                          instead");
                                    }
                                    _ => ()
                                  }

                                  do opt_trait.iter() |trait_ref| {
                                        // This is OK if and only if the
                                        // trait was defined in this
                                        // crate.

                                        let trait_def_id =
                                            self.trait_ref_to_trait_def_id(
                                                *trait_ref);

                                        if trait_def_id.crate != local_crate {
                                            let session =
                                                self.crate_context.tcx.sess;
                                            session.span_err(item.span,
                                                             ~"cannot \
                                                               provide an \
                                                               extension \
                                                               implementa\
                                                                  tion \
                                                               for a trait \
                                                               not defined \
                                                               in this \
                                                               crate");
                                        }
                                    }
                                }
                            }
                        }

                        visit_item(item, (), visitor);
                    }
                    _ => {
                        visit_item(item, (), visitor);
                    }
                }
            },
            .. *default_visitor()
        }));
    }

    fn trait_ref_to_trait_def_id(trait_ref: @trait_ref) -> def_id {
        let def_map = self.crate_context.tcx.def_map;
        let trait_def = def_map.get(trait_ref.ref_id);
        let trait_id = def_id_of_def(trait_def);
        return trait_id;
    }

    // This check doesn't really have anything to do with coherence. It's
    // here for historical reasons
    fn please_check_that_trait_methods_are_implemented(
        all_methods: &mut ~[@MethodInfo],
        trait_did: def_id,
        trait_ref_span: span) {

        let tcx = self.crate_context.tcx;

        let mut provided_names = send_map::linear::LinearSet::new();
        // Implemented methods
        for uint::range(0, all_methods.len()) |i| {
            provided_names.insert(all_methods[i].ident);
        }
        // Default methods
        for ty::provided_trait_methods(tcx, trait_did).each |ident| {
            provided_names.insert(*ident);
        }

        for (*ty::trait_methods(tcx, trait_did)).each |method| {
            if provided_names.contains(&method.ident) { loop; }

            tcx.sess.span_err(trait_ref_span,
                              fmt!("missing method `%s`",
                                   tcx.sess.str_of(method.ident)));
        }
    }

    // Converts an implementation in the AST to an Impl structure.
    fn create_impl_from_item(item: @item) -> @Impl {
        fn add_provided_methods(all_methods: &mut ~[@MethodInfo],
                                all_provided_methods: ~[@ProvidedMethodInfo],
                                sess: driver::session::Session) {
            for all_provided_methods.each |provided_method| {
                debug!(
                    "(creating impl) adding provided method `%s` to impl",
                    sess.str_of(provided_method.method_info.ident));
                vec::push(&mut *all_methods, provided_method.method_info);
            }
        }

        match /*bad*/copy item.node {
            item_impl(_, trait_refs, _, ast_methods) => {
                let mut methods = ~[];
                for ast_methods.each |ast_method| {
                    methods.push(method_to_MethodInfo(*ast_method));
                }

                // Check that we have implementations of every trait method
                for trait_refs.each |trait_ref| {
                    let trait_did =
                        self.trait_ref_to_trait_def_id(*trait_ref);
                    self.please_check_that_trait_methods_are_implemented(
                        &mut methods,
                        trait_did,
                        trait_ref.path.span);
                }

                // For each trait that the impl implements, see which
                // methods are provided.  For each of those methods,
                // if a method of that name is not inherent to the
                // impl, use the provided definition in the trait.
                for trait_refs.each |trait_ref| {
                    let trait_did =
                        self.trait_ref_to_trait_def_id(*trait_ref);

                    match self.crate_context.tcx
                              .provided_methods
                              .find(local_def(item.id)) {
                        None => {
                            debug!("(creating impl) trait with node_id `%d` \
                                    has no provided methods", trait_did.node);
                            /* fall through */
                        }
                        Some(all_provided) => {
                            debug!("(creating impl) trait with node_id `%d` \
                                    has provided methods", trait_did.node);
                            // Add all provided methods.
                            add_provided_methods(
                                &mut methods,
                                all_provided.get(),
                                self.crate_context.tcx.sess);
                        }
                    }
                }

                return @{
                    did: local_def(item.id),
                    ident: item.ident,
                    methods: methods
                };
            }
            _ => {
                self.crate_context.tcx.sess.span_bug(item.span,
                                                     ~"can't convert a \
                                                       non-impl to an impl");
            }
        }
    }

    fn span_of_impl(implementation: @Impl) -> span {
        assert implementation.did.crate == local_crate;
        match self.crate_context.tcx.items.find(implementation.did.node) {
            Some(node_item(item, _)) => {
                return item.span;
            }
            _ => {
                self.crate_context.tcx.sess.bug(~"span_of_impl() called on \
                                                  something that wasn't an \
                                                  impl!");
            }
        }
    }

    // External crate handling

    fn add_impls_for_module(impls_seen: HashMap<def_id,()>,
                            crate_store: CStore,
                            module_def_id: def_id) {

        let implementations = get_impls_for_mod(crate_store,
                                                module_def_id,
                                                None);
        for (*implementations).each |implementation| {
            debug!("coherence: adding impl from external crate: %s",
                   ty::item_path_str(self.crate_context.tcx,
                                     implementation.did));

            // Make sure we don't visit the same implementation
            // multiple times.
            match impls_seen.find(implementation.did) {
                None => {
                    // Good. Continue.
                    impls_seen.insert(implementation.did, ());
                }
                Some(_) => {
                    // Skip this one.
                    loop;
                }
            }

            let self_type = lookup_item_type(self.crate_context.tcx,
                                             implementation.did);
            let associated_traits = get_impl_traits(self.crate_context.tcx,
                                                    implementation.did);

            // Do a sanity check to make sure that inherent methods have base
            // types.

            if associated_traits.len() == 0 {
                match get_base_type_def_id(self.inference_context,
                                           dummy_sp(),
                                           self_type.ty) {
                    None => {
                        let session = self.crate_context.tcx.sess;
                        session.bug(fmt!(
                            "no base type for external impl \
                             with no trait: %s (type %s)!",
                             session.str_of(implementation.ident),
                             ty_to_str(self.crate_context.tcx,self_type.ty)));
                    }
                    Some(_) => {
                        // Nothing to do.
                    }
                }
            }

            // Record all the trait methods.
            for associated_traits.each |trait_type| {
                match get(*trait_type).sty {
                    ty_trait(trait_id, _, _) => {
                        self.add_trait_method(trait_id, *implementation);
                    }
                    _ => {
                        self.crate_context.tcx.sess.bug(~"trait type \
                                                          returned is not a \
                                                          trait");
                    }
                }
            }

            // Add the implementation to the mapping from
            // implementation to base type def ID, if there is a base
            // type for this implementation.

            match get_base_type_def_id(self.inference_context,
                                     dummy_sp(),
                                     self_type.ty) {
                None => {
                    // Nothing to do.
                }
                Some(base_type_def_id) => {
                    self.add_inherent_method(base_type_def_id,
                                             *implementation);

                    self.base_type_def_ids.insert(implementation.did,
                                                  base_type_def_id);
                }
            }
        }
    }

    fn add_default_methods_for_external_trait(trait_def_id: ast::def_id) {
        let tcx = self.crate_context.tcx;
        let pmm = tcx.provided_methods;

        if pmm.contains_key(trait_def_id) { return; }

        debug!("(adding default methods for trait) processing trait");

        for csearch::get_provided_trait_methods(tcx, trait_def_id).each
                                                |trait_method_info| {
            debug!("(adding default methods for trait) found default method");

            // Create a new def ID for this provided method.
            let parse_sess = &self.crate_context.tcx.sess.parse_sess;
            let new_did = local_def(parse::next_node_id(*parse_sess));

            let provided_method_info =
                @ProvidedMethodInfo {
                    method_info: @{
                        did: new_did,
                        n_tps: trait_method_info.ty.tps.len(),
                        ident: trait_method_info.ty.ident,
                        self_type: trait_method_info.ty.self_ty
                    },
                    trait_method_def_id: trait_method_info.def_id
                };

            let method_infos = @DVec();
            method_infos.push(provided_method_info);
            pmm.insert(trait_def_id, method_infos);
        }
    }

    // Adds implementations and traits from external crates to the coherence
    // info.
    fn add_external_crates() {
        let impls_seen = HashMap();

        let crate_store = self.crate_context.tcx.sess.cstore;
        do iter_crate_data(crate_store) |crate_number, _crate_metadata| {
            self.add_impls_for_module(impls_seen,
                                      crate_store,
                                      def_id { crate: crate_number,
                                               node: 0 });

            for each_path(crate_store, crate_number) |_p, def_like| {
                match def_like {
                    dl_def(def_mod(def_id)) => {
                        self.add_impls_for_module(impls_seen,
                                                  crate_store,
                                                  def_id);
                    }
                    dl_def(def_ty(def_id)) => {
                        let tcx = self.crate_context.tcx;
                        let polytype = csearch::get_type(tcx, def_id);
                        match ty::get(polytype.ty).sty {
                            ty::ty_trait(*) => {
                                self.add_default_methods_for_external_trait(
                                    def_id);
                            }
                            _ => {}
                        }
                    }
                    dl_def(_) | dl_impl(_) | dl_field => {
                        // Skip this.
                        loop;
                    }
                }
            }
        }
    }

    //
    // Destructors
    //

    fn populate_destructor_table() {
        let coherence_info = &self.crate_context.coherence_info;
        let tcx = self.crate_context.tcx;
        let drop_trait = tcx.lang_items.drop_trait();
        let impls_opt = coherence_info.extension_methods.find(drop_trait);

        let impls;
        match impls_opt {
            None => return, // No types with (new-style) destructors present.
            Some(found_impls) => impls = found_impls
        }

        for impls.each |impl_info| {
            if impl_info.methods.len() < 1 {
                // We'll error out later. For now, just don't ICE.
                loop;
            }
            let method_def_id = impl_info.methods[0].did;

            let self_type = self.get_self_type_for_implementation(*impl_info);
            match ty::get(self_type.ty).sty {
                ty::ty_struct(type_def_id, _) => {
                    tcx.destructor_for_type.insert(type_def_id,
                                                   method_def_id);
                    tcx.destructors.insert(method_def_id, ());
                }
                _ => {
                    // Destructors only work on nominal types.
                    if impl_info.did.crate == ast::local_crate {
                        match tcx.items.find(impl_info.did.node) {
                            Some(ast_map::node_item(@ref item, _)) => {
                                tcx.sess.span_err((*item).span,
                                                  ~"the Drop trait may only \
                                                    be implemented on \
                                                    structures");
                            }
                            _ => {
                                tcx.sess.bug(~"didn't find impl in ast map");
                            }
                        }
                    } else {
                        tcx.sess.bug(~"found external impl of Drop trait on \
                                       something other than a struct");
                    }
                }
            }
        }
    }
}

fn check_coherence(crate_context: @crate_ctxt, crate: @crate) {
    let coherence_checker = @CoherenceChecker(crate_context);
    (*coherence_checker).check_coherence(crate);
}

