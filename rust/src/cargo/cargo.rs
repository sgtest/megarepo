// cargo.rs - Rust package manager

use rustc;
use std;

import rustc::syntax::{ast, codemap};
import rustc::syntax::parse::parser;
import rustc::util::filesearch::{get_cargo_root, get_cargo_root_nearest,
                                 get_cargo_sysroot, libdir};
import rustc::driver::diagnostic;

import result::{ok, err};
import io::writer_util;
import std::json;
import result;
import std::map;
import std::map::hashmap;
import str;
import std::tempfile;
import vec;
import std::getopts;
import getopts::{optflag, optopt, opt_present};

enum _src {
    /* Break cycles in package <-> source */
    _source(source),
}

type package = {
//    source: _src,
    name: str,
    uuid: str,
    url: str,
    method: str,
    description: str,
    ref: option<str>,
    tags: [str]
};

type source = {
    name: str,
    url: str,
    sig: option<str>,
    key: option<str>,
    keyfp: option<str>,
    mut packages: [package]
};

type cargo = {
    pgp: bool,
    root: str,
    bindir: str,
    libdir: str,
    workdir: str,
    sourcedir: str,
    sources: map::hashmap<str, source>,
    opts: options
};

type pkg = {
    name: str,
    vers: str,
    uuid: str,
    desc: option<str>,
    sigs: option<str>,
    crate_type: option<str>
};

type options = {
    test: bool,
    mode: mode,
    free: [str],
};

enum mode { system_mode, user_mode, local_mode }

fn opts() -> [getopts::opt] {
    [optflag("g"), optflag("G"), optopt("mode"), optflag("test")]
}

fn info(msg: str) {
    io::stdout().write_line("info: " + msg);
}

fn warn(msg: str) {
    io::stdout().write_line("warning: " + msg);
}

fn error(msg: str) {
    io::stdout().write_line("error: " + msg);
}

fn load_link(mis: [@ast::meta_item]) -> (option<str>,
                                         option<str>,
                                         option<str>) {
    let mut name = none;
    let mut vers = none;
    let mut uuid = none;
    for a: @ast::meta_item in mis {
        alt a.node {
            ast::meta_name_value(v, {node: ast::lit_str(s), span: _}) {
                alt v {
                    "name" { name = some(s); }
                    "vers" { vers = some(s); }
                    "uuid" { uuid = some(s); }
                    _ { }
                }
            }
            _ { fail "load_link: meta items must be name-values"; }
        }
    }
    (name, vers, uuid)
}

fn load_pkg(filename: str) -> option<pkg> {
    let cm = codemap::new_codemap();
    let handler = diagnostic::mk_handler(none);
    let sess = @{
        cm: cm,
        mut next_id: 1,
        span_diagnostic: diagnostic::mk_span_handler(handler, cm),
        mut chpos: 0u,
        mut byte_pos: 0u
    };
    let c = parser::parse_crate_from_crate_file(filename, [], sess);

    let mut name = none;
    let mut vers = none;
    let mut uuid = none;
    let mut desc = none;
    let mut sigs = none;
    let mut crate_type = none;

    for a in c.node.attrs {
        alt a.node.value.node {
            ast::meta_name_value(v, {node: ast::lit_str(s), span: _}) {
                alt v {
                    "desc" { desc = some(v); }
                    "sigs" { sigs = some(v); }
                    "crate_type" { crate_type = some(v); }
                    _ { }
                }
            }
            ast::meta_list(v, mis) {
                if v == "link" {
                    let (n, v, u) = load_link(mis);
                    name = n;
                    vers = v;
                    uuid = u;
                }
            }
            _ { fail "load_pkg: pkg attributes may not contain meta_words"; }
        }
    }

    alt (name, vers, uuid) {
        (some(name0), some(vers0), some(uuid0)) {
            some({
                name: name0,
                vers: vers0,
                uuid: uuid0,
                desc: desc,
                sigs: sigs,
                crate_type: crate_type})
        }
        _ { ret none; }
    }
}

fn print(s: str) {
    io::stdout().write_line(s);
}

fn rest(s: str, start: uint) -> str {
    if (start >= str::len(s)) {
        ""
    } else {
        str::slice(s, start, str::len(s))
    }
}

