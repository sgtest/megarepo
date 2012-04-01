import io::writer_util;

import rustc::syntax::{ast, ast_util, fold, visit, codemap};
import rustc::syntax::parse::parser;
import rustc::syntax::print::pprust;
import rustc::driver::diagnostic;

enum test_mode { tm_converge, tm_run, }
type context = { mode: test_mode }; // + rng

fn write_file(filename: str, content: str) {
    result::get(
        io::file_writer(filename, [io::create, io::truncate]))
        .write_str(content);
}

fn contains(haystack: str, needle: str) -> bool {
    str::contains(haystack, needle)
}

fn find_rust_files(&files: [str], path: str) {
    if str::ends_with(path, ".rs") && !contains(path, "utf8") {
        // ignoring "utf8" tests because something is broken
        files += [path];
    } else if os::path_is_dir(path)
        && !contains(path, "compile-fail")
        && !contains(path, "build") {
        for p in os::list_dir_path(path) {
            find_rust_files(files, p);
        }
    }
}


fn common_exprs() -> [ast::expr] {
    fn dse(e: ast::expr_) -> ast::expr {
        { id: 0, node: e, span: ast_util::dummy_sp() }
    }

    fn dsl(l: ast::lit_) -> ast::lit {
        { node: l, span: ast_util::dummy_sp() }
    }

    [dse(ast::expr_break),
     dse(ast::expr_cont),
     dse(ast::expr_fail(option::none)),
     dse(ast::expr_fail(option::some(@dse(ast::expr_lit(@dsl(ast::lit_str("boo"))))))),
     dse(ast::expr_ret(option::none)),
     dse(ast::expr_lit(@dsl(ast::lit_nil))),
     dse(ast::expr_lit(@dsl(ast::lit_bool(false)))),
     dse(ast::expr_lit(@dsl(ast::lit_bool(true)))),
     dse(ast::expr_unary(ast::box(ast::m_imm),
                         @dse(ast::expr_lit(@dsl(ast::lit_bool(true)))))),
     dse(ast::expr_unary(ast::uniq(ast::m_imm),
                         @dse(ast::expr_lit(@dsl(ast::lit_bool(true))))))
    ]
}

pure fn safe_to_steal_expr(e: @ast::expr, tm: test_mode) -> bool {
    safe_to_use_expr(*e, tm)
}

pure fn safe_to_use_expr(e: ast::expr, tm: test_mode) -> bool {
    alt tm {
      tm_converge {
        alt e.node {
          // If the fuzzer moves a block-ending-in-semicolon into callee position,
          // the pretty-printer can't preserve this even by parenthesizing!!
          // See email to marijn.
          ast::expr_if(_, _, _) { false }
          ast::expr_if_check(_, _, _) { false }
          ast::expr_block(_) { false }
          ast::expr_alt(_, _, _) { false }
          ast::expr_for(_, _, _) { false }
          ast::expr_while(_, _) { false }

          // https://github.com/mozilla/rust/issues/955
          ast::expr_do_while(_, _) { false }

          // https://github.com/mozilla/rust/issues/929
          ast::expr_cast(_, _) { false }
          ast::expr_assert(_) { false }
          ast::expr_binary(_, _, _) { false }
          ast::expr_assign(_, _) { false }
          ast::expr_assign_op(_, _, _) { false }

          ast::expr_fail(option::none) { false }
          ast::expr_ret(option::none) { false }

          // https://github.com/mozilla/rust/issues/953
          ast::expr_fail(option::some(_)) { false }

          // https://github.com/mozilla/rust/issues/927
          //ast::expr_assert(_) { false }
          ast::expr_check(_, _) { false }

          // https://github.com/mozilla/rust/issues/928
          //ast::expr_cast(_, _) { false }

          // https://github.com/mozilla/rust/issues/1458
          ast::expr_call(_, _, _) { false }

          _ { true }
        }
      }
      tm_run { true }
    }
}

fn safe_to_steal_ty(t: @ast::ty, tm: test_mode) -> bool {
    alt t.node {
        // https://github.com/mozilla/rust/issues/971
        ast::ty_constr(_, _) { false }

        // Other restrictions happen to be the same.
        _ { safe_to_replace_ty(t.node, tm) }
    }
}

