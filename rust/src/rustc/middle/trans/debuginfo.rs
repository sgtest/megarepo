import std::map;
import std::map::hashmap;
import lib::llvm::llvm;
import lib::llvm::ValueRef;
import trans::common::*;
import trans::base;
import trans::build::B;
import middle::ty;
import syntax::{ast, codemap, ast_util, ast_map};
import codemap::span;
import ast::ty;
import pat_util::*;
import util::ppaux::ty_to_str;
import driver::session::session;

export create_local_var;
export create_function;
export create_arg;
export update_source_pos;
export debug_ctxt;
export mk_ctxt;

const LLVMDebugVersion: int = (9 << 16);

const DW_LANG_RUST: int = 0x9000;
const DW_VIRTUALITY_none: int = 0;

const CompileUnitTag: int = 17;
const FileDescriptorTag: int = 41;
const SubprogramTag: int = 46;
const SubroutineTag: int = 21;
const BasicTypeDescriptorTag: int = 36;
const AutoVariableTag: int = 256;
const ArgVariableTag: int = 257;
const ReturnVariableTag: int = 258;
const LexicalBlockTag: int = 11;
const PointerTypeTag: int = 15;
const StructureTypeTag: int = 19;
const MemberTag: int = 13;
const ArrayTypeTag: int = 1;
const SubrangeTag: int = 33;

const DW_ATE_boolean: int = 0x02;
const DW_ATE_float: int = 0x04;
const DW_ATE_signed: int = 0x05;
const DW_ATE_signed_char: int = 0x06;
const DW_ATE_unsigned: int = 0x07;
const DW_ATE_unsigned_char: int = 0x08;

fn llstr(s: ~str) -> ValueRef {
    str::as_c_str(s, |sbuf| {
        llvm::LLVMMDString(sbuf, str::len(s) as libc::c_uint)
    })
}
fn lltag(lltag: int) -> ValueRef {
    lli32(LLVMDebugVersion | lltag)
}
fn lli32(val: int) -> ValueRef {
    C_i32(val as i32)
}
fn lli64(val: int) -> ValueRef {
    C_i64(val as i64)
}
fn lli1(bval: bool) -> ValueRef {
    C_bool(bval)
}
fn llmdnode(elems: ~[ValueRef]) -> ValueRef unsafe {
    llvm::LLVMMDNode(vec::unsafe::to_ptr(elems),
                     vec::len(elems) as libc::c_uint)
}
fn llunused() -> ValueRef {
    lli32(0x0)
}
fn llnull() -> ValueRef unsafe {
    unsafe::reinterpret_cast(ptr::null::<ValueRef>())
}

fn add_named_metadata(cx: @crate_ctxt, name: ~str, val: ValueRef) {
    str::as_c_str(name, |sbuf| {
        llvm::LLVMAddNamedMetadataOperand(cx.llmod, sbuf, val)
    })
}

////////////////

type debug_ctxt = {
    llmetadata: metadata_cache,
    names: namegen,
    crate_file: ~str
};

fn mk_ctxt(crate: ~str) -> debug_ctxt {
    {llmetadata: map::int_hash(),
     names: new_namegen(),
     crate_file: crate}
}

fn update_cache(cache: metadata_cache, mdtag: int, val: debug_metadata) {
    let existing = if cache.contains_key(mdtag) {
        cache.get(mdtag)
    } else {
        ~[]
    };
    cache.insert(mdtag, vec::append_one(existing, val));
}

type metadata<T> = {node: ValueRef, data: T};

type file_md = {path: ~str};
type compile_unit_md = {name: ~str};
type subprogram_md = {id: ast::node_id};
type local_var_md = {id: ast::node_id};
type tydesc_md = {hash: uint};
type block_md = {start: codemap::loc, end: codemap::loc};
type argument_md = {id: ast::node_id};
type retval_md = {id: ast::node_id};

type metadata_cache = hashmap<int, ~[debug_metadata]>;

