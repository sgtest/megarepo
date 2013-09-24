// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * rusti - A REPL using the JIT backend
 *
 * Rusti works by serializing state between lines of input. This means that each
 * line can be run in a separate task, and the only limiting factor is that all
 * local bound variables are encodable.
 *
 * This is accomplished by feeding in generated input to rustc for execution in
 * the JIT compiler. Currently input actually gets fed in three times to get
 * information about the program.
 *
 * - Pass #1
 *   In this pass, the input is simply thrown at the parser and the input comes
 *   back. This validates the structure of the program, and at this stage the
 *   global items (fns, structs, impls, traits, etc.) are filtered from the
 *   input into the "global namespace". These declarations shadow all previous
 *   declarations of an item by the same name.
 *
 * - Pass #2
 *   After items have been stripped, the remaining input is passed to rustc
 *   along with all local variables declared (initialized to nothing). This pass
 *   runs up to typechecking. From this, we can learn about the types of each
 *   bound variable, what variables are bound, and also ensure that all the
 *   types are encodable (the input can actually be run).
 *
 * - Pass #3
 *   Finally, a program is generated to deserialize the local variable state,
 *   run the code input, and then reserialize all bindings back into a local
 *   hash map. This code is then run in the JIT engine provided by the rust
 *   compiler.
 *
 * - Pass #4
 *   Once this code runs, the input has fully been run and the hash map of local
 *   variables from TLS is read back into the local store of variables. This is
 *   then used later to pass back along to the parent rusti task and then begin
 *   waiting for input again.
 *
 * - Pass #5
 *   When running rusti code, it's important to consume ownership of the LLVM
 *   jit contextual information to prevent code from being deallocated too soon
 *   (before drop glue runs, see #7732). For this reason, the jit context is
 *   consumed and also passed along to the parent task. The parent task then
 *   keeps around all contexts while rusti is running. This must be done because
 *   tasks could in theory be spawned off and running in the background (still
 *   using the code).
 *
 * Encoding/decoding is done with EBML, and there is simply a map of ~str ->
 * ~[u8] maintaining the values of each local binding (by name).
 */

#[link(name = "rusti",
       vers = "0.8",
       uuid = "7fb5bf52-7d45-4fee-8325-5ad3311149fc",
       url = "https://github.com/mozilla/rust/tree/master/src/rusti")];

#[license = "MIT/ASL2"];
#[crate_type = "lib"];

extern mod extra;
extern mod rustc;
extern mod syntax;

use std::{libc, io, os, task};
use std::cell::Cell;
use extra::rl::CompletionCb;
use extra::rl;

use rustc::driver::{driver, session};
use rustc::back::link::jit;
use syntax::{ast, codemap, diagnostic};
use syntax::ast_util::*;
use syntax::diagnostic::Emitter;
use syntax::parse::token;
use syntax::print::pprust;

use program::Program;
use utils::*;

mod program;
pub mod utils;

/**
 * A structure shared across REPL instances for storing history
 * such as statements and view items. I wish the AST was sendable.
 */
pub struct Repl {
    prompt: ~str,
    binary: ~str,
    running: bool,
    lib_search_paths: ~[~str],
    engines: ~[~jit::Engine],

    program: ~Program,
}

// Action to do after reading a :command
enum CmdAction {
    action_none,
    action_run_line(~str),
}

struct EncodableWarningEmitter;

impl diagnostic::Emitter for EncodableWarningEmitter {
    fn emit(&self,
            cm: Option<(@codemap::CodeMap, codemap::Span)>,
            msg: &str,
            lvl: diagnostic::level) {
        diagnostic::DefaultEmitter.emit(cm, msg, lvl);
        if msg.contains("failed to find an implementation of trait") &&
           msg.contains("extra::serialize::Encodable") {
            diagnostic::DefaultEmitter.emit(cm,
                                            "Currrently rusti serializes \
                                             bound locals between different \
                                             lines of input. This means that \
                                             all values of local variables \
                                             need to be encodable, and this \
                                             type isn't encodable",
                                            diagnostic::note);
        }
    }
}