// Not type-parameterized: https://github.com/mozilla/rust/issues/898 (FIXED)
fn stash_expr_if(c: fn@(@ast::expr, test_mode)->bool,
                 es: @mut [ast::expr],
                 e: @ast::expr,
                 tm: test_mode) {
    if c(e, tm) {
        *es += [*e];
    } else {/* now my indices are wrong :( */ }
}

fn stash_ty_if(c: fn@(@ast::ty, test_mode)->bool,
               es: @mut [ast::ty],
               e: @ast::ty,
               tm: test_mode) {
    if c(e, tm) {
        *es += [*e];
    } else {/* now my indices are wrong :( */ }
}

type stolen_stuff = {exprs: [ast::expr], tys: [ast::ty]};

fn steal(crate: ast::crate, tm: test_mode) -> stolen_stuff {
    let exprs = @mut [];
    let tys = @mut [];
    let v = visit::mk_simple_visitor(@{
        visit_expr: bind stash_expr_if(safe_to_steal_expr, exprs, _, tm),
        visit_ty: bind stash_ty_if(safe_to_steal_ty, tys, _, tm)
        with *visit::default_simple_visitor()
    });
    visit::visit_crate(crate, (), v);
    {exprs: *exprs, tys: *tys}
}


fn safe_to_replace_expr(e: ast::expr_, _tm: test_mode) -> bool {
    alt e {
      // https://github.com/mozilla/rust/issues/652
      ast::expr_if(_, _, _) { false }
      ast::expr_block(_) { false }

      // expr_call is also missing a constraint
      ast::expr_fn_block(_, _) { false }

      _ { true }
    }
}

fn safe_to_replace_ty(t: ast::ty_, _tm: test_mode) -> bool {
    alt t {
      ast::ty_infer { false } // always implicit, always top level
      ast::ty_bot { false }   // in source, can only appear as the out type of a function
      ast::ty_mac(_) { false }
      _ { true }
    }
}

// Replace the |i|th expr (in fold order) of |crate| with |newexpr|.
fn replace_expr_in_crate(crate: ast::crate, i: uint, newexpr: ast::expr, tm: test_mode) ->
   ast::crate {
    let j: @mut uint = @mut 0u;
    fn fold_expr_rep(j_: @mut uint, i_: uint, newexpr_: ast::expr_,
                     original: ast::expr_, fld: fold::ast_fold, tm_: test_mode) ->
       ast::expr_ {
        *j_ += 1u;
        if i_ + 1u == *j_ && safe_to_replace_expr(original, tm_) {
            newexpr_
        } else {
            fold::noop_fold_expr(original, fld)
        }
    }
    let afp =
        {fold_expr: fold::wrap(bind fold_expr_rep(j, i, newexpr.node, _, _, tm))
            with *fold::default_ast_fold()};
    let af = fold::make_fold(afp);
    let crate2: @ast::crate = @af.fold_crate(crate);
    *crate2
}


// Replace the |i|th ty (in fold order) of |crate| with |newty|.
fn replace_ty_in_crate(crate: ast::crate, i: uint, newty: ast::ty, tm: test_mode) ->
   ast::crate {
    let j: @mut uint = @mut 0u;
    fn fold_ty_rep(j_: @mut uint, i_: uint, newty_: ast::ty_,
                     original: ast::ty_, fld: fold::ast_fold, tm_: test_mode) ->
       ast::ty_ {
        *j_ += 1u;
        if i_ + 1u == *j_ && safe_to_replace_ty(original, tm_) {
            newty_
        } else { fold::noop_fold_ty(original, fld) }
    }
    let afp =
        {fold_ty: fold::wrap(bind fold_ty_rep(j, i, newty.node, _, _, tm))
            with *fold::default_ast_fold()};
    let af = fold::make_fold(afp);
    let crate2: @ast::crate = @af.fold_crate(crate);
    *crate2
}

fn under(n: uint, it: fn(uint)) {
    let mut i: uint = 0u;
    while i < n { it(i); i += 1u; }
}