enum debug_metadata {
    file_metadata(@metadata<file_md>),
    compile_unit_metadata(@metadata<compile_unit_md>),
    subprogram_metadata(@metadata<subprogram_md>),
    local_var_metadata(@metadata<local_var_md>),
    tydesc_metadata(@metadata<tydesc_md>),
    block_metadata(@metadata<block_md>),
    argument_metadata(@metadata<argument_md>),
    retval_metadata(@metadata<retval_md>),
}

fn cast_safely<T: copy, U>(val: T) -> U unsafe {
    let val2 = val;
    return unsafe::transmute(val2);
}

fn md_from_metadata<T>(val: debug_metadata) -> T unsafe {
    match val {
      file_metadata(md) => cast_safely(md),
      compile_unit_metadata(md) => cast_safely(md),
      subprogram_metadata(md) => cast_safely(md),
      local_var_metadata(md) => cast_safely(md),
      tydesc_metadata(md) => cast_safely(md),
      block_metadata(md) => cast_safely(md),
      argument_metadata(md) => cast_safely(md),
      retval_metadata(md) => cast_safely(md)
    }
}

fn cached_metadata<T: copy>(cache: metadata_cache, mdtag: int,
                           eq: fn(md: T) -> bool) -> option<T> unsafe {
    if cache.contains_key(mdtag) {
        let items = cache.get(mdtag);
        for items.each |item| {
            let md: T = md_from_metadata::<T>(item);
            if eq(md) {
                return option::some(md);
            }
        }
    }
    return option::none;
}

fn create_compile_unit(cx: @crate_ctxt)
    -> @metadata<compile_unit_md> unsafe {
    let cache = get_cache(cx);
    let crate_name = option::get(cx.dbg_cx).crate_file;
    let tg = CompileUnitTag;
    match cached_metadata::<@metadata<compile_unit_md>>(cache, tg,
                        |md| md.data.name == crate_name) {
      option::some(md) => return md,
      option::none => ()
    }

    let (_, work_dir) = get_file_path_and_dir(cx.sess.working_dir,
                                              crate_name);
    let unit_metadata = ~[lltag(tg),
                         llunused(),
                         lli32(DW_LANG_RUST),
                         llstr(crate_name),
                         llstr(work_dir),
                         llstr(env!{"CFG_VERSION"}),
                         lli1(true), // deprecated: main compile unit
                         lli1(cx.sess.opts.optimize != 0u),
                         llstr(~""), // flags (???)
                         lli32(0) // runtime version (???)
                        ];
    let unit_node = llmdnode(unit_metadata);
    add_named_metadata(cx, ~"llvm.dbg.cu", unit_node);
    let mdval = @{node: unit_node, data: {name: crate_name}};
    update_cache(cache, tg, compile_unit_metadata(mdval));

    return mdval;
}

fn get_cache(cx: @crate_ctxt) -> metadata_cache {
    option::get(cx.dbg_cx).llmetadata
}

fn get_file_path_and_dir(work_dir: ~str, full_path: ~str) -> (~str, ~str) {
    (if str::starts_with(full_path, work_dir) {
        str::slice(full_path, str::len(work_dir) + 1u,
                   str::len(full_path))
    } else {
        full_path
    }, work_dir)
}

fn create_file(cx: @crate_ctxt, full_path: ~str) -> @metadata<file_md> {
    let cache = get_cache(cx);;
    let tg = FileDescriptorTag;
    match cached_metadata::<@metadata<file_md>>(
        cache, tg, |md| md.data.path == full_path) {
        option::some(md) => return md,
        option::none => ()
    }

    let (file_path, work_dir) = get_file_path_and_dir(cx.sess.working_dir,
                                                      full_path);
    let unit_node = create_compile_unit(cx).node;
    let file_md = ~[lltag(tg),
                   llstr(file_path),
                   llstr(work_dir),
                   unit_node];
    let val = llmdnode(file_md);
    let mdval = @{node: val, data: {path: full_path}};
    update_cache(cache, tg, file_metadata(mdval));
    return mdval;
}