/// Run an input string in a Repl, returning the new Repl.
fn run(mut program: ~Program, binary: ~str, lib_search_paths: ~[~str],
       input: ~str) -> (~Program, Option<~jit::Engine>)
{
    // Build some necessary rustc boilerplate for compiling things
    let binary = binary.to_managed();
    let options = @session::options {
        crate_type: session::unknown_crate,
        binary: binary,
        addl_lib_search_paths: @mut lib_search_paths.map(|p| Path(*p)),
        jit: true,
        .. (*session::basic_options()).clone()
    };
    // Because we assume that everything is encodable (and assert so), add some
    // extra helpful information if the error crops up. Otherwise people are
    // bound to be very confused when they find out code is running that they
    // never typed in...
    let sess = driver::build_session(options,
                                     @EncodableWarningEmitter as
                                        @diagnostic::Emitter);
    let intr = token::get_ident_interner();

    //
    // Stage 1: parse the input and filter it into the program (as necessary)
    //
    debug!("parsing: %s", input);
    let crate = parse_input(sess, input);
    let mut to_run = ~[];       // statements to run (emitted back into code)
    let new_locals = @mut ~[];  // new locals being defined
    let mut result = None;      // resultant expression (to print via pp)
    do find_main(crate, sess) |blk| {
        // Fish out all the view items, be sure to record 'extern mod' items
        // differently beause they must appear before all 'use' statements
        for vi in blk.view_items.iter() {
            let s = do with_pp(intr) |pp, _| {
                pprust::print_view_item(pp, vi);
            };
            match vi.node {
                ast::view_item_extern_mod(*) => {
                    program.record_extern(s);
                }
                ast::view_item_use(*) => { program.record_view_item(s); }
            }
        }

        // Iterate through all of the block's statements, inserting them into
        // the correct portions of the program
        for stmt in blk.stmts.iter() {
            let s = do with_pp(intr) |pp, _| { pprust::print_stmt(pp, *stmt); };
            match stmt.node {
                ast::StmtDecl(d, _) => {
                    match d.node {
                        ast::DeclItem(it) => {
                            let name = sess.str_of(it.ident);
                            match it.node {
                                // Structs are treated specially because to make
                                // them at all usable they need to be decorated
                                // with #[deriving(Encoable, Decodable)]
                                ast::item_struct(*) => {
                                    program.record_struct(name, s);
                                }
                                // Item declarations are hoisted out of main()
                                _ => { program.record_item(name, s); }
                            }
                        }

                        // Local declarations must be specially dealt with,
                        // record all local declarations for use later on
                        ast::DeclLocal(l) => {
                            let mutbl = l.is_mutbl;
                            do each_binding(l) |path, _| {
                                let s = do with_pp(intr) |pp, _| {
                                    pprust::print_path(pp, path, false);
                                };
                                new_locals.push((s, mutbl));
                            }
                            to_run.push(s);
                        }
                    }
                }

                // run statements with expressions (they have effects)
                ast::StmtMac(*) | ast::StmtSemi(*) | ast::StmtExpr(*) => {
                    to_run.push(s);
                }
            }
        }
        result = do blk.expr.map_move |e| {
            do with_pp(intr) |pp, _| { pprust::print_expr(pp, e); }
        };
    }
    // return fast for empty inputs
    if to_run.len() == 0 && result.is_none() {
        return (program, None);
    }

    //
    // Stage 2: run everything up to typeck to learn the types of the new
    //          variables introduced into the program
    //
    info!("Learning about the new types in the program");
    program.set_cache(); // before register_new_vars (which changes them)
    let input = to_run.connect("\n");
    let test = program.test_code(input, &result, *new_locals);
    debug!("testing with ^^^^^^ %?", (||{ println(test) })());
    let dinput = driver::str_input(test.to_managed());
    let cfg = driver::build_configuration(sess);

    let crate = driver::phase_1_parse_input(sess, cfg.clone(), &dinput);
    let expanded_crate = driver::phase_2_configure_and_expand(sess, cfg, crate);
    let analysis = driver::phase_3_run_analysis_passes(sess, expanded_crate);

    // Once we're typechecked, record the types of all local variables defined
    // in this input
    do find_main(crate, sess) |blk| {
        program.register_new_vars(blk, analysis.ty_cx);
    }

    //
    // Stage 3: Actually run the code in the JIT
    //
    info!("actually running code");
    let code = program.code(input, &result);
    debug!("actually running ^^^^^^ %?", (||{ println(code) })());
    let input = driver::str_input(code.to_managed());
    let cfg = driver::build_configuration(sess);
    let outputs = driver::build_output_filenames(&input, &None, &None, [], sess);
    let sess = driver::build_session(options,
                                     @diagnostic::DefaultEmitter as
                                        @diagnostic::Emitter);

    let crate = driver::phase_1_parse_input(sess, cfg.clone(), &input);
    let expanded_crate = driver::phase_2_configure_and_expand(sess, cfg, crate);
    let analysis = driver::phase_3_run_analysis_passes(sess, expanded_crate);
    let trans = driver::phase_4_translate_to_llvm(sess, expanded_crate, &analysis, outputs);
    driver::phase_5_run_llvm_passes(sess, &trans, outputs);

    //
    // Stage 4: Inform the program that computation is done so it can update all
    //          local variable bindings.
    //
    info!("cleaning up after code");
    program.consume_cache();

    //
    // Stage 5: Extract the LLVM execution engine to take ownership of the
    //          generated JIT code. This means that rusti can spawn parallel
    //          tasks and we won't deallocate the code emitted until rusti
    //          itself is destroyed.
    //
    return (program, jit::consume_engine());

    fn parse_input(sess: session::Session, input: &str) -> @ast::Crate {
        let code = fmt!("fn main() {\n %s \n}", input);
        let input = driver::str_input(code.to_managed());
        let cfg = driver::build_configuration(sess);
        driver::phase_1_parse_input(sess, cfg.clone(), &input)
    }

    fn find_main(crate: @ast::Crate, sess: session::Session,
                 f: &fn(&ast::Block)) {
        for item in crate.module.items.iter() {
            match item.node {
                ast::item_fn(_, _, _, _, ref blk) => {
                    if item.ident == sess.ident_of("main") {
                        return f(blk);
                    }
                }
                _ => {}
            }
        }
        fail!("main function was expected somewhere...");
    }
}

