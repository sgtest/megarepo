/*

# Collect phase

The collect phase of type check has the job of visiting all items,
determining their type, and writing that type into the `tcx.tcache`
table.  Despite its name, this table does not really operate as a
*cache*, at least not for the types of items defined within the
current crate: we assume that after the collect phase, the types of
all local items will be present in the table.

Unlike most of the types that are present in Rust, the types computed
for each item are in fact polytypes.  In "layman's terms", this means
that they are generic types that may have type parameters (more
mathematically phrased, they are universally quantified over a set of
type parameters).  Polytypes are represented by an instance of
`ty::ty_param_bounds_and_ty`.  This combines the core type along with
a list of the bounds for each parameter.  Type parameters themselves
are represented as `ty_param()` instances.

*/

import astconv::{ast_conv, ty_of_fn_decl, ty_of_arg, ast_ty_to_ty};
import rscope::*;

fn collect_item_types(ccx: @crate_ctxt, crate: @ast::crate) {

    // FIXME (#2592): hooking into the "intrinsic" root module is crude.
    // There ought to be a better approach. Attributes?

    for crate.node.module.items.each |crate_item| {
        if *crate_item.ident == ~"intrinsic" {
            alt crate_item.node {
              ast::item_mod(m) {
                for m.items.each |intrinsic_item| {
                    let def_id = { crate: ast::local_crate,
                                  node: intrinsic_item.id };
                    let substs = {self_r: none, self_ty: none, tps: ~[]};

                    alt intrinsic_item.node {

                      ast::item_trait(_, _) {
                        let ty = ty::mk_trait(ccx.tcx, def_id, substs);
                        ccx.tcx.intrinsic_defs.insert
                            (intrinsic_item.ident, (def_id, ty));
                      }

                      ast::item_enum(_, _) {
                        let ty = ty::mk_enum(ccx.tcx, def_id, substs);
                        ccx.tcx.intrinsic_defs.insert
                            (intrinsic_item.ident, (def_id, ty));
                      }

                     _ { }
                    }
                }
              }
              _ { }
            }
            break;
        }
    }

    visit::visit_crate(*crate, (), visit::mk_simple_visitor(@{
        visit_item: |a|convert(ccx, a),
        visit_foreign_item: |a|convert_foreign(ccx, a)
        with *visit::default_simple_visitor()
    }));
}

impl methods for @crate_ctxt {
    fn to_ty<RS: region_scope copy owned>(
        rs: RS, ast_ty: @ast::ty) -> ty::t {

        ast_ty_to_ty(self, rs, ast_ty)
    }
}

impl of ast_conv for @crate_ctxt {
    fn tcx() -> ty::ctxt { self.tcx }
    fn ccx() -> @crate_ctxt { self }

    fn get_item_ty(id: ast::def_id) -> ty::ty_param_bounds_and_ty {
        if id.crate != ast::local_crate {
            csearch::get_type(self.tcx, id)
        } else {
            alt self.tcx.items.find(id.node) {
              some(ast_map::node_item(item, _)) {
                ty_of_item(self, item)
              }
              some(ast_map::node_foreign_item(foreign_item, _, _)) {
                ty_of_foreign_item(self, foreign_item)
              }
              x {
                self.tcx.sess.bug(fmt!{"unexpected sort of item \
                                        in get_item_ty(): %?", x});
              }
            }
        }
    }

    fn ty_infer(span: span) -> ty::t {
        self.tcx.sess.span_bug(span,
                               ~"found `ty_infer` in unexpected place");
    }
}