fn line_from_span(cm: codemap::codemap, sp: span) -> uint {
    codemap::lookup_char_pos(cm, sp.lo).line
}

fn create_block(cx: block) -> @metadata<block_md> {
    let cache = get_cache(cx.ccx());
    let mut cx = cx;
    while option::is_none(cx.node_info) {
        match cx.parent {
          some(b) => cx = b,
          none => fail
        }
    }
    let sp = option::get(cx.node_info).span;

    let start = codemap::lookup_char_pos(cx.sess().codemap, sp.lo);
    let fname = start.file.name;
    let end = codemap::lookup_char_pos(cx.sess().codemap, sp.hi);
    let tg = LexicalBlockTag;
    /*alt cached_metadata::<@metadata<block_md>>(
        cache, tg,
        {|md| start == md.data.start && end == md.data.end}) {
      option::some(md) { return md; }
      option::none {}
    }*/

    let parent = match cx.parent {
        none => create_function(cx.fcx).node,
        some(bcx) => create_block(bcx).node
    };
    let file_node = create_file(cx.ccx(), fname);
    let unique_id = match cache.find(LexicalBlockTag) {
      option::some(v) => vec::len(v) as int,
      option::none => 0
    };
    let lldata = ~[lltag(tg),
                  parent,
                  lli32(start.line as int),
                  lli32(start.col as int),
                  file_node.node,
                  lli32(unique_id)
                 ];
    let val = llmdnode(lldata);
    let mdval = @{node: val, data: {start: start, end: end}};
    //update_cache(cache, tg, block_metadata(mdval));
    return mdval;
}

fn size_and_align_of(cx: @crate_ctxt, t: ty::t) -> (int, int) {
    let llty = type_of::type_of(cx, t);
    (shape::llsize_of_real(cx, llty) as int,
     shape::llalign_of_pref(cx, llty) as int)
}

fn create_basic_type(cx: @crate_ctxt, t: ty::t, ty: ast::prim_ty, span: span)
    -> @metadata<tydesc_md> {
    let cache = get_cache(cx);
    let tg = BasicTypeDescriptorTag;
    match cached_metadata::<@metadata<tydesc_md>>(
        cache, tg, |md| ty::type_id(t) == md.data.hash) {
      option::some(md) => return md,
      option::none => ()
    }

    let (name, encoding) = match check ty {
      ast::ty_bool => (~"bool", DW_ATE_boolean),
      ast::ty_int(m) => match m {
        ast::ty_char => (~"char", DW_ATE_unsigned),
        ast::ty_i => (~"int", DW_ATE_signed),
        ast::ty_i8 => (~"i8", DW_ATE_signed_char),
        ast::ty_i16 => (~"i16", DW_ATE_signed),
        ast::ty_i32 => (~"i32", DW_ATE_signed),
        ast::ty_i64 => (~"i64", DW_ATE_signed)
      }
      ast::ty_uint(m) => match m {
        ast::ty_u => (~"uint", DW_ATE_unsigned),
        ast::ty_u8 => (~"u8", DW_ATE_unsigned_char),
        ast::ty_u16 => (~"u16", DW_ATE_unsigned),
        ast::ty_u32 => (~"u32", DW_ATE_unsigned),
        ast::ty_u64 => (~"u64", DW_ATE_unsigned)
      }
      ast::ty_float(m) => match m {
        ast::ty_f => (~"float", DW_ATE_float),
        ast::ty_f32 => (~"f32", DW_ATE_float),
        ast::ty_f64 => (~"f64", DW_ATE_float)
      }
    };

    let fname = filename_from_span(cx, span);
    let file_node = create_file(cx, fname);
    let cu_node = create_compile_unit(cx);
    let (size, align) = size_and_align_of(cx, t);
    let lldata = ~[lltag(tg),
                  cu_node.node,
                  llstr(name),
                  file_node.node,
                  lli32(0), //XXX source line
                  lli64(size * 8),  // size in bits
                  lli64(align * 8), // alignment in bits
                  lli64(0), //XXX offset?
                  lli32(0), //XXX flags?
                  lli32(encoding)];
    let llnode = llmdnode(lldata);
    let mdval = @{node: llnode, data: {hash: ty::type_id(t)}};
    update_cache(cache, tg, tydesc_metadata(mdval));
    add_named_metadata(cx, ~"llvm.dbg.ty", llnode);
    return mdval;
}

