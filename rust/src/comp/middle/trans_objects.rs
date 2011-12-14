// Translation of object-related things to LLVM IR.

import core::{str, option, vec};
import option::{none, some};

import lib::llvm::{llvm, True};
import lib::llvm::llvm::{TypeRef, ValueRef};

import back::abi;
import back::link::{mangle_internal_name_by_path,
                    mangle_internal_name_by_path_and_seq};
import syntax::{ast, ast_util};
import syntax::codemap::span;

import trans_common::*;
import trans::*;
import trans_build::*;

export trans_anon_obj;
export trans_obj;

// trans_obj: create an LLVM function that is the object constructor for the
// object being translated.
fn trans_obj(cx: @local_ctxt, sp: span, ob: ast::_obj, ctor_id: ast::node_id,
             ty_params: [ast::ty_param]) {

    // To make a function, we have to create a function context and, inside
    // that, a number of block contexts for which code is generated.
    let ccx = cx.ccx;
    let llctor_decl;
    alt ccx.item_ids.find(ctor_id) {
      some(x) { llctor_decl = x; }
      _ { cx.ccx.sess.span_fatal(sp, "unbound llctor_decl in trans_obj"); }
    }

    // Much like trans_fn, we must create an LLVM function, but since we're
    // starting with an ast::_obj rather than an ast::_fn, we have some setup
    // work to do.

    // The fields of our object will become the arguments to the function
    // we're creating.
    let fn_args: [ast::arg] = [];
    for f: ast::obj_field in ob.fields {
        fn_args += [{mode: ast::by_copy, ty: f.ty, ident: f.ident,
                     id: f.id}];
    }
    let fcx = new_fn_ctxt(cx, sp, llctor_decl);

    //  Create the first block context in the function and keep a handle on it
    //  to pass to finish_fn later.
    let bcx = new_top_block_ctxt(fcx);
    let lltop = bcx.llbb;

    // Both regular arguments and type parameters are handled here.
    create_llargs_for_fn_args(fcx, none::<ty::t>, fn_args, ty_params);
    let arg_tys: [ty::arg] = arg_tys_of_fn(ccx, ctor_id);
    bcx = copy_args_to_allocas(fcx, bcx, fn_args, arg_tys);

    // Pick up the type of this object by looking at our own output type, that
    // is, the output type of the object constructor we're building.
    let self_ty = ty::ret_ty_of_fn(ccx.tcx, ctor_id);

    // Set up the two-word pair that we're going to return from the object
    // constructor we're building.  The two elements of this pair will be a
    // vtable pointer and a body pointer.  (llretptr already points to the
    // place where this two-word pair should go; it was pre-allocated by the
    // caller of the function.)
    let pair = bcx.fcx.llretptr;

    // Grab onto the first and second elements of the pair.
    // abi::obj_field_vtbl and abi::obj_field_box simply specify words 0 and 1
    // of 'pair'.
    let pair_vtbl = GEPi(bcx, pair, [0, abi::obj_field_vtbl]);
    let pair_box = GEPi(bcx, pair, [0, abi::obj_field_box]);

    // Make a vtable for this object: a static array of pointers to functions.
    // It will be located in the read-only memory of the executable we're
    // creating and will contain ValueRefs for all of this object's methods.
    // create_vtbl returns a pointer to the vtable, which we store.
    let vtbl = create_vtbl(cx, sp, self_ty, ob, ty_params, none, []);
    vtbl = PointerCast(bcx, vtbl, T_ptr(T_empty_struct()));

    Store(bcx, vtbl, pair_vtbl);

    // Next we have to take care of the other half of the pair we're
    // returning: a boxed (reference-counted) tuple containing a tydesc,
    // typarams, and fields.
    let llbox_ty: TypeRef = T_ptr(T_empty_struct());

    if vec::len(ty_params) == 0u &&
           vec::len(arg_tys) == 0u {
        // If the object we're translating has no fields or type parameters,
        // there's not much to do.

        // Store null into pair, if no args or typarams.
        Store(bcx, C_null(llbox_ty), pair_box);
    } else {
        let obj_fields: [ty::t] = [];
        for a: ty::arg in arg_tys { obj_fields += [a.ty]; }

        let tps: [ty::t] = [];
        let tydesc_ty = ty::mk_type(ccx.tcx);
        for tp: ast::ty_param in ty_params { tps += [tydesc_ty]; }

        // Synthesize an object body type and hand it off to
        // trans_malloc_boxed, which allocates a box, including space for a
        // refcount.
        let body_ty: ty::t =
            create_object_body_type(ccx.tcx, obj_fields, tps, none);

        // We have to get this type descriptor now so that
        // trans_malloc_boxed() doesn't generate a type descriptor with the
        // wrong storage type and override the type descriptor we're about to
        // generate.
        let ti = none;
        let storage = tps_obj(vec::len(ty_params));
        let body_td = get_tydesc(bcx, body_ty, true, storage, ti).result;
        bcx = body_td.bcx;
        lazily_emit_tydesc_glue(bcx, abi::tydesc_field_drop_glue, ti);
        lazily_emit_tydesc_glue(bcx, abi::tydesc_field_free_glue, ti);

        let box = trans_malloc_boxed(bcx, body_ty);
        bcx = box.bcx;
        let body = box.body;

        // Put together a tydesc for the body, so that the object can later be
        // freed by calling through its tydesc.

        // Every object (not just those with type parameters) needs to have a
        // tydesc to describe its body, since all objects have unknown type to
        // the user of the object.  So the tydesc is needed to keep track of
        // the types of the object's fields, so that the fields can be freed
        // later.

        // postcondition on create_object_body_type?
        check type_is_tup_like(bcx, body_ty);
        let body_tydesc =
            GEP_tup_like(bcx, body_ty, body, [0, abi::obj_body_elt_tydesc]);
        bcx = body_tydesc.bcx;

        check type_is_tup_like(bcx, body_ty);
        let r =
            GEP_tup_like(bcx, body_ty, body, [0, abi::obj_body_elt_typarams]);
        bcx = r.bcx;
        let body_typarams = r.val;

        Store(bcx, body_td.val, body_tydesc.val);

        // Copy the object's type parameters and fields into the space we
        // allocated for the object body.  (This is something like saving the
        // lexical environment of a function in its closure: the "captured
        // typarams" are any type parameters that are passed to the object
        // constructor and are then available to the object's methods.
        // Likewise for the object's fields.)

        // Copy typarams into captured typarams.
        // TODO: can we just get typarams_ty out of body_ty instead?
        let typarams_ty: ty::t = ty::mk_tup(ccx.tcx, tps);
        let i: int = 0;
        for tp: ast::ty_param in ty_params {
            let typaram = bcx.fcx.lltydescs[i];
            // Silly check
            check type_is_tup_like(bcx, typarams_ty);
            let capture =
                GEP_tup_like(bcx, typarams_ty, body_typarams, [0, i]);
            bcx = capture.bcx;
            bcx = copy_val(bcx, INIT, capture.val, typaram, tydesc_ty);
            i += 1;
        }

        // Copy args into body fields.
        // how to get rid of this check?
        check type_is_tup_like(bcx, body_ty);
        let body_fields =
            GEP_tup_like(bcx, body_ty, body, [0, abi::obj_body_elt_fields]);
        bcx = body_fields.bcx;
        // TODO: can we just get fields_ty out of body_ty instead?
        let fields_ty = ty::mk_tup(ccx.tcx, obj_fields);
        i = 0;
        for f: ast::obj_field in ob.fields {
            alt bcx.fcx.llargs.find(f.id) {
              some(local_mem(arg)) {
                // Silly check
                check type_is_tup_like(bcx, fields_ty);
                let field =
                    GEP_tup_like(bcx, fields_ty, body_fields.val, [0, i]);
                bcx = memmove_ty(field.bcx, field.val, arg, arg_tys[i].ty);
                i += 1;
              }
            }
        }

        // Store box ptr in outer pair.
        let p = PointerCast(bcx, box.box, llbox_ty);
        Store(bcx, p, pair_box);
    }
    build_return(bcx);

    // Insert the mandatory first few basic blocks before lltop.
    finish_fn(fcx, lltop);
}