fn get_enum_variant_types(ccx: @crate_ctxt,
                          enum_ty: ty::t,
                          variants: ~[ast::variant],
                          ty_params: ~[ast::ty_param],
                          rp: bool) {
    let tcx = ccx.tcx;

    // Create a set of parameter types shared among all the variants.
    for variants.each |variant| {
        // Nullary enum constructors get turned into constants; n-ary enum
        // constructors get turned into functions.
        let result_ty = if vec::len(variant.node.args) == 0u {
            enum_ty
        } else {
            let rs = type_rscope(rp);
            let args = variant.node.args.map(|va| {
                let arg_ty = ccx.to_ty(rs, va.ty);
                {mode: ast::expl(ast::by_copy), ty: arg_ty}
            });
            ty::mk_fn(tcx, {purity: ast::pure_fn,
                            proto: ast::proto_box,
                            inputs: args,
                            output: enum_ty,
                            ret_style: ast::return_val})
        };
        let tpt = {bounds: ty_param_bounds(ccx, ty_params),
                   rp: rp,
                   ty: result_ty};
        tcx.tcache.insert(local_def(variant.node.id), tpt);
        write_ty_to_tcx(tcx, variant.node.id, result_ty);
    }
}

fn ensure_trait_methods(ccx: @crate_ctxt, id: ast::node_id) {
    fn store_methods<T>(ccx: @crate_ctxt, id: ast::node_id,
                        stuff: ~[T], f: fn@(T) -> ty::method) {
        ty::store_trait_methods(ccx.tcx, id, @vec::map(stuff, f));
    }

    let tcx = ccx.tcx;
    let rp = tcx.region_paramd_items.contains_key(id);
    alt check tcx.items.get(id) {
      ast_map::node_item(@{node: ast::item_trait(_, ms), _}, _) {
        store_methods::<ast::trait_method>(ccx, id, ms, |m| {
            alt m {
              required(ty_m) {
                ty_of_ty_method(ccx, ty_m, rp)
              }
              provided(m) {
                ty_of_method(ccx, m, rp)
              }
            }
        });
      }
      ast_map::node_item(@{node: ast::item_class(_,_,its,_,_), _}, _) {
        let (_,ms) = split_class_items(its);
        // All methods need to be stored, since lookup_method
        // relies on the same method cache for self-calls
        store_methods::<@ast::method>(ccx, id, ms, |m| {
            ty_of_method(ccx, m, rp)
        });
      }
    }
}

/**
 * Checks that a method from an impl/class conforms to the signature of
 * the same method as declared in the trait.
 *
 * # Parameters
 *
 * - impl_m: the method in the impl
 * - impl_tps: the type params declared on the impl itself (not the method!)
 * - if_m: the method in the trait
 * - if_substs: the substitutions used on the type of the trait
 * - self_ty: the self type of the impl
 */
fn compare_impl_method(tcx: ty::ctxt, sp: span,
                       impl_m: ty::method, impl_tps: uint,
                       if_m: ty::method, if_substs: ty::substs,
                       self_ty: ty::t) {

    if impl_m.tps != if_m.tps {
        tcx.sess.span_err(sp, ~"method `" + *if_m.ident +
                          ~"` has an incompatible set of type parameters");
        ret;
    }

    if vec::len(impl_m.fty.inputs) != vec::len(if_m.fty.inputs) {
        tcx.sess.span_err(sp,fmt!{"method `%s` has %u parameters \
                                   but the trait has %u",
                                  *if_m.ident,
                                  vec::len(impl_m.fty.inputs),
                                  vec::len(if_m.fty.inputs)});
        ret;
    }

    // Perform substitutions so that the trait/impl methods are expressed
    // in terms of the same set of type/region parameters:
    // - replace trait type parameters with those from `if_substs`
    // - replace method parameters on the trait with fresh, dummy parameters
    //   that correspond to the parameters we will find on the impl
    // - replace self region with a fresh, dummy region
    let dummy_self_r = ty::re_free(0, ty::br_self);
    let impl_fty = {
        let impl_fty = ty::mk_fn(tcx, impl_m.fty);
        replace_bound_self(tcx, impl_fty, dummy_self_r)
    };
    let if_fty = {
        let dummy_tps = do vec::from_fn((*if_m.tps).len()) |i| {
            // hack: we don't know the def id of the impl tp, but it
            // is not important for unification
            ty::mk_param(tcx, i + impl_tps, {crate: 0, node: 0})
        };
        let substs = {
            self_r: some(dummy_self_r),
            self_ty: some(self_ty),
            tps: vec::append(if_substs.tps, dummy_tps)
        };
        let if_fty = ty::mk_fn(tcx, if_m.fty);
        ty::subst(tcx, substs, if_fty)
    };
    require_same_types(
        tcx, none, sp, impl_fty, if_fty,
        || ~"method `" + *if_m.ident + ~"` has an incompatible type");
    ret;

    // Replaces bound references to the self region with `with_r`.
    fn replace_bound_self(tcx: ty::ctxt, ty: ty::t,
                          with_r: ty::region) -> ty::t {
        do ty::fold_regions(tcx, ty) |r, _in_fn| {
            if r == ty::re_bound(ty::br_self) {with_r} else {r}
        }
    }
}