fn create_pointer_type(cx: @crate_ctxt, t: ty::t, span: span,
                       pointee: @metadata<tydesc_md>)
    -> @metadata<tydesc_md> {
    let tg = PointerTypeTag;
    /*let cache = cx.llmetadata;
    match cached_metadata::<@metadata<tydesc_md>>(
        cache, tg, {|md| ty::hash_ty(t) == ty::hash_ty(md.data.hash)}) {
      option::some(md) { return md; }
      option::none {}
    }*/
    let (size, align) = size_and_align_of(cx, t);
    let fname = filename_from_span(cx, span);
    let file_node = create_file(cx, fname);
    //let cu_node = create_compile_unit(cx, fname);
    let llnode = create_derived_type(tg, file_node.node, ~"", 0, size * 8,
                                     align * 8, 0, pointee.node);
    let mdval = @{node: llnode, data: {hash: ty::type_id(t)}};
    //update_cache(cache, tg, tydesc_metadata(mdval));
    add_named_metadata(cx, ~"llvm.dbg.ty", llnode);
    return mdval;
}

type struct_ctxt = {
    file: ValueRef,
    name: ~str,
    line: int,
    mut members: ~[ValueRef],
    mut total_size: int,
    align: int
};

fn finish_structure(cx: @struct_ctxt) -> ValueRef {
    return create_composite_type(StructureTypeTag, cx.name, cx.file, cx.line,
                              cx.total_size, cx.align, 0, option::none,
                              option::some(cx.members));
}

fn create_structure(file: @metadata<file_md>, name: ~str, line: int)
    -> @struct_ctxt {
    let cx = @{file: file.node,
               name: name,
               line: line,
               mut members: ~[],
               mut total_size: 0,
               align: 64 //XXX different alignment per arch?
              };
    return cx;
}

fn create_derived_type(type_tag: int, file: ValueRef, name: ~str, line: int,
                       size: int, align: int, offset: int, ty: ValueRef)
    -> ValueRef {
    let lldata = ~[lltag(type_tag),
                  file,
                  llstr(name),
                  file,
                  lli32(line),
                  lli64(size),
                  lli64(align),
                  lli64(offset),
                  lli32(0),
                  ty];
    return llmdnode(lldata);
}

fn add_member(cx: @struct_ctxt, name: ~str, line: int, size: int, align: int,
              ty: ValueRef) {
    vec::push(cx.members, create_derived_type(MemberTag, cx.file, name, line,
                                       size * 8, align * 8, cx.total_size,
                                       ty));
    cx.total_size += size * 8;
}

fn create_record(cx: @crate_ctxt, t: ty::t, fields: ~[ast::ty_field],
                 span: span) -> @metadata<tydesc_md> {
    let fname = filename_from_span(cx, span);
    let file_node = create_file(cx, fname);
    let scx = create_structure(file_node,
                               option::get(cx.dbg_cx).names(~"rec"),
                               line_from_span(cx.sess.codemap,
                                              span) as int);
    for fields.each |field| {
        let field_t = ty::get_field(t, field.node.ident).mt.ty;
        let ty_md = create_ty(cx, field_t, field.node.mt.ty);
        let (size, align) = size_and_align_of(cx, field_t);
        add_member(scx, *field.node.ident,
                   line_from_span(cx.sess.codemap, field.span) as int,
                   size as int, align as int, ty_md.node);
    }
    let mdval = @{node: finish_structure(scx), data:{hash: ty::type_id(t)}};
    return mdval;
}