// trans_anon_obj: create and return a pointer to an object.  This code
// differs from trans_obj in that, rather than creating an object constructor
// function and putting it in the generated code as an object item, we are
// instead "inlining" the construction of the object and returning the object
// itself.
fn trans_anon_obj(bcx: @block_ctxt, sp: span, anon_obj: ast::anon_obj,
                  id: ast::node_id, dest: trans::dest) -> @block_ctxt {
    let bcx = bcx;
    if dest == trans::ignore {
        alt anon_obj.inner_obj {
          some(e) { ret trans::trans_expr(bcx, e, trans::ignore); }
          none. { ret bcx; }
        }
    }

    let ccx = bcx_ccx(bcx);

    // Fields.  FIXME (part of issue #538): Where do we fill in the field
    // *values* from the outer object?
    let additional_fields: [ast::anon_obj_field] = [];
    let additional_field_vals: [result] = [];
    let additional_field_tys: [ty::t] = [];
    alt anon_obj.fields {
      none. { }
      some(fields) {
        additional_fields = fields;
        for f: ast::anon_obj_field in fields {
            additional_field_tys += [node_id_type(ccx, f.id)];
            additional_field_vals += [trans_temp_expr(bcx, f.expr)];
        }
      }
    }

    // Get the type of the eventual entire anonymous object, possibly with
    // extensions.  NB: This type includes both inner and outer methods.
    let outer_obj_ty = ty::node_id_to_type(ccx.tcx, id);

    // Create a vtable for the anonymous object.

    // create_vtbl() wants an ast::_obj and all we have is an ast::anon_obj,
    // so we need to roll our own.  NB: wrapper_obj includes only outer
    // methods, not inner ones.
    let wrapper_obj: ast::_obj =
        {fields:
             vec::map(ast_util::obj_field_from_anon_obj_field,
                           additional_fields),
         methods: anon_obj.methods};

    let inner_obj_ty: ty::t;
    let vtbl;
    alt anon_obj.inner_obj {
      none. {
        // We need a dummy inner_obj_ty for setting up the object body later.
        inner_obj_ty = ty::mk_type(ccx.tcx);

        // If there's no inner_obj -- that is, if we're creating a new object
        // from nothing rather than extending an existing object -- then we
        // just pass the outer object to create_vtbl().  Our vtable won't need
        // to have any forwarding slots.
        vtbl =
            create_vtbl(bcx.fcx.lcx, sp, outer_obj_ty, wrapper_obj, [], none,
                        additional_field_tys);
      }
      some(e) {
        // TODO: What makes more sense to get the type of an expr -- calling
        // ty::expr_ty(ccx.tcx, e) on it or calling
        // ty::node_id_to_type(ccx.tcx, id) on its id?
        inner_obj_ty = ty::expr_ty(ccx.tcx, e);
        //inner_obj_ty = ty::node_id_to_type(ccx.tcx, e.id);

        // If there's a inner_obj, we pass its type along to create_vtbl().
        // Part of what create_vtbl() will do is take the set difference of
        // methods defined on the original and methods being added.  For every
        // method defined on the original that does *not* have one with a
        // matching name and type being added, we'll need to create a
        // forwarding slot.  And, of course, we need to create a normal vtable
        // entry for every method being added.
        vtbl =
            create_vtbl(bcx.fcx.lcx, sp, outer_obj_ty, wrapper_obj, [],
                        some(inner_obj_ty), additional_field_tys);
      }
    }

    vtbl = PointerCast(bcx, vtbl, T_ptr(T_empty_struct()));

    // Next we have to take care of the other half of the pair we're
    // returning: a boxed (reference-counted) tuple containing a tydesc,
    // typarams, fields, and a pointer to our inner_obj.
    let llbox_ty: TypeRef = T_ptr(T_empty_struct());

    let box = C_null(llbox_ty);
    if vec::len(additional_fields) > 0u || anon_obj.inner_obj != none {
        // Synthesize a type for the object body and hand it off to
        // trans_malloc_boxed, which allocates a box, including space for a
        // refcount.
        let body_ty: ty::t =
            create_object_body_type(ccx.tcx, additional_field_tys, [],
                                    some(inner_obj_ty));
        let box_r = trans_malloc_boxed(bcx, body_ty);
        box = box_r.box;
        bcx = box_r.bcx;
        add_clean_free(bcx, box, false);
        let body = box_r.body;

        // Put together a tydesc for the body, so that the object can later be
        // freed by calling through its tydesc.

        // Every object (not just those with type parameters) needs to have a
        // tydesc to describe its body, since all objects have unknown type to
        // the user of the object.  So the tydesc is needed to keep track of
        // the types of the object's fields, so that the fields can be freed
        // later.
        // postcondition on create_object_body_type?
        check type_is_tup_like(bcx, body_ty);
        let body_tydesc =
            GEP_tup_like(bcx, body_ty, body, [0, abi::obj_body_elt_tydesc]);
        bcx = body_tydesc.bcx;
        let ti = none;
        let body_td = get_tydesc(bcx, body_ty, true, tps_normal, ti).result;
        lazily_emit_tydesc_glue(bcx, abi::tydesc_field_drop_glue, ti);
        lazily_emit_tydesc_glue(bcx, abi::tydesc_field_free_glue, ti);
        bcx = body_td.bcx;
        Store(bcx, body_td.val, body_tydesc.val);

        // Copy the object's fields into the space we allocated for the object
        // body.  (This is something like saving the lexical environment of a
        // function in its closure: the fields were passed to the object
        // constructor and are now available to the object's methods.
        check type_is_tup_like(bcx, body_ty);
        let body_fields =
            GEP_tup_like(bcx, body_ty, body, [0, abi::obj_body_elt_fields]);
        bcx = body_fields.bcx;
        let i: int = 0;
        for f: ast::anon_obj_field in additional_fields {
            // FIXME (part of issue #538): make this work eventually, when we
            // have additional field exprs in the AST.
            load_if_immediate(bcx, additional_field_vals[i].val,
                              additional_field_tys[i]);
            let fields_ty: ty::t = ty::mk_tup(ccx.tcx, additional_field_tys);
            // Silly check
            check type_is_tup_like(bcx, fields_ty);
            let field = GEP_tup_like(bcx, fields_ty, body_fields.val, [0, i]);
            bcx = field.bcx;
            bcx =
                copy_val(bcx, INIT, field.val, additional_field_vals[i].val,
                         additional_field_tys[i]);
            i += 1;
        }

        // If there's a inner_obj, copy a pointer to it into the object's
        // body.
        alt anon_obj.inner_obj {
          none. { }
          some(e) {
            // If inner_obj (the object being extended) exists, translate it.
            // Translating inner_obj returns a ValueRef (pointer to a 2-word
            // value) wrapped in a result.
            check type_is_tup_like(bcx, body_ty);
            let {bcx: cx, val: body_inner_obj} = GEP_tup_like
                (bcx, body_ty, body, [0, abi::obj_body_elt_inner_obj]);
            bcx = trans_expr_save_in(cx, e, body_inner_obj);
          }
        }
        revoke_clean(bcx, box);
        box = PointerCast(bcx, box, llbox_ty);
    }
    let pair = trans::get_dest_addr(dest);
    let pair_vtbl = GEPi(bcx, pair, [0, abi::obj_field_vtbl]);
    Store(bcx, vtbl, pair_vtbl);
    let pair_box = GEPi(bcx, pair, [0, abi::obj_field_box]);
    Store(bcx, box, pair_box);
    ret bcx;
}