fn check_methods_against_trait(ccx: @crate_ctxt,
                               tps: ~[ast::ty_param],
                               rp: bool,
                               selfty: ty::t,
                               a_trait_ty: @ast::trait_ref,
                               ms: ~[converted_method]) {

    let tcx = ccx.tcx;
    let (did, tpt) = instantiate_trait_ref(ccx, a_trait_ty, rp);
    if did.crate == ast::local_crate {
        ensure_trait_methods(ccx, did.node);
    }
    for vec::each(*ty::trait_methods(tcx, did)) |if_m| {
        alt vec::find(ms, |m| if_m.ident == m.mty.ident) {
          some({mty: m, id, span}) {
            if m.purity != if_m.purity {
                ccx.tcx.sess.span_err(
                    span, fmt!{"method `%s`'s purity does \
                                not match the trait method's \
                                purity", *m.ident});
            }
            compare_impl_method(
                ccx.tcx, span, m, vec::len(tps),
                if_m, tpt.substs, selfty);
          }
          none {
            // FIXME (#2794): if there's a default impl in the trait,
            // use that.

            tcx.sess.span_err(
                a_trait_ty.path.span,
                fmt!{"missing method `%s`", *if_m.ident});
          }
        } // alt
    } // |if_m|
} // fn

fn convert_class_item(ccx: @crate_ctxt,
                      rp: bool,
                      bounds: @~[ty::param_bounds],
                      v: ast_util::ivar) {
    let tt = ccx.to_ty(type_rscope(rp), v.ty);
    write_ty_to_tcx(ccx.tcx, v.id, tt);
    /* add the field to the tcache */
    ccx.tcx.tcache.insert(local_def(v.id), {bounds: bounds, rp: rp, ty: tt});
}

type converted_method = {mty: ty::method, id: ast::node_id, span: span};

fn convert_methods(ccx: @crate_ctxt,
                   ms: ~[@ast::method],
                   rp: bool,
                   rcvr_bounds: @~[ty::param_bounds],
                   self_ty: ty::t) -> ~[converted_method] {

    let tcx = ccx.tcx;
    do vec::map(ms) |m| {
        write_ty_to_tcx(tcx, m.self_id, self_ty);
        let bounds = ty_param_bounds(ccx, m.tps);
        let mty = ty_of_method(ccx, m, rp);
        let fty = ty::mk_fn(tcx, mty.fty);
        tcx.tcache.insert(
            local_def(m.id),

            // n.b.: the type of a method is parameterized by both
            // the tps on the receiver and those on the method itself
            {bounds: @(vec::append(*rcvr_bounds, *bounds)), rp: rp, ty: fty});
        write_ty_to_tcx(tcx, m.id, fty);
        {mty: mty, id: m.id, span: m.span}
    }
}

