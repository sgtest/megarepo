// Extracting metadata from crate files

import driver::session;
import syntax::ast;
import lib::llvm::False;
import lib::llvm::llvm;
import lib::llvm::mk_object_file;
import lib::llvm::mk_section_iter;
import front::attr;
import middle::resolve;
import syntax::walk;
import syntax::codemap::span;
import back::x86;
import util::common;
import std::str;
import std::vec;
import std::ebml;
import std::fs;
import std::io;
import std::option;
import std::option::none;
import std::option::some;
import std::map::hashmap;
import syntax::print::pprust;
import tags::*;

export read_crates;
export list_file_metadata;

fn metadata_matches(&vec[u8] crate_data,
                    &vec[@ast::meta_item] metas) -> bool {
    auto attrs = decoder::get_crate_attributes(crate_data);
    auto linkage_metas = attr::find_linkage_metas(attrs);

    log #fmt("matching %u metadata requirements against %u items",
             vec::len(metas), vec::len(linkage_metas));

    for (@ast::meta_item needed in metas) {
        if (!attr::contains(linkage_metas, needed)) {
            log #fmt("missing %s", pprust::meta_item_to_str(*needed));
            ret false;
        }
    }
    ret true;
}

fn default_native_lib_naming(session::session sess) ->
   rec(str prefix, str suffix) {
    alt (sess.get_targ_cfg().os) {
        case (session::os_win32) { ret rec(prefix="", suffix=".dll"); }
        case (session::os_macos) { ret rec(prefix="lib", suffix=".dylib"); }
        case (session::os_linux) { ret rec(prefix="lib", suffix=".so"); }
    }
}

fn find_library_crate(&session::session sess, &ast::ident ident,
                      &vec[@ast::meta_item] metas,
                      &vec[str] library_search_paths) ->
   option::t[tup(str, vec[u8])] {

    attr::require_unique_names(sess, metas);

    auto crate_name = {
        auto name_items = attr::find_meta_items_by_name(metas, "name");
        alt (vec::last(name_items)) {
            case (some(?i)) {
                alt (attr::get_meta_item_value_str(i)) {
                    case (some(?n)) { n }
                    case (_) {
                        // FIXME: Probably want a warning here since the user
                        // is using the wrong type of meta item
                        ident
                    }
                }
            }
            case (none) { ident }
        }
    };

    auto nn = default_native_lib_naming(sess);
    let str prefix = nn.prefix + crate_name;
    // FIXME: we could probably use a 'glob' function in std::fs but it will
    // be much easier to write once the unsafe module knows more about FFI
    // tricks. Currently the glob(3) interface is a bit more than we can
    // stomach from here, and writing a C++ wrapper is more work than just
    // manually filtering fs::list_dir here.

    for (str library_search_path in library_search_paths) {
        log #fmt("searching %s", library_search_path);
        for (str path in fs::list_dir(library_search_path)) {
            log #fmt("searching %s", path);
            let str f = fs::basename(path);
            if (!(str::starts_with(f, prefix) &&
                      str::ends_with(f, nn.suffix))) {
                log #fmt("skipping %s, doesn't look like %s*%s", path, prefix,
                         nn.suffix);
                cont;
            }
            alt (get_metadata_section(path)) {
                case (option::some(?cvec)) {
                    if (!metadata_matches(cvec, metas)) {
                        log #fmt("skipping %s, metadata doesn't match", path);
                        cont;
                    }
                    log #fmt("found %s with matching metadata", path);
                    ret some(tup(path, cvec));
                }
                case (_) { }
            }
        }
    }
    ret none;
}

fn get_metadata_section(str filename) -> option::t[vec[u8]] {
    auto b = str::buf(filename);
    auto mb = llvm::LLVMRustCreateMemoryBufferWithContentsOfFile(b);
    if (mb as int == 0) { ret option::none[vec[u8]]; }
    auto of = mk_object_file(mb);
    auto si = mk_section_iter(of.llof);
    while (llvm::LLVMIsSectionIteratorAtEnd(of.llof, si.llsi) == False) {
        auto name_buf = llvm::LLVMGetSectionName(si.llsi);
        auto name = str::str_from_cstr(name_buf);
        if (str::eq(name, x86::get_meta_sect_name())) {
            auto cbuf = llvm::LLVMGetSectionContents(si.llsi);
            auto csz = llvm::LLVMGetSectionSize(si.llsi);
            auto cvbuf = cbuf as vec::vbuf;
            ret option::some[vec[u8]](vec::vec_from_vbuf[u8](cvbuf, csz));
        }
        llvm::LLVMMoveToNextSection(si.llsi);
    }
    ret option::none[vec[u8]];
}

fn load_library_crate(&session::session sess, span span, int cnum,
                      &ast::ident ident, vec[@ast::meta_item] metas,
                      &vec[str] library_search_paths) {
    alt (find_library_crate(sess, ident, metas, library_search_paths)) {
        case (some(?t)) {
            sess.set_external_crate(cnum, rec(name=ident, data=t._1));
            sess.add_used_crate_file(t._0);
            ret;
        }
        case (_) { }
    }
    sess.span_fatal(span, #fmt("can't find crate for '%s'", ident));
}

type env =
    @rec(session::session sess,
         resolve::crate_map crate_map,
         @hashmap[str, int] crate_cache,
         vec[str] library_search_paths,
         mutable int next_crate_num);

fn visit_view_item(env e, &@ast::view_item i) {
    alt (i.node) {
        case (ast::view_item_use(?ident, ?meta_items, ?id)) {
            auto cnum;
            if (!e.crate_cache.contains_key(ident)) {
                cnum = e.next_crate_num;
                load_library_crate(e.sess, i.span, cnum, ident,
                                   meta_items, e.library_search_paths);
                e.crate_cache.insert(ident, e.next_crate_num);
                e.next_crate_num += 1;
            } else { cnum = e.crate_cache.get(ident); }
            e.crate_map.insert(id, cnum);
        }
        case (_) { }
    }
}

fn visit_item(env e, &@ast::item i) {
    alt (i.node) {
        case (ast::item_native_mod(?m)) {
            if (m.abi != ast::native_abi_rust &&
                m.abi != ast::native_abi_cdecl) {
                ret;
            }
            if (!e.sess.add_used_library(m.native_name)) {
                ret;
            }
            for (ast::attribute a in
                     attr::find_attrs_by_name(i.attrs, "link_args")) {
                alt (attr::get_meta_item_value_str(attr::attr_meta(a))) {
                    case (some(?linkarg)) {
                        e.sess.add_used_link_args(linkarg);
                    }
                    case (none) { /* fallthrough */ }
                }
            }
        }
        case (_) {
        }
    }
}

// Reads external crates referenced by "use" directives.
fn read_crates(session::session sess, resolve::crate_map crate_map,
               &ast::crate crate) {
    auto e =
        @rec(sess=sess,
             crate_map=crate_map,
             crate_cache=@syntax::_std::new_str_hash[int](),
             library_search_paths=sess.get_opts().library_search_paths,
             mutable next_crate_num=1);
    auto v =
        rec(visit_view_item_pre=bind visit_view_item(e, _),
            visit_item_pre=bind visit_item(e, _)
            with walk::default_visitor());
    walk::walk_crate(v, crate);
}


fn list_file_metadata(str path, io::writer out) {
    alt (get_metadata_section(path)) {
        case (option::some(?bytes)) {
            decoder::list_crate_metadata(bytes, out);
        }
        case (option::none) {
            out.write_str("Could not find metadata in " + path + ".\n");
        }
    }
}


// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