// Used only inside create_vtbl and create_backwarding_vtbl to distinguish
// different kinds of slots we'll have to create.
tag vtbl_mthd {

    // Normal methods are complete AST nodes, but for forwarding methods, the
    // only information we'll have about them is their type.
    normal_mthd(@ast::method);
    fwding_mthd(@ty::method);
}

// Alphabetize ast::methods by ident.  A helper for create_vtbl.
fn ast_mthd_lteq(&&a: @ast::method, &&b: @ast::method) -> bool {
    ret str::lteq(a.node.ident, b.node.ident);
}

// Alphabetize vtbl_mthds by ident.  A helper for create_vtbl.
fn vtbl_mthd_lteq(a: vtbl_mthd, b: vtbl_mthd) -> bool {
    alt a {
      normal_mthd(ma) {
        alt b {
          normal_mthd(mb) { ret str::lteq(ma.node.ident, mb.node.ident); }
          fwding_mthd(mb) { ret str::lteq(ma.node.ident, mb.ident); }
        }
      }
      fwding_mthd(ma) {
        alt b {
          normal_mthd(mb) { ret str::lteq(ma.ident, mb.node.ident); }
          fwding_mthd(mb) { ret str::lteq(ma.ident, mb.ident); }
        }
      }
    }
}

// filtering_fn: Used by create_vtbl to filter a list of methods to remove the
// ones that we don't need forwarding slots for.
fn filtering_fn(cx: @local_ctxt, m: vtbl_mthd, addtl_meths: [@ast::method]) ->
   option::t<vtbl_mthd> {

    // Since m is a fwding_mthd, and we're checking to see if it's in
    // addtl_meths (which only contains normal_mthds), we can't just check if
    // it's a member of addtl_meths.  Instead, we have to go through
    // addtl_meths and see if there's some method in it that has the same name
    // as m.
    alt m {
      fwding_mthd(fm) {
        for am: @ast::method in addtl_meths {
            if str::eq(am.node.ident, fm.ident) { ret none; }
        }
        ret some(fwding_mthd(fm));
      }
      normal_mthd(_) {
        cx.ccx.sess.bug("create_vtbl(): shouldn't be any \
                        normal_mthds in meths here");
      }
    }
}