fn convert(ccx: @crate_ctxt, it: @ast::item) {
    let tcx = ccx.tcx;
    let rp = tcx.region_paramd_items.contains_key(it.id);
    debug!{"convert: item %s with id %d rp %b", *it.ident, it.id, rp};
    alt it.node {
      // These don't define types.
      ast::item_foreign_mod(_) | ast::item_mod(_) {}
      ast::item_enum(variants, ty_params) {
        let tpt = ty_of_item(ccx, it);
        write_ty_to_tcx(tcx, it.id, tpt.ty);
        get_enum_variant_types(ccx, tpt.ty, variants, ty_params, rp);
      }
      ast::item_impl(tps, trt, selfty, ms) {
        let i_bounds = ty_param_bounds(ccx, tps);
        let selfty = ccx.to_ty(type_rscope(rp), selfty);
        write_ty_to_tcx(tcx, it.id, selfty);
        tcx.tcache.insert(local_def(it.id),
                          {bounds: i_bounds,
                           rp: rp,
                           ty: selfty});

        let cms = convert_methods(ccx, ms, rp, i_bounds, selfty);
        for trt.each |t| {
            check_methods_against_trait(ccx, tps, rp, selfty, t, cms);
        }
      }
      ast::item_trait(*) {
        let tpt = ty_of_item(ccx, it);
        debug!{"item_trait(it.id=%d, tpt.ty=%s)",
               it.id, ty_to_str(tcx, tpt.ty)};
        write_ty_to_tcx(tcx, it.id, tpt.ty);
        ensure_trait_methods(ccx, it.id);
      }
      ast::item_class(tps, traits, members, m_ctor, m_dtor) {
        // Write the class type
        let tpt = ty_of_item(ccx, it);
        write_ty_to_tcx(tcx, it.id, tpt.ty);
        tcx.tcache.insert(local_def(it.id), tpt);

        do option::iter(m_ctor) |ctor| {
            // Write the ctor type
            let t_args = ctor.node.dec.inputs.map(
                |a| ty_of_arg(ccx, type_rscope(rp), a, none) );
            let t_res = ty::mk_class(
                tcx, local_def(it.id),
                {self_r: if rp {some(ty::re_bound(ty::br_self))} else {none},
                 self_ty: none,
                 tps: ty::ty_params_to_tys(tcx, tps)});
            let t_ctor = ty::mk_fn(
                tcx, {purity: ast::impure_fn,
                      proto: ast::proto_any,
                      inputs: t_args,
                      output: t_res,
                      ret_style: ast::return_val});
            write_ty_to_tcx(tcx, ctor.node.id, t_ctor);
            tcx.tcache.insert(local_def(ctor.node.id),
                              {bounds: tpt.bounds,
                               rp: rp,
                               ty: t_ctor});
        }

        do option::iter(m_dtor) |dtor| {
            // Write the dtor type
            let t_dtor = ty::mk_fn(
                tcx,
                ty_of_fn_decl(ccx, type_rscope(rp), ast::proto_any,
                              ast_util::dtor_dec(), none));
            write_ty_to_tcx(tcx, dtor.node.id, t_dtor);
            tcx.tcache.insert(local_def(dtor.node.id),
                              {bounds: tpt.bounds,
                               rp: rp,
                               ty: t_dtor});
        };
        ensure_trait_methods(ccx, it.id);

        // Write the type of each of the members
        let (fields, methods) = split_class_items(members);
        for fields.each |f| {
           convert_class_item(ccx, rp, tpt.bounds, f);
        }
        let {bounds, substs} = mk_substs(ccx, tps, rp);
        let selfty = ty::mk_class(tcx, local_def(it.id), substs);
        let cms = convert_methods(ccx, methods, rp, bounds, selfty);
        for traits.each |trt| {
            check_methods_against_trait(ccx, tps, rp, selfty, trt, cms);
            // trt.impl_id represents (class, trait) pair
            write_ty_to_tcx(tcx, trt.impl_id, tpt.ty);
            tcx.tcache.insert(local_def(trt.impl_id), tpt);
        }
      }
      _ {
        // This call populates the type cache with the converted type
        // of the item in passing. All we have to do here is to write
        // it into the node type table.
        let tpt = ty_of_item(ccx, it);
        write_ty_to_tcx(tcx, it.id, tpt.ty);
      }
    }
}
fn convert_foreign(ccx: @crate_ctxt, i: @ast::foreign_item) {
    // As above, this call populates the type table with the converted
    // type of the foreign item. We simply write it into the node type
    // table.
    let tpt = ty_of_foreign_item(ccx, i);
    alt i.node {
      ast::foreign_item_fn(_, _) {
        write_ty_to_tcx(ccx.tcx, i.id, tpt.ty);
        ccx.tcx.tcache.insert(local_def(i.id), tpt);
      }
    }
}

