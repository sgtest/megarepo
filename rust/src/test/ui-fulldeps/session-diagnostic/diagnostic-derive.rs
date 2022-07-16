// check-fail
// Tests error conditions for specifying diagnostics using #[derive(SessionDiagnostic)]

// normalize-stderr-test "the following other types implement trait `IntoDiagnosticArg`:(?:.*\n){0,9}\s+and \d+ others" -> "normalized in stderr"

// The proc_macro2 crate handles spans differently when on beta/stable release rather than nightly,
// changing the output of this test. Since SessionDiagnostic is strictly internal to the compiler
// the test is just ignored on stable and beta:
// ignore-beta
// ignore-stable

#![feature(rustc_private)]
#![crate_type = "lib"]

extern crate rustc_span;
use rustc_span::symbol::Ident;
use rustc_span::Span;

extern crate rustc_macros;
use rustc_macros::{SessionDiagnostic, LintDiagnostic, SessionSubdiagnostic};

extern crate rustc_middle;
use rustc_middle::ty::Ty;

extern crate rustc_errors;
use rustc_errors::{Applicability, MultiSpan};

extern crate rustc_session;

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct Hello {}

#[derive(SessionDiagnostic)]
#[warning(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct HelloWarn {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
//~^ ERROR `#[derive(SessionDiagnostic)]` can only be used on structs
enum SessionDiagnosticOnEnum {
    Foo,
    Bar,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[error = "E0123"]
//~^ ERROR `#[error = ...]` is not a valid attribute
struct WrongStructAttrStyle {}

#[derive(SessionDiagnostic)]
#[nonsense(typeck::ambiguous_lifetime_bound, code = "E0123")]
//~^ ERROR `#[nonsense(...)]` is not a valid attribute
//~^^ ERROR diagnostic kind not specified
//~^^^ ERROR cannot find attribute `nonsense` in this scope
struct InvalidStructAttr {}

#[derive(SessionDiagnostic)]
#[error("E0123")]
//~^ ERROR `#[error("...")]` is not a valid attribute
//~^^ ERROR diagnostic slug not specified
struct InvalidLitNestedAttr {}

#[derive(SessionDiagnostic)]
#[error(nonsense, code = "E0123")]
//~^ ERROR cannot find value `nonsense` in module `rustc_errors::fluent`
struct InvalidNestedStructAttr {}

#[derive(SessionDiagnostic)]
#[error(nonsense("foo"), code = "E0123", slug = "foo")]
//~^ ERROR `#[error(nonsense(...))]` is not a valid attribute
//~^^ ERROR diagnostic slug not specified
struct InvalidNestedStructAttr1 {}

#[derive(SessionDiagnostic)]
#[error(nonsense = "...", code = "E0123", slug = "foo")]
//~^ ERROR `#[error(nonsense = ...)]` is not a valid attribute
//~^^ ERROR diagnostic slug not specified
struct InvalidNestedStructAttr2 {}

#[derive(SessionDiagnostic)]
#[error(nonsense = 4, code = "E0123", slug = "foo")]
//~^ ERROR `#[error(nonsense = ...)]` is not a valid attribute
//~^^ ERROR diagnostic slug not specified
struct InvalidNestedStructAttr3 {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123", slug = "foo")]
//~^ ERROR `#[error(slug = ...)]` is not a valid attribute
struct InvalidNestedStructAttr4 {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct WrongPlaceField {
    #[suggestion = "bar"]
    //~^ ERROR `#[suggestion = ...]` is not a valid attribute
    sp: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[error(typeck::ambiguous_lifetime_bound, code = "E0456")]
//~^ ERROR specified multiple times
//~^^ ERROR specified multiple times
//~^^^ ERROR specified multiple times
struct ErrorSpecifiedTwice {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[warning(typeck::ambiguous_lifetime_bound, code = "E0293")]
//~^ ERROR specified multiple times
//~^^ ERROR specified multiple times
//~^^^ ERROR specified multiple times
struct WarnSpecifiedAfterError {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0456", code = "E0457")]
//~^ ERROR specified multiple times
struct CodeSpecifiedTwice {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, typeck::ambiguous_lifetime_bound, code = "E0456")]
//~^ ERROR `#[error(typeck::ambiguous_lifetime_bound)]` is not a valid attribute
struct SlugSpecifiedTwice {}

#[derive(SessionDiagnostic)]
struct KindNotProvided {} //~ ERROR diagnostic kind not specified

#[derive(SessionDiagnostic)]
#[error(code = "E0456")]
//~^ ERROR diagnostic slug not specified
struct SlugNotProvided {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound)]
struct CodeNotProvided {}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct MessageWrongType {
    #[primary_span]
    //~^ ERROR `#[primary_span]` attribute can only be applied to fields of type `Span` or `MultiSpan`
    foo: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct InvalidPathFieldAttr {
    #[nonsense]
    //~^ ERROR `#[nonsense]` is not a valid attribute
    //~^^ ERROR cannot find attribute `nonsense` in this scope
    foo: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithField {
    name: String,
    #[label(typeck::label)]
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithMessageAppliedToField {
    #[label(typeck::label)]
    //~^ ERROR the `#[label(...)]` attribute can only be applied to fields of type `Span` or `MultiSpan`
    name: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithNonexistentField {
    #[suggestion(typeck::suggestion, code = "{name}")]
    //~^ ERROR `name` doesn't refer to a field on this type
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
//~^ ERROR invalid format string: expected `'}'`
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorMissingClosingBrace {
    #[suggestion(typeck::suggestion, code = "{name")]
    suggestion: (Span, Applicability),
    name: String,
    val: usize,
}

#[derive(SessionDiagnostic)]
//~^ ERROR invalid format string: unmatched `}`
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorMissingOpeningBrace {
    #[suggestion(typeck::suggestion, code = "name}")]
    suggestion: (Span, Applicability),
    name: String,
    val: usize,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct LabelOnSpan {
    #[label(typeck::label)]
    sp: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct LabelOnNonSpan {
    #[label(typeck::label)]
    //~^ ERROR the `#[label(...)]` attribute can only be applied to fields of type `Span` or `MultiSpan`
    id: u32,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct Suggest {
    #[suggestion(typeck::suggestion, code = "This is the suggested code")]
    #[suggestion_short(typeck::suggestion, code = "This is the suggested code")]
    #[suggestion_hidden(typeck::suggestion, code = "This is the suggested code")]
    #[suggestion_verbose(typeck::suggestion, code = "This is the suggested code")]
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithoutCode {
    #[suggestion(typeck::suggestion)]
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithBadKey {
    #[suggestion(nonsense = "bar")]
    //~^ ERROR `#[suggestion(nonsense = ...)]` is not a valid attribute
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithShorthandMsg {
    #[suggestion(msg = "bar")]
    //~^ ERROR `#[suggestion(msg = ...)]` is not a valid attribute
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithoutMsg {
    #[suggestion(code = "bar")]
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithTypesSwapped {
    #[suggestion(typeck::suggestion, code = "This is suggested code")]
    suggestion: (Applicability, Span),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithWrongTypeApplicabilityOnly {
    #[suggestion(typeck::suggestion, code = "This is suggested code")]
    //~^ ERROR wrong field type for suggestion
    suggestion: Applicability,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithSpanOnly {
    #[suggestion(typeck::suggestion, code = "This is suggested code")]
    suggestion: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithDuplicateSpanAndApplicability {
    #[suggestion(typeck::suggestion, code = "This is suggested code")]
    //~^ ERROR type of field annotated with `#[suggestion(...)]` contains more than one `Span`
    suggestion: (Span, Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct SuggestWithDuplicateApplicabilityAndSpan {
    #[suggestion(typeck::suggestion, code = "This is suggested code")]
    //~^ ERROR type of field annotated with `#[suggestion(...)]` contains more than one
    suggestion: (Applicability, Applicability, Span),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct WrongKindOfAnnotation {
    #[label = "bar"]
    //~^ ERROR `#[label = ...]` is not a valid attribute
    z: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct OptionsInErrors {
    #[label(typeck::label)]
    label: Option<Span>,
    #[suggestion(typeck::suggestion)]
    opt_sugg: Option<(Span, Applicability)>,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0456")]
struct MoveOutOfBorrowError<'tcx> {
    name: Ident,
    ty: Ty<'tcx>,
    #[primary_span]
    #[label(typeck::label)]
    span: Span,
    #[label(typeck::label)]
    other_span: Span,
    #[suggestion(typeck::suggestion, code = "{name}.clone()")]
    opt_sugg: Option<(Span, Applicability)>,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithLifetime<'a> {
    #[label(typeck::label)]
    span: Span,
    name: &'a str,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithDefaultLabelAttr<'a> {
    #[label]
    span: Span,
    name: &'a str,
}

#[derive(SessionDiagnostic)]
//~^ ERROR the trait bound `Hello: IntoDiagnosticArg` is not satisfied
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ArgFieldWithoutSkip {
    #[primary_span]
    span: Span,
    other: Hello,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ArgFieldWithSkip {
    #[primary_span]
    span: Span,
    // `Hello` does not implement `IntoDiagnosticArg` so this would result in an error if
    // not for `#[skip_arg]`.
    #[skip_arg]
    other: Hello,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithSpannedNote {
    #[note]
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithSpannedNoteCustom {
    #[note(typeck::note)]
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[note]
struct ErrorWithNote {
    val: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[note(typeck::note)]
struct ErrorWithNoteCustom {
    val: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithSpannedHelp {
    #[help]
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithSpannedHelpCustom {
    #[help(typeck::help)]
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[help]
struct ErrorWithHelp {
    val: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[help(typeck::help)]
struct ErrorWithHelpCustom {
    val: String,
}

#[derive(SessionDiagnostic)]
#[help]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithHelpWrongOrder {
    val: String,
}

#[derive(SessionDiagnostic)]
#[help(typeck::help)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithHelpCustomWrongOrder {
    val: String,
}

#[derive(SessionDiagnostic)]
#[note]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithNoteWrongOrder {
    val: String,
}

#[derive(SessionDiagnostic)]
#[note(typeck::note)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithNoteCustomWrongOrder {
    val: String,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ApplicabilityInBoth {
    #[suggestion(typeck::suggestion, code = "...", applicability = "maybe-incorrect")]
    //~^ ERROR applicability cannot be set in both the field and attribute
    suggestion: (Span, Applicability),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct InvalidApplicability {
    #[suggestion(typeck::suggestion, code = "...", applicability = "batman")]
    //~^ ERROR invalid applicability
    suggestion: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ValidApplicability {
    #[suggestion(typeck::suggestion, code = "...", applicability = "maybe-incorrect")]
    suggestion: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct NoApplicability {
    #[suggestion(typeck::suggestion, code = "...")]
    suggestion: Span,
}

#[derive(SessionSubdiagnostic)]
#[note(parser::add_paren)]
struct Note;

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound)]
struct Subdiagnostic {
    #[subdiagnostic]
    note: Note,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct VecField {
    #[primary_span]
    #[label]
    spans: Vec<Span>,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct UnitField {
    #[primary_span]
    spans: Span,
    #[help]
    foo: (),
    #[help(typeck::help)]
    bar: (),
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct OptUnitField {
    #[primary_span]
    spans: Span,
    #[help]
    foo: Option<()>,
    #[help(typeck::help)]
    bar: Option<()>,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct LabelWithTrailingPath {
    #[label(typeck::label, foo)]
    //~^ ERROR `#[label(...)]` is not a valid attribute
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct LabelWithTrailingNameValue {
    #[label(typeck::label, foo = "...")]
    //~^ ERROR `#[label(...)]` is not a valid attribute
    span: Span,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct LabelWithTrailingList {
    #[label(typeck::label, foo("..."))]
    //~^ ERROR `#[label(...)]` is not a valid attribute
    span: Span,
}

#[derive(SessionDiagnostic)]
#[lint(typeck::ambiguous_lifetime_bound)]
//~^ ERROR only `#[error(..)]` and `#[warning(..)]` are supported
struct LintsBad {
}

#[derive(LintDiagnostic)]
#[lint(typeck::ambiguous_lifetime_bound)]
struct LintsGood {
}

#[derive(LintDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound)]
//~^ ERROR only `#[lint(..)]` is supported
struct ErrorsBad {
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
struct ErrorWithMultiSpan {
    #[primary_span]
    span: MultiSpan,
}

#[derive(SessionDiagnostic)]
#[error(typeck::ambiguous_lifetime_bound, code = "E0123")]
#[warn_]
struct ErrorWithWarn {
    val: String,
}