fn devnull() -> io::writer { io::mem_buffer_writer(io::mem_buffer()) }

fn as_str(f: fn@(io::writer)) -> str {
    let buf = io::mem_buffer();
    f(io::mem_buffer_writer(buf));
    io::mem_buffer_str(buf)
}

fn check_variants_of_ast(crate: ast::crate, codemap: codemap::codemap,
                         filename: str, cx: context) {
    let stolen = steal(crate, cx.mode);
    let extra_exprs = vec::filter(common_exprs(),
                                  bind safe_to_use_expr(_, cx.mode));
    check_variants_T(crate, codemap, filename, "expr", extra_exprs + stolen.exprs, pprust::expr_to_str, replace_expr_in_crate, cx);
    check_variants_T(crate, codemap, filename, "ty", stolen.tys, pprust::ty_to_str, replace_ty_in_crate, cx);
}

fn check_variants_T<T: copy>(
  crate: ast::crate,
  codemap: codemap::codemap,
  filename: str,
  thing_label: str,
  things: [T],
  stringifier: fn@(@T) -> str,
  replacer: fn@(ast::crate, uint, T, test_mode) -> ast::crate,
  cx: context
  ) {
    #error("%s contains %u %s objects", filename, vec::len(things), thing_label);

    let L = vec::len(things);

    if L < 100u {
        under(uint::min(L, 20u)) {|i|
            log(error, "Replacing... #" + uint::str(i));
            under(uint::min(L, 30u)) {|j|
                log(error, "With... " + stringifier(@things[j]));
                let crate2 = @replacer(crate, i, things[j], cx.mode);
                // It would be best to test the *crate* for stability, but testing the
                // string for stability is easier and ok for now.
                let handler = diagnostic::mk_handler(none);
                let str3 =
                    @as_str(bind pprust::print_crate(
                        codemap,
                        diagnostic::mk_span_handler(handler, codemap),
                        crate2,
                        filename,
                        io::str_reader(""), _,
                        pprust::no_ann()));
                alt cx.mode {
                  tm_converge {
                    check_roundtrip_convergence(str3, 1u);
                  }
                  tm_run {
                    let file_label = #fmt("rusttmp/%s_%s_%u_%u", last_part(filename), thing_label, i, j);
                    let safe_to_run = !(content_is_dangerous_to_run(*str3) || has_raw_pointers(*crate2));
                    check_whole_compiler(*str3, file_label, safe_to_run);
                  }
                }
            }
        }
    }
}

fn last_part(filename: str) -> str {
  let ix = option::get(str::rfind_char(filename, '/'));
  str::slice(filename, ix + 1u, str::len(filename) - 3u)
}

enum happiness { passed, cleanly_rejected(str), known_bug(str), failed(str), }

// We'd find more bugs if we could take an AST here, but
// - that would find many "false positives" or unimportant bugs
// - that would be tricky, requiring use of tasks or serialization or randomness.
// This seems to find plenty of bugs as it is :)
fn check_whole_compiler(code: str, suggested_filename_prefix: str, allow_running: bool) {
    let filename = suggested_filename_prefix + ".rs";
    write_file(filename, code);

    let compile_result = check_compiling(filename);

    let run_result = alt (compile_result, allow_running) {
      (passed, true) { check_running(suggested_filename_prefix) }
      (h, _) { h }
    };

    alt run_result {
      passed | cleanly_rejected(_) | known_bug(_) {
        removeIfExists(suggested_filename_prefix);
        removeIfExists(suggested_filename_prefix + ".rs");
        removeDirIfExists(suggested_filename_prefix + ".dSYM");
      }
      failed(s) {
        log(error, "check_whole_compiler failure: " + s);
        log(error, "Saved as: " + filename);
      }
    }
}

fn removeIfExists(filename: str) {
    // So sketchy!
    assert !contains(filename, " ");
    run::program_output("bash", ["-c", "rm " + filename]);
}

fn removeDirIfExists(filename: str) {
    // So sketchy!
    assert !contains(filename, " ");
    run::program_output("bash", ["-c", "rm -r " + filename]);
}

