use crate::thir::pattern::MatchCheckCtxt;
use rustc_errors::Handler;
use rustc_errors::{
    error_code, Applicability, DiagnosticBuilder, ErrorGuaranteed, IntoDiagnostic, MultiSpan,
};
use rustc_macros::{Diagnostic, LintDiagnostic, Subdiagnostic};
use rustc_middle::ty::{self, Ty};
use rustc_span::{symbol::Ident, Span};

#[derive(LintDiagnostic)]
#[diag(mir_build_unconditional_recursion)]
#[help]
pub struct UnconditionalRecursion {
    #[label]
    pub span: Span,
    #[label(mir_build_unconditional_recursion_call_site_label)]
    pub call_sites: Vec<Span>,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_call_to_unsafe_fn_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnCallToUnsafeFunctionRequiresUnsafe<'a> {
    #[label]
    pub span: Span,
    pub function: &'a str,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_call_to_unsafe_fn_requires_unsafe_nameless)]
#[note]
pub struct UnsafeOpInUnsafeFnCallToUnsafeFunctionRequiresUnsafeNameless {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_inline_assembly_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnUseOfInlineAssemblyRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_initializing_type_with_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnInitializingTypeWithRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_mutable_static_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnUseOfMutableStaticRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_extern_static_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnUseOfExternStaticRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_deref_raw_pointer_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnDerefOfRawPointerRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_union_field_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnAccessToUnionFieldRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_mutation_of_layout_constrained_field_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnMutationOfLayoutConstrainedFieldRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_borrow_of_layout_constrained_field_requires_unsafe)]
pub struct UnsafeOpInUnsafeFnBorrowOfLayoutConstrainedFieldRequiresUnsafe {
    #[label]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unsafe_op_in_unsafe_fn_call_to_fn_with_requires_unsafe)]
#[note]
pub struct UnsafeOpInUnsafeFnCallToFunctionWithRequiresUnsafe<'a> {
    #[label]
    pub span: Span,
    pub function: &'a str,
}

#[derive(Diagnostic)]
#[diag(mir_build_call_to_unsafe_fn_requires_unsafe, code = "E0133")]
#[note]
pub struct CallToUnsafeFunctionRequiresUnsafe<'a> {
    #[primary_span]
    #[label]
    pub span: Span,
    pub function: &'a str,
}

#[derive(Diagnostic)]
#[diag(mir_build_call_to_unsafe_fn_requires_unsafe_nameless, code = "E0133")]
#[note]
pub struct CallToUnsafeFunctionRequiresUnsafeNameless {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_call_to_unsafe_fn_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct CallToUnsafeFunctionRequiresUnsafeUnsafeOpInUnsafeFnAllowed<'a> {
    #[primary_span]
    #[label]
    pub span: Span,
    pub function: &'a str,
}

#[derive(Diagnostic)]
#[diag(
    mir_build_call_to_unsafe_fn_requires_unsafe_nameless_unsafe_op_in_unsafe_fn_allowed,
    code = "E0133"
)]
#[note]
pub struct CallToUnsafeFunctionRequiresUnsafeNamelessUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_inline_assembly_requires_unsafe, code = "E0133")]
#[note]
pub struct UseOfInlineAssemblyRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_inline_assembly_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct UseOfInlineAssemblyRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_initializing_type_with_requires_unsafe, code = "E0133")]
#[note]
pub struct InitializingTypeWithRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(
    mir_build_initializing_type_with_requires_unsafe_unsafe_op_in_unsafe_fn_allowed,
    code = "E0133"
)]
#[note]
pub struct InitializingTypeWithRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_mutable_static_requires_unsafe, code = "E0133")]
#[note]
pub struct UseOfMutableStaticRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_mutable_static_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct UseOfMutableStaticRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_extern_static_requires_unsafe, code = "E0133")]
#[note]
pub struct UseOfExternStaticRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_extern_static_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct UseOfExternStaticRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_deref_raw_pointer_requires_unsafe, code = "E0133")]
#[note]
pub struct DerefOfRawPointerRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_deref_raw_pointer_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct DerefOfRawPointerRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_union_field_requires_unsafe, code = "E0133")]
#[note]
pub struct AccessToUnionFieldRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_union_field_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct AccessToUnionFieldRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_mutation_of_layout_constrained_field_requires_unsafe, code = "E0133")]
#[note]
pub struct MutationOfLayoutConstrainedFieldRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(
    mir_build_mutation_of_layout_constrained_field_requires_unsafe_unsafe_op_in_unsafe_fn_allowed,
    code = "E0133"
)]
#[note]
pub struct MutationOfLayoutConstrainedFieldRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_borrow_of_layout_constrained_field_requires_unsafe, code = "E0133")]
#[note]
pub struct BorrowOfLayoutConstrainedFieldRequiresUnsafe {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(
    mir_build_borrow_of_layout_constrained_field_requires_unsafe_unsafe_op_in_unsafe_fn_allowed,
    code = "E0133"
)]
#[note]
pub struct BorrowOfLayoutConstrainedFieldRequiresUnsafeUnsafeOpInUnsafeFnAllowed {
    #[primary_span]
    #[label]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_call_to_fn_with_requires_unsafe, code = "E0133")]
#[note]
pub struct CallToFunctionWithRequiresUnsafe<'a> {
    #[primary_span]
    #[label]
    pub span: Span,
    pub function: &'a str,
}