// Compiles a crate given by the filename as a library if the compiled
// version doesn't exist or is older than the source file. Binary is
// the name of the compiling executable. Returns Some(true) if it
// successfully compiled, Some(false) if the crate wasn't compiled
// because it already exists and is newer than the source file, or
// None if there were compile errors.
fn compile_crate(src_filename: ~str, binary: ~str) -> Option<bool> {
    match do task::try {
        let src_path = Path(src_filename);
        let binary = binary.to_managed();
        let options = @session::options {
            binary: binary,
            addl_lib_search_paths: @mut ~[os::getcwd()],
            .. (*session::basic_options()).clone()
        };
        let input = driver::file_input(src_path.clone());
        let sess = driver::build_session(options,
                                         @diagnostic::DefaultEmitter as
                                            @diagnostic::Emitter);
        *sess.building_library = true;
        let cfg = driver::build_configuration(sess);
        let outputs = driver::build_output_filenames(
            &input, &None, &None, [], sess);
        // If the library already exists and is newer than the source
        // file, skip compilation and return None.
        let mut should_compile = true;
        let dir = os::list_dir_path(&Path(outputs.out_filename.dirname()));
        let maybe_lib_path = do dir.iter().find |file| {
            // The actual file's name has a hash value and version
            // number in it which is unknown at this time, so looking
            // for a file that matches out_filename won't work,
            // instead we guess which file is the library by matching
            // the prefix and suffix of out_filename to files in the
            // directory.
            let file_str = file.filename().unwrap();
            file_str.starts_with(outputs.out_filename.filestem().unwrap())
                && file_str.ends_with(outputs.out_filename.filetype().unwrap())
        };
        match maybe_lib_path {
            Some(lib_path) => {
                let (src_mtime, _) = src_path.get_mtime().unwrap();
                let (lib_mtime, _) = lib_path.get_mtime().unwrap();
                if lib_mtime >= src_mtime {
                    should_compile = false;
                }
            },
            None => { },
        }
        if (should_compile) {
            println(fmt!("compiling %s...", src_filename));
            let crate = driver::phase_1_parse_input(sess, cfg.clone(), &input);
            let expanded_crate = driver::phase_2_configure_and_expand(sess, cfg, crate);
            let analysis = driver::phase_3_run_analysis_passes(sess, expanded_crate);
            let trans = driver::phase_4_translate_to_llvm(sess, expanded_crate, &analysis, outputs);
            driver::phase_5_run_llvm_passes(sess, &trans, outputs);
            true
        } else { false }
    } {
        Ok(true) => Some(true),
        Ok(false) => Some(false),
        Err(_) => None,
    }
}

/// Tries to get a line from rl after outputting a prompt. Returns
/// None if no input was read (e.g. EOF was reached).
fn get_line(use_rl: bool, prompt: &str) -> Option<~str> {
    if use_rl {
        let result = rl::read(prompt);

        match result {
            None => None,
            Some(line) => {
                rl::add_history(line);
                Some(line)
            }
        }
    } else {
        if io::stdin().eof() {
            None
        } else {
            Some(io::stdin().read_line())
        }
    }
}

