// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Code that generates a test runner to run all the tests in a crate


use driver::session;
use front::config;
use front::std_inject::with_version;
use metadata::creader::Loader;

use std::cell::RefCell;
use std::vec;
use syntax::ast_util::*;
use syntax::attr::AttrMetaMethods;
use syntax::attr;
use syntax::codemap::{DUMMY_SP, Span, ExpnInfo, NameAndSpan, MacroAttribute};
use syntax::codemap;
use syntax::ext::base::ExtCtxt;
use syntax::fold::Folder;
use syntax::fold;
use syntax::opt_vec;
use syntax::parse::token::InternedString;
use syntax::parse::token;
use syntax::print::pprust;
use syntax::{ast, ast_util};
use syntax::util::small_vector::SmallVector;

struct Test {
    span: Span,
    path: ~[ast::Ident],
    bench: bool,
    ignore: bool,
    should_fail: bool
}

struct TestCtxt<'a> {
    sess: session::Session,
    path: RefCell<~[ast::Ident]>,
    ext_cx: ExtCtxt<'a>,
    testfns: RefCell<~[Test]>,
    is_test_crate: bool,
    config: ast::CrateConfig,
}

// Traverse the crate, collecting all the test functions, eliding any
// existing main functions, and synthesizing a main test harness
pub fn modify_for_testing(sess: session::Session,
                          krate: ast::Crate) -> ast::Crate {
    // We generate the test harness when building in the 'test'
    // configuration, either with the '--test' or '--cfg test'
    // command line options.
    let should_test = attr::contains_name(krate.config, "test");

    if should_test {
        generate_test_harness(sess, krate)
    } else {
        strip_test_functions(krate)
    }
}

struct TestHarnessGenerator<'a> {
    cx: TestCtxt<'a>,
}

impl<'a> fold::Folder for TestHarnessGenerator<'a> {
    fn fold_crate(&mut self, c: ast::Crate) -> ast::Crate {
        let folded = fold::noop_fold_crate(c, self);

        // Add a special __test module to the crate that will contain code
        // generated for the test harness
        ast::Crate {
            module: add_test_module(&self.cx, &folded.module),
            .. folded
        }
    }

    fn fold_item(&mut self, i: @ast::Item) -> SmallVector<@ast::Item> {
        {
            let mut path = self.cx.path.borrow_mut();
            path.get().push(i.ident);
        }
        debug!("current path: {}",
               ast_util::path_name_i(self.cx.path.get()));

        if is_test_fn(&self.cx, i) || is_bench_fn(i) {
            match i.node {
                ast::ItemFn(_, purity, _, _, _)
                    if purity == ast::UnsafeFn => {
                    let sess = self.cx.sess;
                    sess.span_fatal(i.span,
                                    "unsafe functions cannot be used for \
                                     tests");
                }
                _ => {
                    debug!("this is a test function");
                    let test = Test {
                        span: i.span,
                        path: self.cx.path.get(),
                        bench: is_bench_fn(i),
                        ignore: is_ignored(&self.cx, i),
                        should_fail: should_fail(i)
                    };
                    {
                        let mut testfns = self.cx.testfns.borrow_mut();
                        testfns.get().push(test);
                    }
                    // debug!("have {} test/bench functions",
                    //        cx.testfns.len());
                }
            }
        }

        let res = fold::noop_fold_item(i, self);
        {
            let mut path = self.cx.path.borrow_mut();
            path.get().pop();
        }
        res
    }

    fn fold_mod(&mut self, m: &ast::Mod) -> ast::Mod {
        // Remove any #[main] from the AST so it doesn't clash with
        // the one we're going to add. Only if compiling an executable.

        fn nomain(cx: &TestCtxt, item: @ast::Item) -> @ast::Item {
            if !cx.sess.building_library.get() {
                @ast::Item {
                    attrs: item.attrs.iter().filter_map(|attr| {
                        if !attr.name().equiv(&("main")) {
                            Some(*attr)
                        } else {
                            None
                        }
                    }).collect(),
                    .. (*item).clone()
                }
            } else {
                item
            }
        }

        let mod_nomain = ast::Mod {
            view_items: m.view_items.clone(),
            items: m.items.iter().map(|i| nomain(&self.cx, *i)).collect(),
        };

        fold::noop_fold_mod(&mod_nomain, self)
    }
}