fn ty_of_method(ccx: @crate_ctxt,
                m: @ast::method,
                rp: bool) -> ty::method {
    {ident: m.ident,
     tps: ty_param_bounds(ccx, m.tps),
     fty: ty_of_fn_decl(ccx, type_rscope(rp), ast::proto_bare,
                        m.decl, none),
     purity: m.decl.purity,
     vis: m.vis}
}

fn ty_of_ty_method(self: @crate_ctxt,
                   m: ast::ty_method,
                   rp: bool) -> ty::method {
    {ident: m.ident,
     tps: ty_param_bounds(self, m.tps),
     fty: ty_of_fn_decl(self, type_rscope(rp), ast::proto_bare,
                                 m.decl, none),
     // assume public, because this is only invoked on trait methods
     purity: m.decl.purity, vis: ast::public}
}

/*
  Instantiates the path for the given trait reference, assuming that
  it's bound to a valid trait type. Returns the def_id for the defining
  trait. Fails if the type is a type other than an trait type.
 */
fn instantiate_trait_ref(ccx: @crate_ctxt, t: @ast::trait_ref, rp: bool)
    -> (ast::def_id, ty_param_substs_and_ty) {

    let sp = t.path.span, err = ~"can only implement trait types",
        sess = ccx.tcx.sess;

    let rscope = type_rscope(rp);

    alt lookup_def_tcx(ccx.tcx, t.path.span, t.ref_id) {
      ast::def_ty(t_id) {
        let tpt = astconv::ast_path_to_ty(ccx, rscope, t_id, t.path,
                                          t.ref_id);
        alt ty::get(tpt.ty).struct {
           ty::ty_trait(*) {
              (t_id, tpt)
           }
           _ { sess.span_fatal(sp, err); }
        }
      }
      _ {
          sess.span_fatal(sp, err);
      }
    }
}