/// Run a command, e.g. :clear, :exit, etc.
fn run_cmd(repl: &mut Repl, _in: @io::Reader, _out: @io::Writer,
           cmd: ~str, args: ~[~str], use_rl: bool) -> CmdAction {
    let mut action = action_none;
    match cmd {
        ~"exit" => repl.running = false,
        ~"clear" => {
            repl.program.clear();

            // XXX: Win32 version of linenoise can't do this
            //rl::clear();
        }
        ~"help" => {
            println(
                ":{\\n ..lines.. \\n:}\\n - execute multiline command\n\
                 :load <crate> ... - loads given crates as dynamic libraries\n\
                 :clear - clear the bindings\n\
                 :exit - exit from the repl\n\
                 :help - show this message");
        }
        ~"load" => {
            let mut loaded_crates: ~[~str] = ~[];
            for arg in args.iter() {
                let (crate, filename) =
                    if arg.ends_with(".rs") || arg.ends_with(".rc") {
                    (arg.slice_to(arg.len() - 3).to_owned(), (*arg).clone())
                } else {
                    ((*arg).clone(), *arg + ".rs")
                };
                match compile_crate(filename, repl.binary.clone()) {
                    Some(_) => loaded_crates.push(crate),
                    None => { }
                }
            }
            for crate in loaded_crates.iter() {
                let crate_path = Path(*crate);
                let crate_dir = crate_path.dirname();
                repl.program.record_extern(fmt!("extern mod %s;", *crate));
                if !repl.lib_search_paths.iter().any(|x| x == &crate_dir) {
                    repl.lib_search_paths.push(crate_dir);
                }
            }
            if loaded_crates.is_empty() {
                println("no crates loaded");
            } else {
                printfln!("crates loaded: %s", loaded_crates.connect(", "));
            }
        }
        ~"{" => {
            let mut multiline_cmd = ~"";
            let mut end_multiline = false;
            while (!end_multiline) {
                match get_line(use_rl, "rusti| ") {
                    None => fail!("unterminated multiline command :{ .. :}"),
                    Some(line) => {
                        if line.trim() == ":}" {
                            end_multiline = true;
                        } else {
                            multiline_cmd.push_str(line);
                            multiline_cmd.push_char('\n');
                        }
                    }
                }
            }
            action = action_run_line(multiline_cmd);
        }
        _ => println(~"unknown cmd: " + cmd)
    }
    return action;
}

/// Executes a line of input, which may either be rust code or a
/// :command. Returns a new Repl if it has changed.
pub fn run_line(repl: &mut Repl, input: @io::Reader, out: @io::Writer, line: ~str,
                use_rl: bool) -> bool
{
    if line.starts_with(":") {
        // drop the : and the \n (one byte each)
        let full = line.slice(1, line.len());
        let split: ~[~str] = full.word_iter().map(|s| s.to_owned()).collect();
        let len = split.len();

        if len > 0 {
            let cmd = split[0].clone();

            if !cmd.is_empty() {
                let args = if len > 1 {
                    split.slice(1, len).to_owned()
                } else { ~[] };

                match run_cmd(repl, input, out, cmd, args, use_rl) {
                    action_none => { }
                    action_run_line(multiline_cmd) => {
                        if !multiline_cmd.is_empty() {
                            return run_line(repl, input, out, multiline_cmd, use_rl);
                        }
                    }
                }
                return true;
            }
        }
    }

    let line = Cell::new(line);
    let program = Cell::new(repl.program.clone());
    let lib_search_paths = Cell::new(repl.lib_search_paths.clone());
    let binary = Cell::new(repl.binary.clone());
    let result = do task::try {
        run(program.take(), binary.take(), lib_search_paths.take(), line.take())
    };

    match result {
        Ok((program, engine)) => {
            repl.program = program;
            match engine {
                Some(e) => { repl.engines.push(e); }
                None => {}
            }
            return true;
        }
        Err(*) => { return false; }
    }
}

pub fn main() {
    let args = os::args();
    main_args(args);
}

struct Completer;

impl CompletionCb for Completer {
    fn complete(&self, line: ~str, suggest: &fn(~str)) {
        if line.starts_with(":") {
            suggest(~":clear");
            suggest(~":exit");
            suggest(~":help");
            suggest(~":load");
        }
    }
}