// create_vtbl: Create a vtable for a regular object or for an outer anonymous
// object, and return a pointer to it.
fn create_vtbl(cx: @local_ctxt, sp: span, outer_obj_ty: ty::t, ob: ast::_obj,
               ty_params: [ast::ty_param], inner_obj_ty: option::t<ty::t>,
               additional_field_tys: [ty::t]) -> ValueRef {

    let llmethods: [ValueRef] = [];

    alt inner_obj_ty {
      none. {
        // We're creating a vtable for a regular object, or for an anonymous
        // object that doesn't extend an existing one.

        // Sort and process all the methods.
        let meths =
            std::sort::merge_sort(bind ast_mthd_lteq(_, _), ob.methods);

        for m: @ast::method in meths {
            llmethods +=
                [process_normal_mthd(cx, m, outer_obj_ty, ty_params)];
        }
      }
      some(inner_obj_ty) {
        // We're creating a vtable for an anonymous object that extends an
        // existing one.

        // The vtable needs to contain 'forwarding slots' for any methods that
        // were on the inner object and are not being overridden by the outer
        // one.  To find the set of methods that we need forwarding slots for,
        // we take the set difference of { methods on the original object }
        // and { methods being added, whether entirely new or overriding }.

        let meths: [vtbl_mthd] = [];

        // Gather up methods on the inner object.
        alt ty::struct(cx.ccx.tcx, inner_obj_ty) {
          ty::ty_obj(inner_obj_methods) {
            for m: ty::method in inner_obj_methods {
                meths += [fwding_mthd(@m)];
            }
          }
          _ {
            cx.ccx.sess.bug("create_vtbl(): trying to extend a \
                            non-object");
          }
        }

        // Filter out any methods that we don't need forwarding slots for
        // because they're being overridden.
        let f = bind filtering_fn(cx, _, ob.methods);
        meths = vec::filter_map(f, meths);

        // And now add the additional ones, both overriding ones and entirely
        // new ones.  These will just be normal methods.
        for m: @ast::method in ob.methods { meths += [normal_mthd(m)]; }

        // Sort all the methods and process them.
        meths =
            std::sort::merge_sort(bind vtbl_mthd_lteq(_, _), meths);

        // To create forwarding methods, we'll need a "backwarding" vtbl.  See
        // create_backwarding_vtbl and process_bkwding_method for details.
        let backwarding_vtbl: ValueRef =
            create_backwarding_vtbl(cx, sp, inner_obj_ty, outer_obj_ty);

        for m: vtbl_mthd in meths {
            alt m {
              normal_mthd(nm) {
                llmethods +=
                    [process_normal_mthd(cx, nm, outer_obj_ty, ty_params)];
              }
              fwding_mthd(fm) {
                llmethods +=
                    [process_fwding_mthd(cx, sp, fm, ty_params, inner_obj_ty,
                                         backwarding_vtbl,
                                         additional_field_tys)];
              }
            }
        }
      }
    }

    ret finish_vtbl(cx, llmethods, "vtbl");
}

