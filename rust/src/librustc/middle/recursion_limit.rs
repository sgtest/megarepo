// Recursion limit.
//
// There are various parts of the compiler that must impose arbitrary limits
// on how deeply they recurse to prevent stack overflow. Users can override
// this via an attribute on the crate like `#![recursion_limit="22"]`. This pass
// just peeks and looks for that attribute.

use crate::session::Session;
use core::num::IntErrorKind;
use rustc::bug;
use rustc_span::symbol::{sym, Symbol};
use syntax::ast;

use rustc_data_structures::sync::Once;

pub fn update_limits(sess: &Session, krate: &ast::Crate) {
    update_limit(sess, krate, &sess.recursion_limit, sym::recursion_limit, 128);
    update_limit(sess, krate, &sess.type_length_limit, sym::type_length_limit, 1048576);
}

fn update_limit(
    sess: &Session,
    krate: &ast::Crate,
    limit: &Once<usize>,
    name: Symbol,
    default: usize,
) {
    for attr in &krate.attrs {
        if !attr.check_name(name) {
            continue;
        }

        if let Some(s) = attr.value_str() {
            match s.as_str().parse() {
                Ok(n) => {
                    limit.set(n);
                    return;
                }
                Err(e) => {
                    let mut err = sess.struct_span_err(
                        attr.span,
                        "`recursion_limit` must be a non-negative integer",
                    );

                    let value_span = attr
                        .meta()
                        .and_then(|meta| meta.name_value_literal().cloned())
                        .map(|lit| lit.span)
                        .unwrap_or(attr.span);

                    let error_str = match e.kind() {
                        IntErrorKind::Overflow => "`recursion_limit` is too large",
                        IntErrorKind::Empty => "`recursion_limit` must be a non-negative integer",
                        IntErrorKind::InvalidDigit => "not a valid integer",
                        IntErrorKind::Underflow => bug!("`recursion_limit` should never underflow"),
                        IntErrorKind::Zero => bug!("zero is a valid `recursion_limit`"),
                        kind => bug!("unimplemented IntErrorKind variant: {:?}", kind),
                    };

                    err.span_label(value_span, error_str);
                    err.emit();
                }
            }
        }
    }
    limit.set(default);
}