#[derive(Diagnostic)]
#[diag(mir_build_call_to_fn_with_requires_unsafe_unsafe_op_in_unsafe_fn_allowed, code = "E0133")]
#[note]
pub struct CallToFunctionWithRequiresUnsafeUnsafeOpInUnsafeFnAllowed<'a> {
    #[primary_span]
    #[label]
    pub span: Span,
    pub function: &'a str,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unused_unsafe)]
pub struct UnusedUnsafe {
    #[label]
    pub span: Span,
    #[subdiagnostic]
    pub enclosing: Option<UnusedUnsafeEnclosing>,
}

#[derive(Subdiagnostic)]
pub enum UnusedUnsafeEnclosing {
    #[label(mir_build_unused_unsafe_enclosing_block_label)]
    Block {
        #[primary_span]
        span: Span,
    },
    #[label(mir_build_unused_unsafe_enclosing_fn_label)]
    Function {
        #[primary_span]
        span: Span,
    },
}

pub(crate) struct NonExhaustivePatternsTypeNotEmpty<'p, 'tcx, 'm> {
    pub cx: &'m MatchCheckCtxt<'p, 'tcx>,
    pub expr_span: Span,
    pub span: Span,
    pub ty: Ty<'tcx>,
}

impl<'a> IntoDiagnostic<'a> for NonExhaustivePatternsTypeNotEmpty<'_, '_, '_> {
    fn into_diagnostic(self, handler: &'a Handler) -> DiagnosticBuilder<'_, ErrorGuaranteed> {
        let mut diag = handler.struct_span_err_with_code(
            self.span,
            rustc_errors::fluent::mir_build_non_exhaustive_patterns_type_not_empty,
            error_code!(E0004),
        );

        let peeled_ty = self.ty.peel_refs();
        diag.set_arg("ty", self.ty);
        diag.set_arg("peeled_ty", peeled_ty);

        if let ty::Adt(def, _) = peeled_ty.kind() {
            let def_span = self
                .cx
                .tcx
                .hir()
                .get_if_local(def.did())
                .and_then(|node| node.ident())
                .map(|ident| ident.span)
                .unwrap_or_else(|| self.cx.tcx.def_span(def.did()));

            // workaround to make test pass
            let mut span: MultiSpan = def_span.into();
            span.push_span_label(def_span, "");

            diag.span_note(span, rustc_errors::fluent::def_note);
        }

        let is_variant_list_non_exhaustive = match self.ty.kind() {
            ty::Adt(def, _) if def.is_variant_list_non_exhaustive() && !def.did().is_local() => {
                true
            }
            _ => false,
        };

        if is_variant_list_non_exhaustive {
            diag.note(rustc_errors::fluent::non_exhaustive_type_note);
        } else {
            diag.note(rustc_errors::fluent::type_note);
        }

        if let ty::Ref(_, sub_ty, _) = self.ty.kind() {
            if !sub_ty.is_inhabited_from(self.cx.tcx, self.cx.module, self.cx.param_env) {
                diag.note(rustc_errors::fluent::reference_note);
            }
        }

        let mut suggestion = None;
        let sm = self.cx.tcx.sess.source_map();
        if self.span.eq_ctxt(self.expr_span) {
            // Get the span for the empty match body `{}`.
            let (indentation, more) = if let Some(snippet) = sm.indentation_before(self.span) {
                (format!("\n{}", snippet), "    ")
            } else {
                (" ".to_string(), "")
            };
            suggestion = Some((
                self.span.shrink_to_hi().with_hi(self.expr_span.hi()),
                format!(
                    " {{{indentation}{more}_ => todo!(),{indentation}}}",
                    indentation = indentation,
                    more = more,
                ),
            ));
        }

        if let Some((span, sugg)) = suggestion {
            diag.span_suggestion_verbose(
                span,
                rustc_errors::fluent::suggestion,
                sugg,
                Applicability::HasPlaceholders,
            );
        } else {
            diag.help(rustc_errors::fluent::help);
        }

        diag
    }
}