fn create_boxed_type(cx: @crate_ctxt, outer: ty::t, _inner: ty::t,
                     span: span, boxed: @metadata<tydesc_md>)
    -> @metadata<tydesc_md> {
    //let tg = StructureTypeTag;
    /*let cache = cx.llmetadata;
    match cached_metadata::<@metadata<tydesc_md>>(
        cache, tg, {|md| ty::hash_ty(outer) == ty::hash_ty(md.data.hash)}) {
      option::some(md) { return md; }
      option::none {}
    }*/
    let fname = filename_from_span(cx, span);
    let file_node = create_file(cx, fname);
    //let cu_node = create_compile_unit_metadata(cx, fname);
    let uint_t = ty::mk_uint(cx.tcx);
    let refcount_type = create_basic_type(cx, uint_t,
                                          ast::ty_uint(ast::ty_u), span);
    let scx = create_structure(file_node, ty_to_str(cx.tcx, outer), 0);
    add_member(scx, ~"refcnt", 0, sys::size_of::<uint>() as int,
               sys::min_align_of::<uint>() as int, refcount_type.node);
    add_member(scx, ~"boxed", 0, 8, //XXX member_size_and_align(??)
               8, //XXX just a guess
               boxed.node);
    let llnode = finish_structure(scx);
    let mdval = @{node: llnode, data: {hash: ty::type_id(outer)}};
    //update_cache(cache, tg, tydesc_metadata(mdval));
    add_named_metadata(cx, ~"llvm.dbg.ty", llnode);
    return mdval;
}

fn create_composite_type(type_tag: int, name: ~str, file: ValueRef, line: int,
                         size: int, align: int, offset: int,
                         derived: option<ValueRef>,
                         members: option<~[ValueRef]>)
    -> ValueRef {
    let lldata = ~[lltag(type_tag),
                  file,
                  llstr(name), // type name
                  file, // source file definition
                  lli32(line), // source line definition
                  lli64(size), // size of members
                  lli64(align), // align
                  lli32/*64*/(offset), // offset
                  lli32(0), // flags
                  if option::is_none(derived) {
                      llnull()
                  } else { // derived from
                      option::get(derived)
                  },
                  if option::is_none(members) {
                      llnull()
                  } else { //members
                      llmdnode(option::get(members))
                  },
                  lli32(0),  // runtime language
                  llnull()
                 ];
    return llmdnode(lldata);
}

fn create_vec(cx: @crate_ctxt, vec_t: ty::t, elem_t: ty::t,
              vec_ty_span: codemap::span, elem_ty: @ast::ty)
    -> @metadata<tydesc_md> {
    let fname = filename_from_span(cx, vec_ty_span);
    let file_node = create_file(cx, fname);
    let elem_ty_md = create_ty(cx, elem_t, elem_ty);
    let scx = create_structure(file_node, ty_to_str(cx.tcx, vec_t), 0);
    let size_t_type = create_basic_type(cx, ty::mk_uint(cx.tcx),
                                        ast::ty_uint(ast::ty_u), vec_ty_span);
    add_member(scx, ~"fill", 0, sys::size_of::<libc::size_t>() as int,
               sys::min_align_of::<libc::size_t>() as int, size_t_type.node);
    add_member(scx, ~"alloc", 0, sys::size_of::<libc::size_t>() as int,
               sys::min_align_of::<libc::size_t>() as int, size_t_type.node);
    let subrange = llmdnode(~[lltag(SubrangeTag), lli64(0), lli64(0)]);
    let (arr_size, arr_align) = size_and_align_of(cx, elem_t);
    let data_ptr = create_composite_type(ArrayTypeTag, ~"", file_node.node, 0,
                                         arr_size, arr_align, 0,
                                         option::some(elem_ty_md.node),
                                         option::some(~[subrange]));
    add_member(scx, ~"data", 0, 0, // clang says the size should be 0
               sys::min_align_of::<u8>() as int, data_ptr);
    let llnode = finish_structure(scx);
    return @{node: llnode, data: {hash: ty::type_id(vec_t)}};
}