pub fn main_args(args: &[~str]) {
    #[fixed_stack_segment]; #[inline(never)];

    let input = io::stdin();
    let out = io::stdout();
    let mut repl = Repl {
        prompt: ~"rusti> ",
        binary: args[0].clone(),
        running: true,
        lib_search_paths: ~[],
        engines: ~[],

        program: ~Program::new(),
    };

    let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

    // only print this stuff if the user is actually typing into rusti
    if istty {
        println("WARNING: The Rust REPL is experimental and may be");
        println("unstable. If you encounter problems, please use the");
        println("compiler instead. Type :help for help.");

        unsafe {
            rl::complete(@Completer as @CompletionCb)
        }
    }

    while repl.running {
        match get_line(istty, repl.prompt) {
            None => break,
            Some(line) => {
                if line.is_empty() {
                    if istty {
                        println("()");
                    }
                    loop;
                }
                run_line(&mut repl, input, out, line, istty);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use program::Program;
    use super::*;

    fn repl() -> Repl {
        Repl {
            prompt: ~"rusti> ",
            binary: ~"rusti",
            running: true,
            lib_search_paths: ~[],
            engines: ~[],
            program: ~Program::new(),
        }
    }

    // FIXME: #7220 rusti on 32bit mac doesn't work.
    // FIXME: #7641 rusti on 32bit linux cross compile doesn't work
    // FIXME: #7115 re-enable once LLVM has been upgraded
    #[cfg(thiswillneverbeacfgflag)]
    fn run_program(prog: &str) {
        let mut r = repl();
        for cmd in prog.split_iter('\n') {
            assert!(run_line(&mut r, io::stdin(), io::stdout(),
                             cmd.to_owned(), false),
                    "the command '%s' failed", cmd);
        }
    }
    fn run_program(_: &str) {}

    #[ignore]
    #[test]
    fn super_basic() {
        run_program("");
    }

    #[ignore]
    #[test]
    fn regression_5937() {
        run_program("use std::hashmap;");
    }

    #[ignore]
    #[test]
    fn regression_5784() {
        run_program("let a = 3;");
    }

    #[test] #[ignore]
    fn new_tasks() {
        // XXX: can't spawn new tasks because the JIT code is cleaned up
        //      after the main function is done.
        run_program("
            spawn( || println(\"Please don't segfault\") );
            do spawn { println(\"Please?\"); }
        ");
    }

    #[ignore]
    #[test]
    fn inferred_integers_usable() {
        run_program("let a = 2;\n()\n");
        run_program("
            let a = 3;
            let b = 4u;
            assert!((a as uint) + b == 7)
        ");
    }

    #[ignore]
    #[test]
    fn local_variables_allow_shadowing() {
        run_program("
            let a = 3;
            let a = 5;
            assert!(a == 5)
        ");
    }

    #[ignore]
    #[test]
    fn string_usable() {
        run_program("
            let a = ~\"\";
            let b = \"\";
            let c = @\"\";
            let d = a + b + c;
            assert!(d.len() == 0);
        ");
    }

    #[ignore]
    #[test]
    fn vectors_usable() {
        run_program("
            let a = ~[1, 2, 3];
            let b = &[1, 2, 3];
            let c = @[1, 2, 3];
            let d = a + b + c;
            assert!(d.len() == 9);
            let e: &[int] = [];
        ");
    }

    #[ignore]
    #[test]
    fn structs_usable() {
        run_program("
            struct A{ a: int }
            let b = A{ a: 3 };
            assert!(b.a == 3)
        ");
    }

    #[ignore]
    #[test]
    fn mutable_variables_work() {
        run_program("
            let mut a = 3;
            a = 5;
            let mut b = std::hashmap::HashSet::new::<int>();
            b.insert(a);
            assert!(b.contains(&5))
            assert!(b.len() == 1)
        ");
    }

    #[ignore]
    #[test]
    fn functions_saved() {
        run_program("
            fn fib(x: int) -> int { if x < 2 {x} else { fib(x - 1) + fib(x - 2) } }
            let a = fib(3);
            let a = a + fib(4);
            assert!(a == 5)
        ");
    }

    #[ignore]
    #[test]
    fn modules_saved() {
        run_program("
            mod b { pub fn foo() -> uint { 3 } }
            assert!(b::foo() == 3)
        ");
    }

    #[ignore]
    #[test]
    fn multiple_functions() {
        run_program("
            fn f() {}
            fn f() {}
            f()
        ");
    }

    #[ignore]
    #[test]
    fn multiple_items_same_name() {
        run_program("
            fn f() {}
            mod f {}
            struct f;
            enum f {}
            fn f() {}
            f()
        ");
    }

    #[ignore]
    #[test]
    fn simultaneous_definition_and_expression() {
        run_program("
            let a = 3; a as u8
        ");
    }

    #[ignore]
    #[test]
    fn exit_quits() {
        let mut r = repl();
        assert!(r.running);
        let result = run_line(&mut r, io::stdin(), io::stdout(),
                              ~":exit", false);
        assert!(result);
        assert!(!r.running);
    }
}