fn generate_test_harness(sess: session::Session, krate: ast::Crate)
                         -> ast::Crate {
    let loader = &mut Loader::new(sess);
    let mut cx: TestCtxt = TestCtxt {
        sess: sess,
        ext_cx: ExtCtxt::new(sess.parse_sess, sess.opts.cfg.clone(), loader),
        path: RefCell::new(~[]),
        testfns: RefCell::new(~[]),
        is_test_crate: is_test_crate(&krate),
        config: krate.config.clone(),
    };

    cx.ext_cx.bt_push(ExpnInfo {
        call_site: DUMMY_SP,
        callee: NameAndSpan {
            name: ~"test",
            format: MacroAttribute,
            span: None
        }
    });

    let mut fold = TestHarnessGenerator {
        cx: cx
    };
    let res = fold.fold_crate(krate);
    fold.cx.ext_cx.bt_pop();
    return res;
}

fn strip_test_functions(krate: ast::Crate) -> ast::Crate {
    // When not compiling with --test we should not compile the
    // #[test] functions
    config::strip_items(krate, |attrs| {
        !attr::contains_name(attrs, "test") &&
        !attr::contains_name(attrs, "bench")
    })
}

fn is_test_fn(cx: &TestCtxt, i: @ast::Item) -> bool {
    let has_test_attr = attr::contains_name(i.attrs, "test");

    fn has_test_signature(i: @ast::Item) -> bool {
        match &i.node {
          &ast::ItemFn(ref decl, _, _, ref generics, _) => {
            let no_output = match decl.output.node {
                ast::TyNil => true,
                _ => false
            };
            decl.inputs.is_empty()
                && no_output
                && !generics.is_parameterized()
          }
          _ => false
        }
    }

    if has_test_attr && !has_test_signature(i) {
        let sess = cx.sess;
        sess.span_err(
            i.span,
            "functions used as tests must have signature fn() -> ()."
        );
    }

    return has_test_attr && has_test_signature(i);
}

fn is_bench_fn(i: @ast::Item) -> bool {
    let has_bench_attr = attr::contains_name(i.attrs, "bench");

    fn has_test_signature(i: @ast::Item) -> bool {
        match i.node {
            ast::ItemFn(ref decl, _, _, ref generics, _) => {
                let input_cnt = decl.inputs.len();
                let no_output = match decl.output.node {
                    ast::TyNil => true,
                    _ => false
                };
                let tparm_cnt = generics.ty_params.len();
                // NB: inadequate check, but we're running
                // well before resolve, can't get too deep.
                input_cnt == 1u
                    && no_output && tparm_cnt == 0u
            }
          _ => false
        }
    }

    return has_bench_attr && has_test_signature(i);
}

fn is_ignored(cx: &TestCtxt, i: @ast::Item) -> bool {
    i.attrs.iter().any(|attr| {
        // check ignore(cfg(foo, bar))
        attr.name().equiv(&("ignore")) && match attr.meta_item_list() {
            Some(ref cfgs) => attr::test_cfg(cx.config, cfgs.iter().map(|x| *x)),
            None => true
        }
    })
}

fn should_fail(i: @ast::Item) -> bool {
    attr::contains_name(i.attrs, "should_fail")
}

fn add_test_module(cx: &TestCtxt, m: &ast::Mod) -> ast::Mod {
    let testmod = mk_test_module(cx);
    ast::Mod {
        items: vec::append_one(m.items.clone(), testmod),
        ..(*m).clone()
    }
}

/*

We're going to be building a module that looks more or less like:

mod __test {
  #[!resolve_unexported]
  extern crate test (name = "test", vers = "...");
  fn main() {
    test::test_main_static(::os::args(), tests)
  }

  static tests : &'static [test::TestDescAndFn] = &[
    ... the list of tests in the crate ...
  ];
}

*/

fn mk_std(cx: &TestCtxt) -> ast::ViewItem {
    let id_test = token::str_to_ident("test");
    let vi = if cx.is_test_crate {
        ast::ViewItemUse(
            ~[@nospan(ast::ViewPathSimple(id_test,
                                          path_node(~[id_test]),
                                          ast::DUMMY_NODE_ID))])
    } else {
        ast::ViewItemExternMod(id_test,
                               with_version("test"),
                               ast::DUMMY_NODE_ID)
    };
    ast::ViewItem {
        node: vi,
        attrs: ~[],
        vis: ast::Inherited,
        span: DUMMY_SP
    }
}

