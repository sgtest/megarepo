// The crate store - a central repo for information collected about external
// crates and libraries

import std::map;
import std::vec;
import std::str;

type crate_metadata = rec(str name, vec[u8] data);

type cstore = @rec(map::hashmap[int, crate_metadata] metas,
                   mutable vec[str] used_crate_files,
                   mutable vec[str] used_libraries,
                   mutable vec[str] used_link_args);

fn mk_cstore() -> cstore {
    auto meta_cache = map::new_int_hash[crate_metadata]();
    ret @rec(metas = meta_cache,
             mutable used_crate_files = [],
             mutable used_libraries = [],
             mutable used_link_args = []);
}

fn get_crate_data(&cstore cstore, int cnum) -> crate_metadata {
    ret cstore.metas.get(cnum);
}

fn set_crate_data(&cstore cstore, int cnum, &crate_metadata data) {
    cstore.metas.insert(cnum, data);
}

fn have_crate_data(&cstore cstore, int cnum) -> bool {
    ret cstore.metas.contains_key(cnum);
}

fn add_used_crate_file(&cstore cstore, &str lib) {
    if (!vec::member(lib, cstore.used_crate_files)) {
        cstore.used_crate_files += [lib];
    }
}

fn get_used_crate_files(&cstore cstore) -> vec[str] {
    ret cstore.used_crate_files;
}

fn add_used_library(&cstore cstore, &str lib) -> bool {
    if (lib == "") { ret false; }

    if (vec::member(lib, cstore.used_libraries)) {
        ret false;
    }

    cstore.used_libraries += [lib];
    ret true;
}

fn get_used_libraries(&cstore cstore) -> vec[str] {
    ret cstore.used_libraries;
}

fn add_used_link_args(&cstore cstore, &str args) {
    cstore.used_link_args += str::split(args, ' ' as u8);
}

fn get_used_link_args(&cstore cstore) -> vec[str] {
    ret cstore.used_link_args;
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