fn create_ty(_cx: @crate_ctxt, _t: ty::t, _ty: @ast::ty)
    -> @metadata<tydesc_md> {
    /*let cache = get_cache(cx);
    match cached_metadata::<@metadata<tydesc_md>>(
        cache, tg, {|md| t == md.data.hash}) {
      option::some(md) { return md; }
      option::none {}
    }*/

    /* FIXME (#2012): disabled this code as part of the patch that moves
     * recognition of named builtin types into resolve. I tried to fix
     * it, but it seems to already be broken -- it's only called when
     * --xg is given, and compiling with --xg fails on trivial programs.
     *
     * Generating an ast::ty from a ty::t seems like it should not be
     * needed. It is only done to track spans, but you will not get the
     * right spans anyway -- types tend to refer to stuff defined
     * elsewhere, not be self-contained.
     */

    fail;
    /*
    fn t_to_ty(cx: crate_ctxt, t: ty::t, span: span) -> @ast::ty {
        let ty = match ty::get(t).struct {
          ty::ty_nil { ast::ty_nil }
          ty::ty_bot { ast::ty_bot }
          ty::ty_bool { ast::ty_bool }
          ty::ty_int(t) { ast::ty_int(t) }
          ty::ty_float(t) { ast::ty_float(t) }
          ty::ty_uint(t) { ast::ty_uint(t) }
          ty::ty_box(mt) { ast::ty_box({ty: t_to_ty(cx, mt.ty, span),
                                        mutbl: mt.mutbl}) }
          ty::ty_uniq(mt) { ast::ty_uniq({ty: t_to_ty(cx, mt.ty, span),
                                          mutbl: mt.mutbl}) }
          ty::ty_rec(fields) {
            let fs = ~[];
            for field in fields {
                vec::push(fs, {node: {ident: field.ident,
                               mt: {ty: t_to_ty(cx, field.mt.ty, span),
                                    mutbl: field.mt.mutbl}},
                        span: span});
            }
            ast::ty_rec(fs)
          }
          ty::ty_vec(mt) { ast::ty_vec({ty: t_to_ty(cx, mt.ty, span),
                                        mutbl: mt.mutbl}) }
          _ {
            cx.sess.span_bug(span, "t_to_ty: Can't handle this type");
          }
        };
        return @{node: ty, span: span};
    }

    match ty.node {
      ast::ty_box(mt) {
        let inner_t = match ty::get(t).struct {
          ty::ty_box(boxed) { boxed.ty }
          _ { cx.sess.span_bug(ty.span, "t_to_ty was incoherent"); }
        };
        let md = create_ty(cx, inner_t, mt.ty);
        let box = create_boxed_type(cx, t, inner_t, ty.span, md);
        return create_pointer_type(cx, t, ty.span, box);
      }

      ast::ty_uniq(mt) {
        let inner_t = match ty::get(t).struct {
          ty::ty_uniq(boxed) { boxed.ty }
          // Hoping we'll have a way to eliminate this check soon.
          _ { cx.sess.span_bug(ty.span, "t_to_ty was incoherent"); }
        };
        let md = create_ty(cx, inner_t, mt.ty);
        return create_pointer_type(cx, t, ty.span, md);
      }

      ast::ty_infer {
        let inferred = t_to_ty(cx, t, ty.span);
        return create_ty(cx, t, inferred);
      }

      ast::ty_rec(fields) {
        return create_record(cx, t, fields, ty.span);
      }

      ast::ty_vec(mt) {
        let inner_t = ty::sequence_element_type(cx.tcx, t);
        let inner_ast_t = t_to_ty(cx, inner_t, mt.ty.span);
        let v = create_vec(cx, t, inner_t, ty.span, inner_ast_t);
        return create_pointer_type(cx, t, ty.span, v);
      }

      ast::ty_path(_, id) {
        match cx.tcx.def_map.get(id) {
          ast::def_prim_ty(pty) {
            return create_basic_type(cx, t, pty, ty.span);
          }
          _ {}
        }
      }

      _ {}
    };
    */
}