fn ty_of_item(ccx: @crate_ctxt, it: @ast::item)
    -> ty::ty_param_bounds_and_ty {

    let def_id = local_def(it.id);
    let tcx = ccx.tcx;
    alt tcx.tcache.find(def_id) {
      some(tpt) { ret tpt; }
      _ {}
    }
    let rp = tcx.region_paramd_items.contains_key(it.id);
    alt it.node {
      ast::item_const(t, _) {
        let typ = ccx.to_ty(empty_rscope, t);
        let tpt = no_params(typ);
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_fn(decl, tps, _) {
        let bounds = ty_param_bounds(ccx, tps);
        let tofd = ty_of_fn_decl(ccx, empty_rscope, ast::proto_bare,
                                          decl, none);
        let tpt = {bounds: bounds,
                   rp: false, // functions do not have a self
                   ty: ty::mk_fn(ccx.tcx, tofd)};
        debug!{"type of %s (id %d) is %s",
               *it.ident, it.id, ty_to_str(tcx, tpt.ty)};
        ccx.tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_ty(t, tps) {
        alt tcx.tcache.find(local_def(it.id)) {
          some(tpt) { ret tpt; }
          none { }
        }

        let rp = tcx.region_paramd_items.contains_key(it.id);
        let tpt = {
            let ty = {
                let t0 = ccx.to_ty(type_rscope(rp), t);
                // Do not associate a def id with a named, parameterized type
                // like "foo<X>".  This is because otherwise ty_to_str will
                // print the name as merely "foo", as it has no way to
                // reconstruct the value of X.
                if !vec::is_empty(tps) { t0 } else {
                    ty::mk_with_id(tcx, t0, def_id)
                }
            };
            {bounds: ty_param_bounds(ccx, tps), rp: rp, ty: ty}
        };

        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_enum(_, tps) {
        // Create a new generic polytype.
        let {bounds, substs} = mk_substs(ccx, tps, rp);
        let t = ty::mk_enum(tcx, local_def(it.id), substs);
        let tpt = {bounds: bounds, rp: rp, ty: t};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_trait(tps, ms) {
        let {bounds, substs} = mk_substs(ccx, tps, rp);
        let t = ty::mk_trait(tcx, local_def(it.id), substs);
        let tpt = {bounds: bounds, rp: rp, ty: t};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_class(tps, _, _, _, _) {
          let {bounds,substs} = mk_substs(ccx, tps, rp);
          let t = ty::mk_class(tcx, local_def(it.id), substs);
          let tpt = {bounds: bounds, rp: rp, ty: t};
          tcx.tcache.insert(local_def(it.id), tpt);
          ret tpt;
      }
      ast::item_impl(*) | ast::item_mod(_) |
      ast::item_foreign_mod(_) { fail; }
      ast::item_mac(*) { fail ~"item macros unimplemented" }
    }
}

fn ty_of_foreign_item(ccx: @crate_ctxt, it: @ast::foreign_item)
    -> ty::ty_param_bounds_and_ty {
    alt it.node {
      ast::foreign_item_fn(fn_decl, params) {
        ret ty_of_foreign_fn_decl(ccx, fn_decl, params,
                                  local_def(it.id));
      }
    }
}
fn ty_param_bounds(ccx: @crate_ctxt,
                   params: ~[ast::ty_param]) -> @~[ty::param_bounds] {

    fn compute_bounds(ccx: @crate_ctxt,
                      param: ast::ty_param) -> ty::param_bounds {
        @do vec::flat_map(*param.bounds) |b| {
            alt b {
              ast::bound_send { ~[ty::bound_send] }
              ast::bound_copy { ~[ty::bound_copy] }
              ast::bound_const { ~[ty::bound_const] }
              ast::bound_owned { ~[ty::bound_owned] }
              ast::bound_trait(t) {
                let ity = ast_ty_to_ty(ccx, empty_rscope, t);
                alt ty::get(ity).struct {
                  ty::ty_trait(*) {
                    ~[ty::bound_trait(ity)]
                  }
                  _ {
                    ccx.tcx.sess.span_err(
                        t.span, ~"type parameter bounds must be \
                                  trait types");
                    ~[]
                  }
                }
              }
            }
        }
    }

    @do params.map |param| {
        alt ccx.tcx.ty_param_bounds.find(param.id) {
          some(bs) { bs }
          none {
            let bounds = compute_bounds(ccx, param);
            ccx.tcx.ty_param_bounds.insert(param.id, bounds);
            bounds
          }
        }
    }
}

fn ty_of_foreign_fn_decl(ccx: @crate_ctxt,
                        decl: ast::fn_decl,
                        ty_params: ~[ast::ty_param],
                        def_id: ast::def_id) -> ty::ty_param_bounds_and_ty {

    let bounds = ty_param_bounds(ccx, ty_params);
    let rb = in_binding_rscope(empty_rscope);
    let input_tys = decl.inputs.map(|a| ty_of_arg(ccx, rb, a, none) );
    let output_ty = ast_ty_to_ty(ccx, rb, decl.output);

    let t_fn = ty::mk_fn(ccx.tcx, {purity: decl.purity,
                                   proto: ast::proto_bare,
                                   inputs: input_tys,
                                   output: output_ty,
                                   ret_style: ast::return_val});
    let tpt = {bounds: bounds, rp: false, ty: t_fn};
    ccx.tcx.tcache.insert(def_id, tpt);
    ret tpt;
}

fn mk_ty_params(ccx: @crate_ctxt, atps: ~[ast::ty_param])
    -> {bounds: @~[ty::param_bounds], params: ~[ty::t]} {

    let mut i = 0u;
    let bounds = ty_param_bounds(ccx, atps);
    {bounds: bounds,
     params: vec::map(atps, |atp| {
         let t = ty::mk_param(ccx.tcx, i, local_def(atp.id));
         i += 1u;
         t
     })}
}

fn mk_substs(ccx: @crate_ctxt, atps: ~[ast::ty_param], rp: bool)
    -> {bounds: @~[ty::param_bounds], substs: ty::substs} {

    let {bounds, params} = mk_ty_params(ccx, atps);
    let self_r = if rp {some(ty::re_bound(ty::br_self))} else {none};
    {bounds: bounds, substs: {self_r: self_r, self_ty: none, tps: params}}
}