// create_backwarding_vtbl: Create a vtable for the inner object of an
// anonymous object, so that any self-calls made from the inner object's
// methods get redirected appropriately.
fn create_backwarding_vtbl(cx: @local_ctxt, sp: span, inner_obj_ty: ty::t,
                           outer_obj_ty: ty::t) -> ValueRef {

    // This vtbl needs to have slots for all of the methods on an inner
    // object, and it needs to forward them to the corresponding slots on the
    // outer object.  All we know about either one are their types.

    let llmethods: [ValueRef] = [];
    let meths: [ty::method] = [];

    // Gather up methods on the inner object.
    alt ty::struct(cx.ccx.tcx, inner_obj_ty) {
      ty::ty_obj(inner_obj_methods) {
        for m: ty::method in inner_obj_methods { meths += [m]; }
      }
      _ {
        // Shouldn't happen.
        cx.ccx.sess.bug("create_backwarding_vtbl(): trying to extend a \
                            non-object");
      }
    }

    // Methods should have already been sorted, so no need to do so again.
    for m: ty::method in meths {
        // We pass outer_obj_ty to process_fwding_mthd() because it's the one
        // being forwarded to.
        llmethods += [process_bkwding_mthd(cx, sp, @m, [], outer_obj_ty, [])];
    }
    ret finish_vtbl(cx, llmethods, "backwarding_vtbl");
}

// finish_vtbl: Given a vector of vtable entries, create the table in
// read-only memory and return a pointer to it.
fn finish_vtbl(cx: @local_ctxt, llmethods: [ValueRef], name: str) ->
   ValueRef {
    let vtbl = C_struct(llmethods);
    let vtbl_name = mangle_internal_name_by_path(cx.ccx, cx.path + [name]);
    let gvar =
        str::as_buf(vtbl_name,
                    {|buf|
                        llvm::LLVMAddGlobal(cx.ccx.llmod, val_ty(vtbl), buf)
                    });
    llvm::LLVMSetInitializer(gvar, vtbl);
    llvm::LLVMSetGlobalConstant(gvar, True);
    llvm::LLVMSetLinkage(gvar,
                         lib::llvm::LLVMInternalLinkage as llvm::Linkage);
    ret gvar;
}

// begin_fn: Set up an LLVM function for backwarding and forwarding functions.
fn begin_fn(cx: @local_ctxt, sp: span, m: @ty::method,
            ty_params: [ast::ty_param], fn_name: str) -> ValueRef {

    // Create a local context that's aware of the name of the method we're
    // creating.
    let mcx: @local_ctxt = @{path: cx.path + ["method", m.ident] with *cx};

    // Make up a name for the function.
    let s: str =
        mangle_internal_name_by_path_and_seq(mcx.ccx, mcx.path, fn_name);

    // Get the function's type and declare it.
    let llfn_ty: TypeRef = type_of_meth(cx.ccx, sp, m, ty_params);
    let llfn: ValueRef = decl_internal_cdecl_fn(cx.ccx.llmod, s, llfn_ty);

    ret llfn;
}