fn filename_from_span(cx: @crate_ctxt, sp: codemap::span) -> ~str {
    codemap::lookup_char_pos(cx.sess.codemap, sp.lo).file.name
}

fn create_var(type_tag: int, context: ValueRef, name: ~str, file: ValueRef,
              line: int, ret_ty: ValueRef) -> ValueRef {
    let lldata = ~[lltag(type_tag),
                  context,
                  llstr(name),
                  file,
                  lli32(line),
                  ret_ty,
                  lli32(0)
                 ];
    return llmdnode(lldata);
}

fn create_local_var(bcx: block, local: @ast::local)
    -> @metadata<local_var_md> unsafe {
    let cx = bcx.ccx();
    let cache = get_cache(cx);
    let tg = AutoVariableTag;
    match cached_metadata::<@metadata<local_var_md>>(
        cache, tg, |md| md.data.id == local.node.id) {
      option::some(md) => return md,
      option::none => ()
    }

    let name = match local.node.pat.node {
      ast::pat_ident(_, pth, _) => ast_util::path_to_ident(pth),
      // FIXME this should be handled (#2533)
      _ => fail ~"no single variable name for local"
    };
    let loc = codemap::lookup_char_pos(cx.sess.codemap,
                                       local.span.lo);
    let ty = node_id_type(bcx, local.node.id);
    let tymd = create_ty(cx, ty, local.node.ty);
    let filemd = create_file(cx, loc.file.name);
    let context = match bcx.parent {
        none => create_function(bcx.fcx).node,
        some(_) => create_block(bcx).node
    };
    let mdnode = create_var(tg, context, *name, filemd.node,
                            loc.line as int, tymd.node);
    let mdval = @{node: mdnode, data: {id: local.node.id}};
    update_cache(cache, AutoVariableTag, local_var_metadata(mdval));

    let llptr = match bcx.fcx.lllocals.find(local.node.id) {
      option::some(local_mem(v)) => v,
      option::some(_) => {
        bcx.tcx().sess.span_bug(local.span, ~"local is bound to \
                something weird");
      }
      option::none => {
        match bcx.fcx.lllocals.get(local.node.pat.id) {
          local_imm(v) => v,
          _ => bcx.tcx().sess.span_bug(local.span, ~"local is bound to \
                                                     something weird")
        }
      }
    };
    let declargs = ~[llmdnode(~[llptr]), mdnode];
    trans::build::Call(bcx, cx.intrinsics.get(~"llvm.dbg.declare"),
                       declargs);
    return mdval;
}

fn create_arg(bcx: block, arg: ast::arg, sp: span)
    -> @metadata<argument_md> unsafe {
    let fcx = bcx.fcx, cx = fcx.ccx;
    let cache = get_cache(cx);
    let tg = ArgVariableTag;
    match cached_metadata::<@metadata<argument_md>>(
        cache, ArgVariableTag, |md| md.data.id == arg.id) {
      option::some(md) => return md,
      option::none => ()
    }

    let loc = codemap::lookup_char_pos(cx.sess.codemap,
                                       sp.lo);
    let ty = node_id_type(bcx, arg.id);
    let tymd = create_ty(cx, ty, arg.ty);
    let filemd = create_file(cx, loc.file.name);
    let context = create_function(bcx.fcx);
    let mdnode = create_var(tg, context.node, *arg.ident, filemd.node,
                            loc.line as int, tymd.node);
    let mdval = @{node: mdnode, data: {id: arg.id}};
    update_cache(cache, tg, argument_metadata(mdval));

    let llptr = match fcx.llargs.get(arg.id) {
      local_mem(v) | local_imm(v) => v,
    };
    let declargs = ~[llmdnode(~[llptr]), mdnode];
    trans::build::Call(bcx, cx.intrinsics.get(~"llvm.dbg.declare"),
                       declargs);
    return mdval;
}