fn mk_test_module(cx: &TestCtxt) -> @ast::Item {

    // Link to test crate
    let view_items = ~[mk_std(cx)];

    // A constant vector of test descriptors.
    let tests = mk_tests(cx);

    // The synthesized main function which will call the console test runner
    // with our list of tests
    let mainfn = (quote_item!(&cx.ext_cx,
        pub fn main() {
            #[main];
            test::test_main_static(::std::os::args(), TESTS);
        }
    )).unwrap();

    let testmod = ast::Mod {
        view_items: view_items,
        items: ~[mainfn, tests],
    };
    let item_ = ast::ItemMod(testmod);

    // This attribute tells resolve to let us call unexported functions
    let resolve_unexported_str = InternedString::new("!resolve_unexported");
    let resolve_unexported_attr =
        attr::mk_attr(attr::mk_word_item(resolve_unexported_str));

    let item = ast::Item {
        ident: token::str_to_ident("__test"),
        attrs: ~[resolve_unexported_attr],
        id: ast::DUMMY_NODE_ID,
        node: item_,
        vis: ast::Public,
        span: DUMMY_SP,
     };

    debug!("Synthetic test module:\n{}\n", pprust::item_to_str(&item));

    return @item;
}

fn nospan<T>(t: T) -> codemap::Spanned<T> {
    codemap::Spanned { node: t, span: DUMMY_SP }
}

fn path_node(ids: ~[ast::Ident]) -> ast::Path {
    ast::Path {
        span: DUMMY_SP,
        global: false,
        segments: ids.move_iter().map(|identifier| ast::PathSegment {
            identifier: identifier,
            lifetimes: opt_vec::Empty,
            types: opt_vec::Empty,
        }).collect()
    }
}

fn path_node_global(ids: ~[ast::Ident]) -> ast::Path {
    ast::Path {
        span: DUMMY_SP,
        global: true,
        segments: ids.move_iter().map(|identifier| ast::PathSegment {
            identifier: identifier,
            lifetimes: opt_vec::Empty,
            types: opt_vec::Empty,
        }).collect()
    }
}

fn mk_tests(cx: &TestCtxt) -> @ast::Item {
    // The vector of test_descs for this crate
    let test_descs = mk_test_descs(cx);

    (quote_item!(&cx.ext_cx,
        pub static TESTS : &'static [self::test::TestDescAndFn] =
            $test_descs
        ;
    )).unwrap()
}

fn is_test_crate(krate: &ast::Crate) -> bool {
    match attr::find_crateid(krate.attrs) {
        Some(ref s) if "test" == s.name => true,
        _ => false
    }
}

fn mk_test_descs(cx: &TestCtxt) -> @ast::Expr {
    let mut descs = ~[];
    {
        let testfns = cx.testfns.borrow();
        debug!("building test vector from {} tests", testfns.get().len());
        for test in testfns.get().iter() {
            descs.push(mk_test_desc_and_fn_rec(cx, test));
        }
    }

    let inner_expr = @ast::Expr {
        id: ast::DUMMY_NODE_ID,
        node: ast::ExprVec(descs, ast::MutImmutable),
        span: DUMMY_SP,
    };

    @ast::Expr {
        id: ast::DUMMY_NODE_ID,
        node: ast::ExprVstore(inner_expr, ast::ExprVstoreSlice),
        span: DUMMY_SP,
    }
}

fn mk_test_desc_and_fn_rec(cx: &TestCtxt, test: &Test) -> @ast::Expr {
    let span = test.span;
    let path = test.path.clone();

    debug!("encoding {}", ast_util::path_name_i(path));

    let name_lit: ast::Lit =
        nospan(ast::LitStr(token::intern_and_get_ident(
                    ast_util::path_name_i(path)), ast::CookedStr));

    let name_expr = @ast::Expr {
          id: ast::DUMMY_NODE_ID,
          node: ast::ExprLit(@name_lit),
          span: span
    };

    let fn_path = path_node_global(path);

    let fn_expr = @ast::Expr {
        id: ast::DUMMY_NODE_ID,
        node: ast::ExprPath(fn_path),
        span: span,
    };

    let t_expr = if test.bench {
        quote_expr!(&cx.ext_cx, self::test::StaticBenchFn($fn_expr) )
    } else {
        quote_expr!(&cx.ext_cx, self::test::StaticTestFn($fn_expr) )
    };

    let ignore_expr = if test.ignore {
        quote_expr!(&cx.ext_cx, true )
    } else {
        quote_expr!(&cx.ext_cx, false )
    };

    let fail_expr = if test.should_fail {
        quote_expr!(&cx.ext_cx, true )
    } else {
        quote_expr!(&cx.ext_cx, false )
    };

    let e = quote_expr!(&cx.ext_cx,
        self::test::TestDescAndFn {
            desc: self::test::TestDesc {
                name: self::test::StaticTestName($name_expr),
                ignore: $ignore_expr,
                should_fail: $fail_expr
            },
            testfn: $t_expr,
        }
    );
    e
}