// process_bkwding_mthd: Create the backwarding function that appears in a
// backwarding vtable slot.
//
// Backwarding functions are used in situations where method calls dispatch
// back through an outer object.  For example, suppose an inner object has
// methods foo and bar, and bar contains the call self.foo().  We extend that
// object with a foo method that overrides the inner foo.  Now, a call to
// outer.bar() should send us to to inner.bar() via a normal forwarding
// function, and then to self.foo().  But inner.bar() was already compiled
// under the assumption that self.foo() is inner.foo(), when we really want to
// reach outer.foo().  So, we give 'self' a vtable of backwarding functions,
// one for each method on inner, each of which takes all the same arguments as
// the corresponding method on inner does, calls that method on outer, and
// returns the value returned from that call.
fn process_bkwding_mthd(cx: @local_ctxt, sp: span, m: @ty::method,
                        ty_params: [ast::ty_param], outer_obj_ty: ty::t,
                        _additional_field_tys: [ty::t]) -> ValueRef {

    let llbackwarding_fn = begin_fn(cx, sp, m, ty_params, "backwarding_fn");
    let fcx = new_fn_ctxt(cx, sp, llbackwarding_fn);
    let bcx = new_top_block_ctxt(fcx);
    let lltop = bcx.llbb;

    // The self-object will arrive in the backwarding function via the
    // llenv argument, but we need to jump past the first item in the
    // self-stack to get to the one we really want.

    // Cast to self-stack's type.
    let llenv =
        PointerCast(bcx, fcx.llenv,
                    T_ptr(T_struct([cx.ccx.rust_object_type,
                                    T_ptr(cx.ccx.rust_object_type)])));
    let llself_obj_ptr = GEPi(bcx, llenv, [0, 1]);
    llself_obj_ptr = Load(bcx, llself_obj_ptr);

    // Cast it back to pointer-to-object-type, so LLVM won't complain.
    llself_obj_ptr =
        PointerCast(bcx, llself_obj_ptr, T_ptr(cx.ccx.rust_object_type));

    // The 'llretptr' that will arrive in the backwarding function we're
    // creating also needs to be the correct type.  Cast it to the method's
    // return type, if necessary.
    let llretptr = fcx.llretptr;
    let ccx = cx.ccx;
    if ty::type_contains_params(ccx.tcx, m.output) {
        let m_output = m.output;
        check non_ty_var(ccx, m_output);
        let llretty = type_of_inner(ccx, sp, m_output);
        llretptr = PointerCast(bcx, llretptr, T_ptr(llretty));
    }

    // Get the index of the method we want.
    let ix: uint = 0u;
    alt ty::struct(bcx_tcx(bcx), outer_obj_ty) {
      ty::ty_obj(methods) {
        ix = ty::method_idx(cx.ccx.sess, sp, m.ident, methods);
      }
      _ {
        // Shouldn't happen.
        cx.ccx.sess.bug("process_bkwding_mthd(): non-object type passed \
                        as outer_obj_ty");
      }
    }

    // Pick out the method being backwarded to from the outer object's vtable.
    let vtbl_type = T_ptr(T_array(T_ptr(T_nil()), ix + 1u));

    let llouter_obj_vtbl =
        GEPi(bcx, llself_obj_ptr, [0, abi::obj_field_vtbl]);
    llouter_obj_vtbl = Load(bcx, llouter_obj_vtbl);
    llouter_obj_vtbl = PointerCast(bcx, llouter_obj_vtbl, vtbl_type);

    let llouter_mthd =
        GEPi(bcx, llouter_obj_vtbl, [0, ix as int]);

    // Set up the outer method to be called.
    let llouter_mthd_ty = type_of_meth(bcx_ccx(bcx), sp, m, ty_params);
    llouter_mthd =
        PointerCast(bcx, llouter_mthd, T_ptr(T_ptr(llouter_mthd_ty)));
    llouter_mthd = Load(bcx, llouter_mthd);

    // Set up the three implicit arguments to the outer method we'll need to
    // call.
    let llouter_mthd_args: [ValueRef] = [llretptr, llself_obj_ptr];

    // Copy the explicit arguments that are being passed into the forwarding
    // function (they're in fcx.llargs) to llouter_mthd_args.

    let a: uint = 2u; // retptr, env come first
    for arg: ty::arg in m.inputs {
        llouter_mthd_args += [llvm::LLVMGetParam(llbackwarding_fn, a)];
        a += 1u;
    }

    // And, finally, call the outer method.
    Call(bcx, llouter_mthd, llouter_mthd_args);

    build_return(bcx);
    finish_fn(fcx, lltop);

    ret llbackwarding_fn;

}