fn check_running(exe_filename: str) -> happiness {
    let p = run::program_output("/Users/jruderman/scripts/timed_run_rust_program.py", [exe_filename]);
    let comb = p.out + "\n" + p.err;
    if str::len(comb) > 1u {
        log(error, "comb comb comb: " + comb);
    }

    if contains(comb, "Assertion failed:") {
        failed("C++ assertion failure")
    } else if contains(comb, "leaked memory in rust main loop") {
        // might also use exit code 134
        //failed("Leaked")
        known_bug("https://github.com/mozilla/rust/issues/910")
    } else if contains(comb, "src/rt/") {
        failed("Mentioned src/rt/")
    } else if contains(comb, "malloc") {
        failed("Mentioned malloc")
    } else {
        alt p.status {
            0         { passed }
            100       { cleanly_rejected("running: explicit fail") }
            101 | 247 { cleanly_rejected("running: timed out") }
            245 | 246 | 138 | 252 { known_bug("https://github.com/mozilla/rust/issues/1466") }
            136 | 248 { known_bug("SIGFPE - https://github.com/mozilla/rust/issues/944") }
            rc        { failed("Rust program ran but exited with status " + int::str(rc)) }
        }
    }
}

fn check_compiling(filename: str) -> happiness {
    let p = run::program_output(
            "/Users/jruderman/code/rust/build/x86_64-apple-darwin/stage1/bin/rustc",
            [filename]);

    //#error("Status: %d", p.status);
    if p.status == 0 {
        passed
    } else if p.err != "" {
        if contains(p.err, "error:") {
            cleanly_rejected("rejected with span_error")
        } else {
            log(error, "Stderr: " + p.err);
            failed("Unfamiliar error message")
        }
    } else if contains(p.out, "Assertion") && contains(p.out, "failed") {
        log(error, "Stdout: " + p.out);
        failed("Looks like an llvm assertion failure")
    } else if contains(p.out, "internal compiler error unimplemented") {
        known_bug("Something unimplemented")
    } else if contains(p.out, "internal compiler error") {
        log(error, "Stdout: " + p.out);
        failed("internal compiler error")

    } else {
        log(error, p.status);
        log(error, "!Stdout: " + p.out);
        failed("What happened?")
    }
}


fn parse_and_print(code: @str) -> str {
    let filename = "tmp.rs";
    let cm = codemap::new_codemap();
    let handler = diagnostic::mk_handler(none);
    let sess = @{
        cm: cm,
        mut next_id: 1,
        span_diagnostic: diagnostic::mk_span_handler(handler, cm),
        mut chpos: 0u,
        mut byte_pos: 0u
    };
    write_file(filename, *code);
    let crate = parser::parse_crate_from_source_str(
        filename, code, [], sess);
    io::with_str_reader(*code) { |rdr|
        as_str(bind pprust::print_crate(sess.cm,
                                        sess.span_diagnostic,
                                        crate,
                                        filename,
                                        rdr, _,
                                        pprust::no_ann()))
    }
}

fn has_raw_pointers(c: ast::crate) -> bool {
    let has_rp = @mut false;
    fn visit_ty(flag: @mut bool, t: @ast::ty) {
        alt t.node {
          ast::ty_ptr(_) { *flag = true; }
          _ { }
        }
    }
    let v =
        visit::mk_simple_visitor(@{visit_ty: bind visit_ty(has_rp, _)
                                      with *visit::default_simple_visitor()});
    visit::visit_crate(c, (), v);
    ret *has_rp;
}

fn content_is_dangerous_to_run(code: str) -> bool {
    let dangerous_patterns =
        ["xfail-test",
         "import",  // espeically fs, run
         "native",
         "unsafe",
         "log"];    // python --> rust pipe deadlock?

    for p: str in dangerous_patterns { if contains(code, p) { ret true; } }
    ret false;
}

fn content_is_dangerous_to_compile(code: str) -> bool {
    let dangerous_patterns =
        ["xfail-test"];

    for p: str in dangerous_patterns { if contains(code, p) { ret true; } }
    ret false;
}