#[derive(Diagnostic)]
#[diag(mir_build_static_in_pattern, code = "E0158")]
pub struct StaticInPattern {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_assoc_const_in_pattern, code = "E0158")]
pub struct AssocConstInPattern {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_const_param_in_pattern, code = "E0158")]
pub struct ConstParamInPattern {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_non_const_path, code = "E0080")]
pub struct NonConstPath {
    #[primary_span]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_unreachable_pattern)]
pub struct UnreachablePattern {
    #[label]
    pub span: Option<Span>,
    #[label(catchall_label)]
    pub catchall: Option<Span>,
}

#[derive(Diagnostic)]
#[diag(mir_build_const_pattern_depends_on_generic_parameter)]
pub struct ConstPatternDependsOnGenericParameter {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_could_not_eval_const_pattern)]
pub struct CouldNotEvalConstPattern {
    #[primary_span]
    pub span: Span,
}

#[derive(Diagnostic)]
#[diag(mir_build_lower_range_bound_must_be_less_than_or_equal_to_upper, code = "E0030")]
pub struct LowerRangeBoundMustBeLessThanOrEqualToUpper {
    #[primary_span]
    #[label]
    pub span: Span,
    #[note(teach_note)]
    pub teach: Option<()>,
}

#[derive(Diagnostic)]
#[diag(mir_build_lower_range_bound_must_be_less_than_upper, code = "E0579")]
pub struct LowerRangeBoundMustBeLessThanUpper {
    #[primary_span]
    pub span: Span,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_leading_irrefutable_let_patterns)]
#[note]
#[help]
pub struct LeadingIrrefutableLetPatterns {
    pub count: usize,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_trailing_irrefutable_let_patterns)]
#[note]
#[help]
pub struct TrailingIrrefutableLetPatterns {
    pub count: usize,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_bindings_with_variant_name, code = "E0170")]
pub struct BindingsWithVariantName {
    #[suggestion(code = "{ty_path}::{ident}", applicability = "machine-applicable")]
    pub suggestion: Option<Span>,
    pub ty_path: String,
    pub ident: Ident,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_irrefutable_let_patterns_generic_let)]
#[note]
#[help]
pub struct IrrefutableLetPatternsGenericLet {
    pub count: usize,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_irrefutable_let_patterns_if_let)]
#[note]
#[help]
pub struct IrrefutableLetPatternsIfLet {
    pub count: usize,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_irrefutable_let_patterns_if_let_guard)]
#[note]
#[help]
pub struct IrrefutableLetPatternsIfLetGuard {
    pub count: usize,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_irrefutable_let_patterns_let_else)]
#[note]
#[help]
pub struct IrrefutableLetPatternsLetElse {
    pub count: usize,
}

#[derive(LintDiagnostic)]
#[diag(mir_build_irrefutable_let_patterns_while_let)]
#[note]
#[help]
pub struct IrrefutableLetPatternsWhileLet {
    pub count: usize,
}

#[derive(Diagnostic)]
#[diag(mir_build_borrow_of_moved_value)]
pub struct BorrowOfMovedValue<'tcx> {
    #[primary_span]
    pub span: Span,
    #[label]
    #[label(occurs_because_label)]
    pub binding_span: Span,
    #[label(value_borrowed_label)]
    pub conflicts_ref: Vec<Span>,
    pub name: Ident,
    pub ty: Ty<'tcx>,
    #[suggestion(code = "ref ", applicability = "machine-applicable")]
    pub suggest_borrowing: Option<Span>,
}

#[derive(Diagnostic)]
#[diag(mir_build_multiple_mut_borrows)]
pub struct MultipleMutBorrows {
    #[primary_span]
    pub span: Span,
    #[label]
    pub binding_span: Span,
    #[subdiagnostic]
    pub occurences: Vec<MultipleMutBorrowOccurence>,
    pub name: Ident,
}

#[derive(Subdiagnostic)]
pub enum MultipleMutBorrowOccurence {
    #[label(mutable_borrow)]
    Mutable {
        #[primary_span]
        span: Span,
        name_mut: Ident,
    },
    #[label(immutable_borrow)]
    Immutable {
        #[primary_span]
        span: Span,
        name_immut: Ident,
    },
    #[label(moved)]
    Moved {
        #[primary_span]
        span: Span,
        name_moved: Ident,
    },
}