// process_fwding_mthd: Create the forwarding function that appears in a
// vtable slot for method calls that need to forward to another object.  A
// helper function for create_vtbl.
//
// Forwarding functions are used for method calls that fall through to an
// inner object.  For example, suppose an inner object has method foo and we
// extend it with a method bar.  The only version of 'foo' we have is on the
// inner object, but we would like to be able to call outer.foo().  So we use
// a forwarding function to make the foo method available on the outer object.
// It takes all the same arguments as the foo method on the inner object does,
// calls inner.foo() with those arguments, and then returns the value returned
// from that call.  (The inner object won't exist until run-time, but we know
// its type statically.)
fn process_fwding_mthd(cx: @local_ctxt, sp: span, m: @ty::method,
                       ty_params: [ast::ty_param], inner_obj_ty: ty::t,
                       backwarding_vtbl: ValueRef,
                       additional_field_tys: [ty::t]) -> ValueRef unsafe {

    // Create a new function context and block context for the function,
    // holding onto a pointer to the first block.
    let llforwarding_fn = begin_fn(cx, sp, m, ty_params, "forwarding_fn");
    let fcx = new_fn_ctxt(cx, sp, llforwarding_fn);
    let bcx = new_top_block_ctxt(fcx);
    let lltop = bcx.llbb;

    // The outer object will arrive in the forwarding function via the llenv
    // argument.
    let llself_obj_ptr = fcx.llenv;

    // The 'llretptr' that will arrive in the forwarding function we're
    // creating also needs to be the correct type.  Cast it to the method's
    // return type, if necessary.
    let llretptr = fcx.llretptr;
    let ccx = cx.ccx;
    if ty::type_contains_params(ccx.tcx, m.output) {
        let m_output = m.output;
        check non_ty_var(ccx, m_output);
        let llretty = type_of_inner(ccx, sp, m_output);
        llretptr = PointerCast(bcx, llretptr, T_ptr(llretty));
    }

    // Now, we have to get the the inner_obj's vtbl out of the self_obj.  This
    // is a multi-step process:

    // First, grab the box out of the self_obj.  It contains a refcount and a
    // body.
    let llself_obj_box =
        GEPi(bcx, llself_obj_ptr, [0, abi::obj_field_box]);
    llself_obj_box = Load(bcx, llself_obj_box);

    let ccx = bcx_ccx(bcx);
    let llbox_ty = T_opaque_obj_ptr(ccx);
    llself_obj_box = PointerCast(bcx, llself_obj_box, llbox_ty);

    // Now, reach into the box and grab the body.
    let llself_obj_body =
        GEPi(bcx, llself_obj_box, [0, abi::box_rc_field_body]);

    // Now, we need to figure out exactly what type the body is supposed to be
    // cast to.
    let body_ty: ty::t =
        create_object_body_type(cx.ccx.tcx, additional_field_tys, [],
                                some(inner_obj_ty));
    // And cast to that type.
    // create_object_body_type maybe should have a postcondition...

    let cx_ccx = cx.ccx;
    check (type_has_static_size(cx_ccx, body_ty));

    llself_obj_body =
        PointerCast(bcx, llself_obj_body,
                    T_ptr(type_of(cx_ccx, sp, body_ty)));

    // Now, reach into the body and grab the inner_obj.
    check type_is_tup_like(bcx, body_ty);
    let llinner_obj =
        GEP_tup_like(bcx, body_ty, llself_obj_body,
                     [0, abi::obj_body_elt_inner_obj]);
    bcx = llinner_obj.bcx;

    // And, now, somewhere in inner_obj is a vtable with an entry for the
    // method we want.  First, pick out the vtable, and then pluck that
    // method's entry out of the vtable so that the forwarding function can
    // call it.
    let llinner_obj_vtbl =
        GEPi(bcx, llinner_obj.val, [0, abi::obj_field_vtbl]);
    llinner_obj_vtbl = Load(bcx, llinner_obj_vtbl);

    let llinner_obj_body =
        GEPi(bcx, llinner_obj.val, [0, abi::obj_field_box]);
    llinner_obj_body = Load(bcx, llinner_obj_body);

    // Get the index of the method we want.
    let ix: uint = 0u;
    alt ty::struct(bcx_tcx(bcx), inner_obj_ty) {
      ty::ty_obj(methods) {
        ix = ty::method_idx(cx.ccx.sess, sp, m.ident, methods);
      }
      _ {
        // Shouldn't happen.
        cx.ccx.sess.bug("process_fwding_mthd(): non-object type passed \
                        as target_obj_ty");
      }
    }

    // Pick out the original method from the vtable.
    let vtbl_type = T_ptr(T_array(T_ptr(T_nil()), ix + 1u));
    llinner_obj_vtbl = PointerCast(bcx, llinner_obj_vtbl, vtbl_type);

    let llorig_mthd =
        GEPi(bcx, llinner_obj_vtbl, [0, ix as int]);

    // Set up the original method to be called.
    let llorig_mthd_ty = type_of_meth(bcx_ccx(bcx), sp, m, ty_params);
    llorig_mthd = PointerCast(bcx, llorig_mthd, T_ptr(T_ptr(llorig_mthd_ty)));
    llorig_mthd = Load(bcx, llorig_mthd);

    // Set up the self-stack.
    let self_stack =
        alloca(bcx,
               T_struct([cx.ccx.rust_object_type,
                         T_ptr(cx.ccx.rust_object_type)]));
    self_stack =
        populate_self_stack(bcx, self_stack, llself_obj_ptr, backwarding_vtbl,
                            llinner_obj_body);

    // Cast self_stack back to pointer-to-object-type to make LLVM happy.
    self_stack = PointerCast(bcx, self_stack, T_ptr(cx.ccx.rust_object_type));

    // Set up the three implicit arguments to the original method we'll need
    // to call.
    let llorig_mthd_args: [ValueRef] = [llretptr, self_stack];

    // Copy the explicit arguments that are being passed into the forwarding
    // function (they're in fcx.llargs) to llorig_mthd_args.

    let a: uint = 2u; // retptr, env come first
    for arg: ty::arg in m.inputs {
        llorig_mthd_args += [llvm::LLVMGetParam(llforwarding_fn, a)];
        a += 1u;
    }

    // And, finally, call the original (inner) method.
    Call(bcx, llorig_mthd, llorig_mthd_args);

    build_return(bcx);
    finish_fn(fcx, lltop);

    ret llforwarding_fn;
}