fn need_dir(s: str) {
    if os::path_is_dir(s) { ret; }
    if !os::make_dir(s, 0x1c0i32) {
        fail #fmt["can't make_dir %s", s];
    }
}

fn parse_source(name: str, j: json::json) -> source {
    alt j {
        json::dict(_j) {
            let url = alt _j.find("url") {
                some(json::string(u)) {
                    u
                }
                _ { fail "Needed 'url' field in source."; }
            };
            let sig = alt _j.find("sig") {
                some(json::string(u)) {
                    some(u)
                }
                _ { none }
            };
            let key = alt _j.find("key") {
                some(json::string(u)) {
                    some(u)
                }
                _ { none }
            };
            let keyfp = alt _j.find("keyfp") {
                some(json::string(u)) {
                    some(u)
                }
                _ { none }
            };
            ret { name: name, url: url, sig: sig, key: key, keyfp: keyfp,
                  mut packages: [] };
        }
        _ { fail "Needed dict value in source."; }
    };
}

fn try_parse_sources(filename: str, sources: map::hashmap<str, source>) {
    if !os::path_exists(filename)  { ret; }
    let c = io::read_whole_file_str(filename);
    alt json::from_str(result::get(c)) {
        ok(json::dict(j)) {
            j.items { |k, v|
                sources.insert(k, parse_source(k, v));
                #debug("source: %s", k);
            }
        }
        ok(_) { fail "malformed sources.json"; }
        err(e) { fail #fmt("%s:%u:%u: %s", filename, e.line, e.col, e.msg); }
    }
}

fn load_one_source_package(&src: source, p: map::hashmap<str, json::json>) {
    let name = alt p.find("name") {
        some(json::string(_n)) { _n }
        _ {
            warn("Malformed source json: " + src.name + " (missing name)");
            ret;
        }
    };

    let uuid = alt p.find("uuid") {
        some(json::string(_n)) { _n }
        _ {
            warn("Malformed source json: " + src.name + " (missing uuid)");
            ret;
        }
    };

    let url = alt p.find("url") {
        some(json::string(_n)) { _n }
        _ {
            warn("Malformed source json: " + src.name + " (missing url)");
            ret;
        }
    };

    let method = alt p.find("method") {
        some(json::string(_n)) { _n }
        _ {
            warn("Malformed source json: " + src.name + " (missing method)");
            ret;
        }
    };

    let ref = alt p.find("ref") {
        some(json::string(_n)) { some(_n) }
        _ { none }
    };

    let mut tags = [];
    alt p.find("tags") {
        some(json::list(js)) {
            for j in js {
                alt j {
                    json::string(_j) { vec::grow(tags, 1u, _j); }
                    _ { }
                }
            }
        }
        _ { }
    }

    let description = alt p.find("description") {
        some(json::string(_n)) { _n }
        _ {
            warn("Malformed source json: " + src.name
                 + " (missing description)");
            ret;
        }
    };

    vec::grow(src.packages, 1u, {
        // source: _source(src),
        name: name,
        uuid: uuid,
        url: url,
        method: method,
        description: description,
        ref: ref,
        tags: tags
    });
    log(debug, "  Loaded package: " + src.name + "/" + name);
}

fn load_source_packages(&c: cargo, &src: source) {
    log(debug, "Loading source: " + src.name);
    let dir = path::connect(c.sourcedir, src.name);
    let pkgfile = path::connect(dir, "packages.json");
    if !os::path_exists(pkgfile) { ret; }
    let pkgstr = io::read_whole_file_str(pkgfile);
    alt json::from_str(result::get(pkgstr)) {
        ok(json::list(js)) {
            for _j: json::json in js {
                alt _j {
                    json::dict(_p) {
                        load_one_source_package(src, _p);
                    }
                    _ {
                        warn("Malformed source json: " + src.name +
                             " (non-dict pkg)");
                    }
                }
            }
        }
        ok(_) {
            warn("Malformed source json: " + src.name +
                 "(packages is not a list)");
        }
        err(e) {
            warn(#fmt("%s:%u:%u: %s", src.name, e.line, e.col, e.msg));
        }
    };
}

fn build_cargo_options(argv: [str]) -> options {
    let match = alt getopts::getopts(argv, opts()) {
        result::ok(m) { m }
        result::err(f) {
            fail #fmt["%s", getopts::fail_str(f)];
        }
    };

    let test = opt_present(match, "test");
    let G = opt_present(match, "G");
    let g = opt_present(match, "g");
    let m = opt_present(match, "mode");
    let is_install = vec::len(match.free) > 1u && match.free[1] == "install";

    if G && g { fail "-G and -g both provided"; }
    if g && m { fail "--mode and -g both provided"; }
    if G && m { fail "--mode and -G both provided"; }

    let mode = if is_install {
        if G { system_mode }
        else if g { user_mode }
        else if m {
            alt getopts::opt_str(match, "mode") {
                "system" { system_mode }
                "user" { user_mode }
                "local" { local_mode }
                _ { fail "argument to `mode` must be one of `system`" +
                    ", `user`, or `local`";
                }
            }
        } else { local_mode }
    } else { system_mode };

    {test: test, mode: mode, free: match.free}
}

fn configure(opts: options) -> cargo {
    let syscargo = result::get(get_cargo_sysroot());
    let get_cargo_dir = alt opts.mode {
        system_mode { get_cargo_sysroot }
        user_mode { get_cargo_root }
        local_mode { get_cargo_root_nearest }
    };

    let p = alt get_cargo_dir() {
        result::ok(p) { p }
        result::err(e) { fail e }
    };

    let sources = map::str_hash::<source>();
    try_parse_sources(path::connect(syscargo, "sources.json"), sources);
    try_parse_sources(path::connect(syscargo, "local-sources.json"), sources);
    let mut c = {
        pgp: pgp::supported(),
        root: p,
        bindir: path::connect(p, "bin"),
        libdir: path::connect(p, "lib"),
        workdir: path::connect(p, "work"),
        sourcedir: path::connect(syscargo, "sources"),
        sources: sources,
        opts: opts
    };

    need_dir(c.root);
    need_dir(c.sourcedir);
    need_dir(c.workdir);
    need_dir(c.libdir);
    need_dir(c.bindir);

    sources.keys { |k|
        let mut s = sources.get(k);
        load_source_packages(c, s);
        sources.insert(k, s);
    };

    if c.pgp {
        pgp::init(c.root);
    } else {
        warn("command \"gpg\" is not found");
        warn("you have to install \"gpg\" from source " +
             " or package manager to get it to work correctly");
    }

    c
}

fn for_each_package(c: cargo, b: fn(source, package)) {
    c.sources.values({ |v|
        for p in copy v.packages {
            b(v, p);
        }
    })
}

// Runs all programs in directory <buildpath>
fn run_programs(buildpath: str) {
    let newv = os::list_dir_path(buildpath);
    for ct: str in newv {
        run::run_program(ct, []);
    }
}

// Runs rustc in <path + subdir> with the given flags
// and returns <path + subdir>
fn run_in_buildpath(what: str, path: str, subdir: str, cf: str,
                    extra_flags: [str]) -> option<str> {
    let buildpath = path::connect(path, subdir);
    need_dir(buildpath);
    #debug("%s: %s -> %s", what, cf, buildpath);
    let p = run::program_output(rustc_sysroot(),
                                ["--out-dir", buildpath, cf] + extra_flags);
    if p.status != 0 {
        error(#fmt["rustc failed: %d\n%s\n%s", p.status, p.err, p.out]);
        ret none;
    }
    some(buildpath)
}

fn test_one_crate(_c: cargo, path: str, cf: str) {
  let buildpath = alt run_in_buildpath("Testing", path, "/test", cf,
                                       [ "--test"]) {
      none { ret; }
      some(bp) { bp }
  };
  run_programs(buildpath);
}

fn install_one_crate(c: cargo, path: str, cf: str) {
    let buildpath = alt run_in_buildpath("Installing", path,
                                         "/build", cf, []) {
      none { ret; }
      some(bp) { bp }
    };
    let newv = os::list_dir_path(buildpath);
    let exec_suffix = os::exe_suffix();
    for ct: str in newv {
        if (exec_suffix != "" && str::ends_with(ct, exec_suffix)) ||
            (exec_suffix == "" && !str::starts_with(path::basename(ct),
                                                    "lib")) {
            #debug("  bin: %s", ct);
            // FIXME: need libstd os::copy or something (Issue #1983)
            run::run_program("cp", [ct, c.bindir]);
            if c.opts.mode == system_mode {
                install_one_crate_to_sysroot(ct, "bin");
            }
        } else {
            #debug("  lib: %s", ct);
            run::run_program("cp", [ct, c.libdir]);
            if c.opts.mode == system_mode {
                install_one_crate_to_sysroot(ct, libdir());
            }
        }
    }
}

fn install_one_crate_to_sysroot(ct: str, target: str) {
    alt os::self_exe_path() {
        some(_path) {
            let path = [_path, "..", target];
            check vec::is_not_empty(path);
            let target_dir = path::normalize(path::connect_many(path));
            let p = run::program_output("cp", [ct, target_dir]);
            if p.status != 0 {
                warn(#fmt["Copying %s to %s is failed", ct, target_dir]);
            }
        }
        none { }
    }
}

fn rustc_sysroot() -> str {
    alt os::self_exe_path() {
        some(_path) {
            let path = [_path, "..", "bin", "rustc"];
            check vec::is_not_empty(path);
            let rustc = path::normalize(path::connect_many(path));
            #debug("  rustc: %s", rustc);
            rustc
        }
        none { "rustc" }
    }
}

fn install_source(c: cargo, path: str) {
    #debug("source: %s", path);
    os::change_dir(path);
    let contents = os::list_dir_path(".");

    #debug("contents: %s", str::connect(contents, ", "));

    let cratefiles =
        vec::filter::<str>(contents, { |n| str::ends_with(n, ".rc") });

    if vec::is_empty(cratefiles) {
        fail "This doesn't look like a rust package (no .rc files).";
    }

    for cf: str in cratefiles {
        let p = load_pkg(cf);
        alt p {
            none { cont; }
            some(_) {
                if c.opts.test {
                    test_one_crate(c, path, cf);
                }
                install_one_crate(c, path, cf);
            }
        }
    }
}

fn install_git(c: cargo, wd: str, url: str, ref: option<str>) {
    run::run_program("git", ["clone", url, wd]);
    if option::is_some::<str>(ref) {
        let r = option::get::<str>(ref);
        os::change_dir(wd);
        run::run_program("git", ["checkout", r]);
    }

    install_source(c, wd);
}

fn install_curl(c: cargo, wd: str, url: str) {
    let tarpath = path::connect(wd, "pkg.tar");
    let p = run::program_output("curl", ["-f", "-s", "-o",
                                         tarpath, url]);
    if p.status != 0 {
        fail #fmt["Fetch of %s failed: %s", url, p.err];
    }
    run::run_program("tar", ["-x", "--strip-components=1",
                             "-C", wd, "-f", tarpath]);
    install_source(c, wd);
}

fn install_file(c: cargo, wd: str, path: str) {
    run::run_program("tar", ["-x", "--strip-components=1",
                             "-C", wd, "-f", path]);
    install_source(c, wd);
}

fn install_package(c: cargo, wd: str, pkg: package) {
    info("Installing with " + pkg.method + " from " + pkg.url + "...");
    if pkg.method == "git" {
        install_git(c, wd, pkg.url, pkg.ref);
    } else if pkg.method == "http" {
        install_curl(c, wd, pkg.url);
    } else if pkg.method == "file" {
        install_file(c, wd, pkg.url);
    }
}

fn cargo_suggestion(c: cargo, syncing: bool, fallback: fn())
{
    if c.sources.size() == 0u {
        error("No sources defined. You may wish to run " +
              "\"cargo init\" then \"cargo sync\".");
        ret;
    }
    if !syncing {
        let mut npkg = 0u;
        c.sources.values({ |v| npkg += vec::len(v.packages) });
        if npkg == 0u {
            error("No packages known. You may wish to run " +
                  "\"cargo sync\".");
            ret;
        }
    }
    fallback();
}

fn install_uuid(c: cargo, wd: str, uuid: str) {
    let mut ps = [];
    for_each_package(c, { |s, p|
        info(#fmt["%s ? %s", p.uuid, uuid]);
        if p.uuid == uuid {
            vec::grow(ps, 1u, (s, p));
        }
    });
    if vec::len(ps) == 1u {
        let (_, p) = ps[0];
        install_package(c, wd, p);
        ret;
    } else if vec::len(ps) == 0u {
        cargo_suggestion(c, false, { || error("No packages match uuid."); });
        ret;
    }
    error("Found multiple packages:");
    for (s,p) in ps {
        info("  " + s.name + "/" + p.uuid + " (" + p.name + ")");
    }
}

fn install_named(c: cargo, wd: str, name: str) {
    let mut ps = [];
    for_each_package(c, { |s, p|
        if p.name == name {
            vec::grow(ps, 1u, (s, p));
        }
    });
    if vec::len(ps) == 1u {
        let (_, p) = ps[0];
        install_package(c, wd, p);
        ret;
    } else if vec::len(ps) == 0u {
        cargo_suggestion(c, false, { || error("No packages match name."); });
        ret;
    }
    error("Found multiple packages:");
    for (s,p) in ps {
        info("  " + s.name + "/" + p.uuid + " (" + p.name + ")");
    }
}

fn install_uuid_specific(c: cargo, wd: str, src: str, uuid: str) {
    alt c.sources.find(src) {
        some(s) {
            if vec::any(copy s.packages, { |p|
                if p.uuid == uuid {
                    install_package(c, wd, p);
                    true
                } else { false }
            }) { ret; }
        }
        _ { }
    }
    error("Can't find package " + src + "/" + uuid);
}

fn install_named_specific(c: cargo, wd: str, src: str, name: str) {
    alt c.sources.find(src) {
        some(s) {
            if vec::any(copy s.packages, { |p|
                if p.name == name {
                    install_package(c, wd, p);
                    true
                } else { false }
            }) { ret; }
        }
        _ { }
    }
    error("Can't find package " + src + "/" + name);
}

fn cmd_install(c: cargo) unsafe {
    // cargo install <pkg>
    if vec::len(c.opts.free) < 3u {
        cmd_usage();
        ret;
    }

    let target = c.opts.free[2];

    let wd = alt tempfile::mkdtemp(c.workdir + path::path_sep(), "") {
        some(_wd) { _wd }
        none { fail "needed temp dir"; }
    };

    if str::starts_with(target, "uuid:") {
        let mut uuid = rest(target, 5u);
        alt str::find_char(uuid, '/') {
            option::some(idx) {
               let source = str::slice(uuid, 0u, idx);
               uuid = str::slice(uuid, idx + 1u, str::len(uuid));
               install_uuid_specific(c, wd, source, uuid);
            }
            option::none {
               install_uuid(c, wd, uuid);
            }
        }
    } else {
        let mut name = target;
        alt str::find_char(name, '/') {
            option::some(idx) {
               let source = str::slice(name, 0u, idx);
               name = str::slice(name, idx + 1u, str::len(name));
               install_named_specific(c, wd, source, name);
            }
            option::none {
               install_named(c, wd, name);
            }
        }
    }
}

fn sync_one(c: cargo, name: str, src: source) {
    let dir = path::connect(c.sourcedir, name);
    let pkgfile = path::connect(dir, "packages.json.new");
    let destpkgfile = path::connect(dir, "packages.json");
    let sigfile = path::connect(dir, "packages.json.sig");
    let keyfile = path::connect(dir, "key.gpg");
    let url = src.url;
    need_dir(dir);
    info(#fmt["fetching source %s...", name]);
    let p = run::program_output("curl", ["-f", "-s", "-o", pkgfile, url]);
    if p.status != 0 {
        warn(#fmt["fetch for source %s (url %s) failed", name, url]);
    } else {
        info(#fmt["fetched source: %s", name]);
    }
    alt src.sig {
        some(u) {
            let p = run::program_output("curl", ["-f", "-s", "-o", sigfile,
                                                 u]);
            if p.status != 0 {
                warn(#fmt["fetch for source %s (sig %s) failed", name, u]);
            }
        }
        _ { }
    }
    alt src.key {
        some(u) {
            let p = run::program_output("curl",  ["-f", "-s", "-o", keyfile,
                                                  u]);
            if p.status != 0 {
                warn(#fmt["fetch for source %s (key %s) failed", name, u]);
            }
            pgp::add(c.root, keyfile);
        }
        _ { }
    }
    alt (src.sig, src.key, src.keyfp) {
        (some(_), some(_), some(f)) {
            let r = pgp::verify(c.root, pkgfile, sigfile, f);
            if !r {
                warn(#fmt["signature verification failed for source %s",
                          name]);
            } else {
                info(#fmt["signature ok for source %s", name]);
            }
        }
        _ {
            info(#fmt["no signature for source %s", name]);
        }
    }
    run::run_program("cp", [pkgfile, destpkgfile]);
}

fn cmd_sync(c: cargo) {
    if vec::len(c.opts.free) == 3u {
        sync_one(c, c.opts.free[2], c.sources.get(c.opts.free[2]));
    } else {
        cargo_suggestion(c, true, { || } );
        c.sources.items { |k, v|
            sync_one(c, k, v);
        }
    }
}

fn cmd_init(c: cargo) {
    let srcurl = "http://www.rust-lang.org/cargo/sources.json";
    let sigurl = "http://www.rust-lang.org/cargo/sources.json.sig";

    let srcfile = path::connect(c.root, "sources.json.new");
    let sigfile = path::connect(c.root, "sources.json.sig");
    let destsrcfile = path::connect(c.root, "sources.json");

    let p = run::program_output("curl", ["-f", "-s", "-o", srcfile, srcurl]);
    if p.status != 0 {
        warn(#fmt["fetch of sources.json failed: %s", p.out]);
        ret;
    }

    let p = run::program_output("curl", ["-f", "-s", "-o", sigfile, sigurl]);
    if p.status != 0 {
        warn(#fmt["fetch of sources.json.sig failed: %s", p.out]);
        ret;
    }

    let r = pgp::verify(c.root, srcfile, sigfile, pgp::signing_key_fp());
    if !r {
        warn(#fmt["signature verification failed for sources.json"]);
    } else {
        info(#fmt["signature ok for sources.json"]);
    }
    run::run_program("cp", [srcfile, destsrcfile]);

    info(#fmt["Initialized .cargo in %s", c.root]);
}

fn print_pkg(s: source, p: package) {
    let mut m = s.name + "/" + p.name + " (" + p.uuid + ")";
    if vec::len(p.tags) > 0u {
        m = m + " [" + str::connect(p.tags, ", ") + "]";
    }
    info(m);
    if p.description != "" {
        print("   >> " + p.description + "\n")
    }
}
fn cmd_list(c: cargo) {
    for_each_package(c, { |s, p|
        if vec::len(c.opts.free) <= 2u || c.opts.free[2] == s.name {
            print_pkg(s, p);
        }
    });
}

fn cmd_search(c: cargo) {
    if vec::len(c.opts.free) < 3u {
        cmd_usage();
        ret;
    }
    let mut n = 0;
    let name = c.opts.free[2];
    let tags = vec::slice(c.opts.free, 3u, vec::len(c.opts.free));
    for_each_package(c, { |s, p|
        if (str::contains(p.name, name) || name == "*") &&
            vec::all(tags, { |t| vec::contains(p.tags, t) }) {
            print_pkg(s, p);
            n += 1;
        }
    });
    info(#fmt["Found %d packages.", n]);
}

fn cmd_usage() {
    print("Usage: cargo <verb> [options] [args...]" +
          "

    init                                          Set up .cargo
    install [options] [source/]package-name       Install by name
    install [options] uuid:[source/]package-uuid  Install by uuid
    list [source]                                 List packages
    search <name | '*'> [tags...]                 Search packages
    sync                                          Sync all sources
    usage                                         This

Options:

  cargo install

    --mode=[system,user,local]   change mode as (system/user/local)
    -g                           equivalent to --mode=user
    -G                           equivalent to --mode=system

NOTE:
\"cargo install\" installs bin/libs to local-level .cargo by default.
To install them into user-level .cargo,  use option -g/--mode=user.
To install them into bin/lib on sysroot, use option -G/--mode=system.
");
}

fn main(argv: [str]) {
    let o = build_cargo_options(argv);

    if vec::len(o.free) < 2u {
        cmd_usage();
        ret;
    }

    let c = configure(o);

    alt o.free[1] {
        "init" { cmd_init(c); }
        "install" { cmd_install(c); }
        "list" { cmd_list(c); }
        "search" { cmd_search(c); }
        "sync" { cmd_sync(c); }
        "usage" { cmd_usage(); }
        _ { cmd_usage(); }
    }
}
