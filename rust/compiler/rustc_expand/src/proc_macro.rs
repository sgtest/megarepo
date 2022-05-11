use crate::base::{self, *};
use crate::proc_macro_server;

use rustc_ast as ast;
use rustc_ast::ptr::P;
use rustc_ast::token;
use rustc_ast::tokenstream::{TokenStream, TokenTree};
use rustc_data_structures::sync::Lrc;
use rustc_errors::ErrorGuaranteed;
use rustc_parse::parser::ForceCollect;
use rustc_span::profiling::SpannedEventArgRecorder;
use rustc_span::{Span, DUMMY_SP};

const EXEC_STRATEGY: pm::bridge::server::SameThread = pm::bridge::server::SameThread;

pub struct BangProcMacro {
    pub client: pm::bridge::client::Client<fn(pm::TokenStream) -> pm::TokenStream>,
}

impl base::ProcMacro for BangProcMacro {
    fn expand<'cx>(
        &self,
        ecx: &'cx mut ExtCtxt<'_>,
        span: Span,
        input: TokenStream,
    ) -> Result<TokenStream, ErrorGuaranteed> {
        let _timer =
            ecx.sess.prof.generic_activity_with_arg_recorder("expand_proc_macro", |recorder| {
                recorder.record_arg_with_span(ecx.expansion_descr(), span);
            });

        let proc_macro_backtrace = ecx.ecfg.proc_macro_backtrace;
        let server = proc_macro_server::Rustc::new(ecx);
        self.client.run(&EXEC_STRATEGY, server, input, proc_macro_backtrace).map_err(|e| {
            let mut err = ecx.struct_span_err(span, "proc macro panicked");
            if let Some(s) = e.as_str() {
                err.help(&format!("message: {}", s));
            }
            err.emit()
        })
    }
}

pub struct AttrProcMacro {
    pub client: pm::bridge::client::Client<fn(pm::TokenStream, pm::TokenStream) -> pm::TokenStream>,
}

impl base::AttrProcMacro for AttrProcMacro {
    fn expand<'cx>(
        &self,
        ecx: &'cx mut ExtCtxt<'_>,
        span: Span,
        annotation: TokenStream,
        annotated: TokenStream,
    ) -> Result<TokenStream, ErrorGuaranteed> {
        let _timer =
            ecx.sess.prof.generic_activity_with_arg_recorder("expand_proc_macro", |recorder| {
                recorder.record_arg_with_span(ecx.expansion_descr(), span);
            });

        let proc_macro_backtrace = ecx.ecfg.proc_macro_backtrace;
        let server = proc_macro_server::Rustc::new(ecx);
        self.client
            .run(&EXEC_STRATEGY, server, annotation, annotated, proc_macro_backtrace)
            .map_err(|e| {
                let mut err = ecx.struct_span_err(span, "custom attribute panicked");
                if let Some(s) = e.as_str() {
                    err.help(&format!("message: {}", s));
                }
                err.emit()
            })
    }
}

pub struct ProcMacroDerive {
    pub client: pm::bridge::client::Client<fn(pm::TokenStream) -> pm::TokenStream>,
}

impl MultiItemModifier for ProcMacroDerive {
    fn expand(
        &self,
        ecx: &mut ExtCtxt<'_>,
        span: Span,
        _meta_item: &ast::MetaItem,
        item: Annotatable,
    ) -> ExpandResult<Vec<Annotatable>, Annotatable> {
        // We need special handling for statement items
        // (e.g. `fn foo() { #[derive(Debug)] struct Bar; }`)
        let is_stmt = matches!(item, Annotatable::Stmt(..));
        let hack = crate::base::ann_pretty_printing_compatibility_hack(&item, &ecx.sess.parse_sess);
        let input = if hack {
            let nt = match item {
                Annotatable::Item(item) => token::NtItem(item),
                Annotatable::Stmt(stmt) => token::NtStmt(stmt),
                _ => unreachable!(),
            };
            TokenTree::token(token::Interpolated(Lrc::new(nt)), DUMMY_SP).into()
        } else {
            item.to_tokens(&ecx.sess.parse_sess)
        };

        let stream = {
            let _timer =
                ecx.sess.prof.generic_activity_with_arg_recorder("expand_proc_macro", |recorder| {
                    recorder.record_arg_with_span(ecx.expansion_descr(), span);
                });
            let proc_macro_backtrace = ecx.ecfg.proc_macro_backtrace;
            let server = proc_macro_server::Rustc::new(ecx);
            match self.client.run(&EXEC_STRATEGY, server, input, proc_macro_backtrace) {
                Ok(stream) => stream,
                Err(e) => {
                    let mut err = ecx.struct_span_err(span, "proc-macro derive panicked");
                    if let Some(s) = e.as_str() {
                        err.help(&format!("message: {}", s));
                    }
                    err.emit();
                    return ExpandResult::Ready(vec![]);
                }
            }
        };

        let error_count_before = ecx.sess.parse_sess.span_diagnostic.err_count();
        let mut parser =
            rustc_parse::stream_to_parser(&ecx.sess.parse_sess, stream, Some("proc-macro derive"));
        let mut items = vec![];

        loop {
            match parser.parse_item(ForceCollect::No) {
                Ok(None) => break,
                Ok(Some(item)) => {
                    if is_stmt {
                        items.push(Annotatable::Stmt(P(ecx.stmt_item(span, item))));
                    } else {
                        items.push(Annotatable::Item(item));
                    }
                }
                Err(mut err) => {
                    err.emit();
                    break;
                }
            }
        }

        // fail if there have been errors emitted
        if ecx.sess.parse_sess.span_diagnostic.err_count() > error_count_before {
            ecx.struct_span_err(span, "proc-macro derive produced unparseable tokens").emit();
        }

        ExpandResult::Ready(items)
    }
}