// create_object_body_type: Synthesize a big structural tuple type for an
// object body: [tydesc, [typaram, ...], [field, ...], inner_obj].
fn create_object_body_type(tcx: ty::ctxt, fields_ty: [ty::t],
                           typarams_ty: [ty::t],
                           maybe_inner_obj_ty: option::t<ty::t>) -> ty::t {

    let tydesc_ty: ty::t = ty::mk_type(tcx);
    let typarams_ty_tup: ty::t = ty::mk_tup(tcx, typarams_ty);
    let fields_ty_tup: ty::t = ty::mk_tup(tcx, fields_ty);

    let body_ty: ty::t;
    alt maybe_inner_obj_ty {
      some(inner_obj_ty) {
        body_ty =
            ty::mk_tup(tcx,
                       [tydesc_ty, typarams_ty_tup, fields_ty_tup,
                        inner_obj_ty]);
      }
      none {
        body_ty =
            ty::mk_tup(tcx, [tydesc_ty, typarams_ty_tup, fields_ty_tup]);
      }
    }

    ret body_ty;
}

// process_normal_mthd: Create the contents of a normal vtable slot.  A helper
// function for create_vtbl.
fn process_normal_mthd(cx: @local_ctxt, m: @ast::method, self_ty: ty::t,
                       ty_params: [ast::ty_param]) -> ValueRef {

    let llfnty = T_nil();
    let ccx = cx.ccx;
    alt ty::struct(cx.ccx.tcx, node_id_type(cx.ccx, m.node.id)) {
      ty::ty_fn(_, inputs, output, rs, _) {
        check non_ty_var(ccx, output);
        llfnty = type_of_fn(ccx, m.span, true, inputs, output,
                            vec::len(ty_params));
      }
    }
    let mcx: @local_ctxt =
        @{path: cx.path + ["method", m.node.ident] with *cx};
    let s: str = mangle_internal_name_by_path(mcx.ccx, mcx.path);
    let llfn: ValueRef = decl_internal_cdecl_fn(ccx.llmod, s, llfnty);

    // Every method on an object gets its node_id inserted into the crate-wide
    // item_ids map, together with the ValueRef that points to where that
    // method's definition will be in the executable.
    ccx.item_ids.insert(m.node.id, llfn);
    ccx.item_symbols.insert(m.node.id, s);
    trans_fn(mcx, m.span, m.node.meth, llfn, some(self_ty), ty_params,
             m.node.id);

    ret llfn;
}

// Update a self-stack structure ([[wrapper_self_pair], self_pair*]) to
// [[backwarding_vtbl*, inner_obj_body*], outer_obj*].
//
// We do this when we're receiving the outer object in a forwarding function
// via the llenv argument, and we want the forwarding function to call a
// method on a "self" that's inner-obj-shaped, but we also want to hold onto
// the outer obj for potential use later by backwarding functions.
fn populate_self_stack(bcx: @block_ctxt, self_stack: ValueRef,
                       outer_obj: ValueRef, backwarding_vtbl: ValueRef,
                       inner_obj_body: ValueRef) -> ValueRef {

    // Drop the outer obj into the second slot.
    let self_pair_ptr = GEPi(bcx, self_stack, [0, 1]);
    Store(bcx, outer_obj, self_pair_ptr);

    // Drop in the backwarding vtbl.
    let wrapper_pair = GEPi(bcx, self_stack, [0, 0]);
    let wrapper_vtbl_ptr = GEPi(bcx, wrapper_pair, [0, 0]);
    let backwarding_vtbl_cast =
        PointerCast(bcx, backwarding_vtbl, T_ptr(T_empty_struct()));
    Store(bcx, backwarding_vtbl_cast, wrapper_vtbl_ptr);

    // Drop in the inner obj body.
    let wrapper_body_ptr = GEPi(bcx, wrapper_pair, [0, 1]);
    Store(bcx, inner_obj_body, wrapper_body_ptr);

    ret self_stack;
}

fn type_of_meth(ccx: @crate_ctxt, sp: span, m: @ty::method,
                tps: [ast::ty_param]) -> TypeRef {
    let out_ty = m.output;
    check non_ty_var(ccx, out_ty);
    type_of_fn(ccx, sp, true, m.inputs, out_ty, vec::len(tps))
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