fn update_source_pos(cx: block, s: span) {
    if !cx.sess().opts.debuginfo {
        return;
    }
    let cm = cx.sess().codemap;
    let blockmd = create_block(cx);
    let loc = codemap::lookup_char_pos(cm, s.lo);
    let scopedata = ~[lli32(loc.line as int),
                     lli32(loc.col as int),
                     blockmd.node,
                     llnull()];
    let dbgscope = llmdnode(scopedata);
    llvm::LLVMSetCurrentDebugLocation(trans::build::B(cx), dbgscope);
}

fn create_function(fcx: fn_ctxt) -> @metadata<subprogram_md> {
    let cx = fcx.ccx;
    let dbg_cx = option::get(cx.dbg_cx);

    debug!{"~~"};
    log(debug, fcx.id);

    let sp = option::get(fcx.span);
    log(debug, codemap::span_to_str(sp, cx.sess.codemap));

    let (ident, ret_ty, id) = match cx.tcx.items.get(fcx.id) {
      ast_map::node_item(item, _) => {
        match item.node {
          ast::item_fn(decl, _, _) => {
            (item.ident, decl.output, item.id)
          }
          _ => fcx.ccx.sess.span_bug(item.span, ~"create_function: item \
                                                  bound to non-function")
        }
      }
      ast_map::node_method(method, _, _) => {
          (method.ident, method.decl.output, method.id)
      }
      ast_map::node_ctor(nm, _, ctor, _, _) => {
        // FIXME: output type may be wrong (#2194)
        (nm, ctor.node.dec.output, ctor.node.id)
      }
      ast_map::node_expr(expr) => {
        match expr.node {
          ast::expr_fn(_, decl, _, _) => {
            (@dbg_cx.names(~"fn"), decl.output, expr.id)
          }
          ast::expr_fn_block(decl, _, _) => {
            (@dbg_cx.names(~"fn"), decl.output, expr.id)
          }
          _ => fcx.ccx.sess.span_bug(expr.span,
                                     ~"create_function: \
                                       expected an expr_fn or fn_block here")
        }
      }
      _ => fcx.ccx.sess.bug(~"create_function: unexpected \
                              sort of node")
    };

    log(debug, ident);
    log(debug, id);

    let cache = get_cache(cx);
    match cached_metadata::<@metadata<subprogram_md>>(
        cache, SubprogramTag, |md| md.data.id == id) {
      option::some(md) => return md,
      option::none => ()
    }

    let loc = codemap::lookup_char_pos(cx.sess.codemap,
                                       sp.lo);
    let file_node = create_file(cx, loc.file.name).node;
    let ty_node = if cx.sess.opts.extra_debuginfo {
        match ret_ty.node {
          ast::ty_nil => llnull(),
          _ => create_ty(cx, ty::node_id_to_type(cx.tcx, id), ret_ty).node
        }
    } else {
        llnull()
    };
    let sub_node = create_composite_type(SubroutineTag, ~"", file_node, 0, 0,
                                         0, 0, option::none,
                                         option::some(~[ty_node]));

    let fn_metadata = ~[lltag(SubprogramTag),
                       llunused(),
                       file_node,
                       llstr(*ident),
                       llstr(*ident), //XXX fully-qualified C++ name
                       llstr(~""), //XXX MIPS name?????
                       file_node,
                       lli32(loc.line as int),
                       sub_node,
                       lli1(false), //XXX static (check export)
                       lli1(true), // defined in compilation unit
                       lli32(DW_VIRTUALITY_none), // virtual-ness
                       lli32(0i), //index into virt func
                       /*llnull()*/ lli32(0), // base type with vtbl
                       lli32(256), // flags
                       lli1(cx.sess.opts.optimize != 0u),
                       fcx.llfn
                       //list of template params
                       //func decl descriptor
                       //list of func vars
                      ];
    let val = llmdnode(fn_metadata);
    add_named_metadata(cx, ~"llvm.dbg.sp", val);
    let mdval = @{node: val, data: {id: id}};
    update_cache(cache, SubprogramTag, subprogram_metadata(mdval));

    return mdval;
}