fn content_might_not_converge(code: str) -> bool {
    let confusing_patterns =
        ["xfail-test",
         "xfail-pretty",
         "self",       // crazy rules enforced by parser rather than typechecker?
         "spawn",      // precedence issues?
         "bind",       // precedence issues?
         " be ",       // don't want to replace its child with a non-call: "Non-call expression in tail call"
         "\n\n\n\n\n"  // https://github.com/mozilla/rust/issues/850
        ];

    for p: str in confusing_patterns { if contains(code, p) { ret true; } }
    ret false;
}

fn file_might_not_converge(filename: str) -> bool {
    let confusing_files = [
      "expr-alt.rs", // pretty-printing "(a = b) = c" vs "a = b = c" and wrapping
      "block-arg-in-ternary.rs", // wrapping
      "move-3-unique.rs", // 0 becomes (0), but both seem reasonable. wtf?
      "move-3.rs"  // 0 becomes (0), but both seem reasonable. wtf?
    ];


    for f in confusing_files { if contains(filename, f) { ret true; } }

    ret false;
}

fn check_roundtrip_convergence(code: @str, maxIters: uint) {

    let mut i = 0u;
    let mut newv = code;
    let mut oldv = code;

    while i < maxIters {
        oldv = newv;
        if content_might_not_converge(*oldv) { ret; }
        newv = @parse_and_print(oldv);
        if oldv == newv { break; }
        i += 1u;
    }

    if oldv == newv {
        #error("Converged after %u iterations", i);
    } else {
        #error("Did not converge after %u iterations!", i);
        write_file("round-trip-a.rs", *oldv);
        write_file("round-trip-b.rs", *newv);
        run::run_program("diff",
                         ["-w", "-u", "round-trip-a.rs",
                          "round-trip-b.rs"]);
        fail "Mismatch";
    }
}

fn check_convergence(files: [str]) {
    #error("pp convergence tests: %u files", vec::len(files));
    for file in files {
        if !file_might_not_converge(file) {
            let s = @result::get(io::read_whole_file_str(file));
            if !content_might_not_converge(*s) {
                #error("pp converge: %s", file);
                // Change from 7u to 2u once https://github.com/mozilla/rust/issues/850 is fixed
                check_roundtrip_convergence(s, 7u);
            }
        }
    }
}

fn check_variants(files: [str], cx: context) {
    for file in files {
        if cx.mode == tm_converge && file_might_not_converge(file) {
            #error("Skipping convergence test based on file_might_not_converge");
            cont;
        }

        let s = @result::get(io::read_whole_file_str(file));
        if contains(*s, "#") {
            cont; // Macros are confusing
        }
        if cx.mode == tm_converge && content_might_not_converge(*s) {
            cont;
        }
        if cx.mode == tm_run && content_is_dangerous_to_compile(*s) {
            cont;
        }

        log(error, "check_variants: " + file);
        let cm = codemap::new_codemap();
        let handler = diagnostic::mk_handler(none);
        let sess = @{
            cm: cm,
            mut next_id: 1,
            span_diagnostic: diagnostic::mk_span_handler(handler, cm),
            mut chpos: 0u,
            mut byte_pos: 0u
        };
        let crate =
            parser::parse_crate_from_source_str(
                file,
                s, [], sess);
        io::with_str_reader(*s) { |rdr|
            #error("%s",
                   as_str(bind pprust::print_crate(sess.cm,
                                                   sess.span_diagnostic,
                                                   crate,
                                                   file,
                                                   rdr, _,
                                                   pprust::no_ann())));
        }
        check_variants_of_ast(*crate, sess.cm, file, cx);
    }
}

fn main(args: [str]) {
    if vec::len(args) != 2u {
        #error("usage: %s <testdir>", args[0]);
        ret;
    }
    let mut files = [];
    let root = args[1];

    find_rust_files(files, root);
    #error("== check_convergence ==");
    check_convergence(files);
    #error("== check_variants: converge ==");
    check_variants(files, { mode: tm_converge });
    #error("== check_variants: run ==");
    check_variants(files, { mode: tm_run });

    #error("Fuzzer done");
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
