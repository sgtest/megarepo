import driver::session;
import driver::session::session;

import std::map::hashmap;

import syntax::attr;

import syntax::ast::{crate, expr_, expr_mac, mac_invoc};
import syntax::fold::*;
import syntax::ext::base::*;
import syntax::ext::qquote::{qq_helper};
import syntax::parse::parser;
import syntax::parse::parser::parse_expr_from_source_str;


import codemap::{span, expanded_from};

fn expand_expr(exts: hashmap<str, syntax_extension>, cx: ext_ctxt,
               e: expr_, s: span, fld: ast_fold,
               orig: fn@(expr_, span, ast_fold) -> (expr_, span))
    -> (expr_, span)
{
    ret alt e {
          expr_mac(mac) {
            alt mac.node {
              mac_invoc(pth, args, body) {
                assert (vec::len(pth.node.idents) > 0u);
                let extname = pth.node.idents[0];
                alt exts.find(extname) {
                  none {
                    cx.span_fatal(pth.span,
                                  #fmt["macro undefined: '%s'", extname])
                  }
                  some(item_decorator(_)) {
                    cx.span_fatal(
                        pth.span,
                        #fmt["%s can only be used as a decorator", extname]);
                  }
                  some(normal({expander: exp, span: exp_sp})) {
                    let expanded = exp(cx, pth.span, args, body);

                    let info = {call_site: s,
                                callie: {name: extname, span: exp_sp}};
                    cx.bt_push(expanded_from(info));
                    //keep going, outside-in
                    let fully_expanded = fld.fold_expr(expanded).node;
                    cx.bt_pop();

                    (fully_expanded, s)
                  }
                  some(macro_defining(ext)) {
                    let named_extension = ext(cx, pth.span, args, body);
                    exts.insert(named_extension.ident, named_extension.ext);
                    (ast::expr_rec([], none), s)
                  }
                }
              }
              _ { cx.span_bug(mac.span, "naked syntactic bit") }
            }
          }
          _ { orig(e, s, fld) }
        };
}

fn expand_mod_items(exts: hashmap<str, syntax_extension>, cx: ext_ctxt,
                    module: ast::_mod, fld: ast_fold,
                    orig: fn@(ast::_mod, ast_fold) -> ast::_mod)
    -> ast::_mod
{
    // Fold the contents first:
    let module = orig(module, fld);

    // For each item, look through the attributes.  If any of them are
    // decorated with "item decorators", then use that function to transform
    // the item into a new set of items.
    let new_items = vec::flat_map(module.items) {|item|
        vec::foldr(item.attrs, [item]) {|attr, items|
            let mname = alt attr.node.value.node {
              ast::meta_word(n) { n }
              ast::meta_name_value(n, _) { n }
              ast::meta_list(n, _) { n }
            };
            alt exts.find(mname) {
              none { items }

              some(normal(_)) | some(macro_defining(_)) {
                cx.span_err(
                    attr.span,
                    #fmt["%s cannot be used as a decorator", mname]);
                items
              }

              some(item_decorator(dec_fn)) {
                dec_fn(cx, attr.span, attr.node.value, items)
              }
            }
        }
    };

    ret {items: new_items with module};
}

fn new_span(cx: ext_ctxt, sp: span) -> span {
    /* this discards information in the case of macro-defining macros */
    ret {lo: sp.lo, hi: sp.hi, expn_info: cx.backtrace()};
}

// FIXME: this is a terrible kludge to inject some macros into the default
// compilation environment. When the macro-definition system is substantially
// more mature, these should move from here, into a compiled part of libcore
// at very least.

fn core_macros() -> str {
    ret
"{
    #macro([#error[f, ...], log(core::error, #fmt[f, ...])]);
    #macro([#warn[f, ...], log(core::warn, #fmt[f, ...])]);
    #macro([#info[f, ...], log(core::info, #fmt[f, ...])]);
    #macro([#debug[f, ...], log(core::debug, #fmt[f, ...])]);
}";
}

fn expand_crate(parse_sess: parser::parse_sess,
                cfg: ast::crate_cfg, c: @crate) -> @crate {
    let exts = syntax_expander_table();
    let afp = default_ast_fold();
    let cx: ext_ctxt = mk_ctxt(parse_sess, cfg);
    let f_pre =
        {fold_expr: bind expand_expr(exts, cx, _, _, _, afp.fold_expr),
         fold_mod: bind expand_mod_items(exts, cx, _, _, afp.fold_mod),
         new_span: bind new_span(cx, _)
            with *afp};
    let f = make_fold(f_pre);
    let cm = parse_expr_from_source_str("<core-macros>",
                                        @core_macros(),
                                        cfg,
                                        parse_sess);

    // This is run for its side-effects on the expander env,
    // as it registers all the core macros as expanders.
    f.fold_expr(cm);

    let res = @f.fold_crate(*c);
    ret res;
}
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
