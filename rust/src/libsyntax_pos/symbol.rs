//! An "interner" is a data structure that associates values with usize tags and
//! allows bidirectional lookup; i.e., given a value, one can easily find the
//! type, and vice versa.

use arena::DroplessArena;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::indexed_vec::Idx;
use rustc_data_structures::newtype_index;
use rustc_macros::symbols;
use serialize::{Decodable, Decoder, Encodable, Encoder};

use std::fmt;
use std::str;
use std::cmp::{PartialEq, Ordering, PartialOrd, Ord};
use std::hash::{Hash, Hasher};

use crate::hygiene::SyntaxContext;
use crate::{Span, DUMMY_SP, GLOBALS};

symbols! {
    // After modifying this list adjust `is_special`, `is_used_keyword`/`is_unused_keyword`,
    // this should be rarely necessary though if the keywords are kept in alphabetic order.
    Keywords {
        // Special reserved identifiers used internally for elided lifetimes,
        // unnamed method parameters, crate root module, error recovery etc.
        Invalid:            "",
        PathRoot:           "{{root}}",
        DollarCrate:        "$crate",
        Underscore:         "_",

        // Keywords that are used in stable Rust.
        As:                 "as",
        Box:                "box",
        Break:              "break",
        Const:              "const",
        Continue:           "continue",
        Crate:              "crate",
        Else:               "else",
        Enum:               "enum",
        Extern:             "extern",
        False:              "false",
        Fn:                 "fn",
        For:                "for",
        If:                 "if",
        Impl:               "impl",
        In:                 "in",
        Let:                "let",
        Loop:               "loop",
        Match:              "match",
        Mod:                "mod",
        Move:               "move",
        Mut:                "mut",
        Pub:                "pub",
        Ref:                "ref",
        Return:             "return",
        SelfLower:          "self",
        SelfUpper:          "Self",
        Static:             "static",
        Struct:             "struct",
        Super:              "super",
        Trait:              "trait",
        True:               "true",
        Type:               "type",
        Unsafe:             "unsafe",
        Use:                "use",
        Where:              "where",
        While:              "while",

        // Keywords that are used in unstable Rust or reserved for future use.
        Abstract:           "abstract",
        Become:             "become",
        Do:                 "do",
        Final:              "final",
        Macro:              "macro",
        Override:           "override",
        Priv:               "priv",
        Typeof:             "typeof",
        Unsized:            "unsized",
        Virtual:            "virtual",
        Yield:              "yield",

        // Edition-specific keywords that are used in stable Rust.
        Dyn:                "dyn", // >= 2018 Edition only

        // Edition-specific keywords that are used in unstable Rust or reserved for future use.
        Async:              "async", // >= 2018 Edition only
        Try:                "try", // >= 2018 Edition only

        // Special lifetime names
        UnderscoreLifetime: "'_",
        StaticLifetime:     "'static",

        // Weak keywords, have special meaning only in specific contexts.
        Auto:               "auto",
        Catch:              "catch",
        Default:            "default",
        Existential:        "existential",
        Union:              "union",
    }

    // Other symbols that can be referred to with syntax_pos::symbols::*
    Other {
        alias,
        align,
        alloc_error_handler,
        allow,
        allow_fail,
        allow_internal_unsafe,
        allow_internal_unstable,
        automatically_derived,
        cfg,
        cfg_attr,
        cold,
        compiler_builtins,
        crate_id,
        crate_name,
        crate_type,
        default_lib_allocator,
        deny,
        deprecated,
        derive,
        doc,
        export_name,
        feature,
        ffi_returns_twice,
        forbid,
        fundamental,
        global_allocator,
        ignore,
        include,
        inline,
        keyword,
        lang,
        link,
        link_args,
        link_name,
        link_section,
        linkage,
        macro_escape,
        macro_export,
        macro_use,
        main,
        marker,
        masked,
        may_dangle,
        must_use,
        naked,
        needs_allocator,
        needs_panic_runtime,
        no_builtins,
        no_core,
        no_debug,
        no_implicit_prelude,
        no_link,
        no_main,
        no_mangle,
        no_start,
        no_std,
        non_exhaustive,
        omit_gdb_pretty_printer_section,
        optimize,
        panic_handler,
        panic_runtime,
        path,
        plugin,
        plugin_registrar,
        prelude_import,
        proc_macro,
        proc_macro_attribute,
        proc_macro_derive,
        profiler_runtime,
        recursion_limit,
        reexport_test_harness_main,
        repr,
        rustc_args_required_const,
        rustc_clean,
        rustc_const_unstable,
        rustc_conversion_suggestion,
        rustc_copy_clone_marker,
        rustc_def_path,
        rustc_deprecated,
        rustc_dirty,
        rustc_dump_program_clauses,
        rustc_dump_user_substs,
        rustc_error,
        rustc_expected_cgu_reuse,
        rustc_if_this_changed,
        rustc_inherit_overflow_checks,
        rustc_layout,
        rustc_layout_scalar_valid_range_end,
        rustc_layout_scalar_valid_range_start,
        rustc_mir,
        rustc_on_unimplemented,
        rustc_outlives,
        rustc_paren_sugar,
        rustc_partition_codegened,
        rustc_partition_reused,
        rustc_proc_macro_decls,
        rustc_regions,
        rustc_std_internal_symbol,
        rustc_symbol_name,
        rustc_synthetic,
        rustc_test_marker,
        rustc_then_this_would_need,
        rustc_transparent_macro,
        rustc_variance,
        sanitizer_runtime,
        should_panic,
        simd,
        spotlight,
        stable,
        start,
        structural_match,
        target_feature,
        test_runner,
        thread_local,
        type_length_limit,
        unsafe_destructor_blind_to_params,
        unstable,
        unwind,
        used,
        warn,
        windows_subsystem,
    }
}

#[derive(Copy, Clone, Eq)]
pub struct Ident {
    pub name: Symbol,
    pub span: Span,
}

impl Ident {
    #[inline]
    pub const fn new(name: Symbol, span: Span) -> Ident {
        Ident { name, span }
    }

    #[inline]
    pub const fn with_empty_ctxt(name: Symbol) -> Ident {
        Ident::new(name, DUMMY_SP)
    }

    /// Maps an interned string to an identifier with an empty syntax context.
    pub fn from_interned_str(string: InternedString) -> Ident {
        Ident::with_empty_ctxt(string.as_symbol())
    }

    /// Maps a string to an identifier with an empty syntax context.
    pub fn from_str(string: &str) -> Ident {
        Ident::with_empty_ctxt(Symbol::intern(string))
    }

    /// Replaces `lo` and `hi` with those from `span`, but keep hygiene context.
    pub fn with_span_pos(self, span: Span) -> Ident {
        Ident::new(self.name, span.with_ctxt(self.span.ctxt()))
    }

    pub fn without_first_quote(self) -> Ident {
        Ident::new(Symbol::intern(self.as_str().trim_start_matches('\'')), self.span)
    }

    /// "Normalize" ident for use in comparisons using "item hygiene".
    /// Identifiers with same string value become same if they came from the same "modern" macro
    /// (e.g., `macro` item, but not `macro_rules` item) and stay different if they came from
    /// different "modern" macros.
    /// Technically, this operation strips all non-opaque marks from ident's syntactic context.
    pub fn modern(self) -> Ident {
        Ident::new(self.name, self.span.modern())
    }

    /// "Normalize" ident for use in comparisons using "local variable hygiene".
    /// Identifiers with same string value become same if they came from the same non-transparent
    /// macro (e.g., `macro` or `macro_rules!` items) and stay different if they came from different
    /// non-transparent macros.
    /// Technically, this operation strips all transparent marks from ident's syntactic context.
    pub fn modern_and_legacy(self) -> Ident {
        Ident::new(self.name, self.span.modern_and_legacy())
    }

    pub fn gensym(self) -> Ident {
        Ident::new(self.name.gensymed(), self.span)
    }

    pub fn gensym_if_underscore(self) -> Ident {
        if self.name == keywords::Underscore.name() { self.gensym() } else { self }
    }

    pub fn as_str(self) -> LocalInternedString {
        self.name.as_str()
    }

    pub fn as_interned_str(self) -> InternedString {
        self.name.as_interned_str()
    }
}

impl PartialEq for Ident {
    fn eq(&self, rhs: &Self) -> bool {
        self.name == rhs.name && self.span.ctxt() == rhs.span.ctxt()
    }
}

impl Hash for Ident {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.span.ctxt().hash(state);
    }
}

impl fmt::Debug for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:?}", self.name, self.span.ctxt())
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.name, f)
    }
}

impl Encodable for Ident {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        if self.span.ctxt().modern() == SyntaxContext::empty() {
            s.emit_str(&self.as_str())
        } else { // FIXME(jseyfried): intercrate hygiene
            let mut string = "#".to_owned();
            string.push_str(&self.as_str());
            s.emit_str(&string)
        }
    }
}

impl Decodable for Ident {
    fn decode<D: Decoder>(d: &mut D) -> Result<Ident, D::Error> {
        let string = d.read_str()?;
        Ok(if !string.starts_with('#') {
            Ident::from_str(&string)
        } else { // FIXME(jseyfried): intercrate hygiene
            Ident::with_empty_ctxt(Symbol::gensym(&string[1..]))
        })
    }
}

/// A symbol is an interned or gensymed string. The use of `newtype_index!` means
/// that `Option<Symbol>` only takes up 4 bytes, because `newtype_index!` reserves
/// the last 256 values for tagging purposes.
///
/// Note that `Symbol` cannot directly be a `newtype_index!` because it implements
/// `fmt::Debug`, `Encodable`, and `Decodable` in special ways.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(SymbolIndex);

newtype_index! {
    pub struct SymbolIndex { .. }
}

impl Symbol {
    const fn new(n: u32) -> Self {
        Symbol(SymbolIndex::from_u32_const(n))
    }

    /// Maps a string to its interned representation.
    pub fn intern(string: &str) -> Self {
        with_interner(|interner| interner.intern(string))
    }

    pub fn interned(self) -> Self {
        with_interner(|interner| interner.interned(self))
    }

    /// Gensyms a new `usize`, using the current interner.
    pub fn gensym(string: &str) -> Self {
        with_interner(|interner| interner.gensym(string))
    }

    pub fn gensymed(self) -> Self {
        with_interner(|interner| interner.gensymed(self))
    }

    pub fn is_gensymed(self) -> bool {
        with_interner(|interner| interner.is_gensymed(self))
    }

    pub fn as_str(self) -> LocalInternedString {
        with_interner(|interner| unsafe {
            LocalInternedString {
                string: std::mem::transmute::<&str, &str>(interner.get(self))
            }
        })
    }

    pub fn as_interned_str(self) -> InternedString {
        with_interner(|interner| InternedString {
            symbol: interner.interned(self)
        })
    }

    pub fn as_u32(self) -> u32 {
        self.0.as_u32()
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let is_gensymed = with_interner(|interner| interner.is_gensymed(*self));
        if is_gensymed {
            write!(f, "{}({:?})", self, self.0)
        } else {
            write!(f, "{}", self)
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.as_str(), f)
    }
}

impl Encodable for Symbol {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        s.emit_str(&self.as_str())
    }
}

impl Decodable for Symbol {
    fn decode<D: Decoder>(d: &mut D) -> Result<Symbol, D::Error> {
        Ok(Symbol::intern(&d.read_str()?))
    }
}

impl<T: std::ops::Deref<Target=str>> PartialEq<T> for Symbol {
    fn eq(&self, other: &T) -> bool {
        self.as_str() == other.deref()
    }
}

// The `&'static str`s in this type actually point into the arena.
//
// Note that normal symbols are indexed upward from 0, and gensyms are indexed
// downward from SymbolIndex::MAX_AS_U32.
#[derive(Default)]
pub struct Interner {
    arena: DroplessArena,
    names: FxHashMap<&'static str, Symbol>,
    strings: Vec<&'static str>,
    gensyms: Vec<Symbol>,
}

impl Interner {
    fn prefill(init: &[&str]) -> Self {
        let mut this = Interner::default();
        for &string in init {
            if string == "" {
                // We can't allocate empty strings in the arena, so handle this here.
                let name = Symbol::new(this.strings.len() as u32);
                this.names.insert("", name);
                this.strings.push("");
            } else {
                this.intern(string);
            }
        }
        this
    }

    pub fn intern(&mut self, string: &str) -> Symbol {
        if let Some(&name) = self.names.get(string) {
            return name;
        }

        let name = Symbol::new(self.strings.len() as u32);

        // `from_utf8_unchecked` is safe since we just allocated a `&str` which is known to be
        // UTF-8.
        let string: &str = unsafe {
            str::from_utf8_unchecked(self.arena.alloc_slice(string.as_bytes()))
        };
        // It is safe to extend the arena allocation to `'static` because we only access
        // these while the arena is still alive.
        let string: &'static str =  unsafe {
            &*(string as *const str)
        };
        self.strings.push(string);
        self.names.insert(string, name);
        name
    }

    pub fn interned(&self, symbol: Symbol) -> Symbol {
        if (symbol.0.as_usize()) < self.strings.len() {
            symbol
        } else {
            self.interned(self.gensyms[(SymbolIndex::MAX_AS_U32 - symbol.0.as_u32()) as usize])
        }
    }

    fn gensym(&mut self, string: &str) -> Symbol {
        let symbol = self.intern(string);
        self.gensymed(symbol)
    }

    fn gensymed(&mut self, symbol: Symbol) -> Symbol {
        self.gensyms.push(symbol);
        Symbol::new(SymbolIndex::MAX_AS_U32 - self.gensyms.len() as u32 + 1)
    }

    fn is_gensymed(&mut self, symbol: Symbol) -> bool {
        symbol.0.as_usize() >= self.strings.len()
    }

    pub fn get(&self, symbol: Symbol) -> &str {
        match self.strings.get(symbol.0.as_usize()) {
            Some(string) => string,
            None => self.get(self.gensyms[(SymbolIndex::MAX_AS_U32 - symbol.0.as_u32()) as usize]),
        }
    }
}

pub mod keywords {
    use super::{Symbol, Ident};

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Keyword {
        ident: Ident,
    }

    impl Keyword {
        #[inline]
        pub fn ident(self) -> Ident {
            self.ident
        }

        #[inline]
        pub fn name(self) -> Symbol {
            self.ident.name
        }
    }

    keywords!();
}

pub mod symbols {
    use super::Symbol;
    symbols!();
}

impl Symbol {
    fn is_used_keyword_2018(self) -> bool {
        self == keywords::Dyn.name()
    }

    fn is_unused_keyword_2018(self) -> bool {
        self >= keywords::Async.name() && self <= keywords::Try.name()
    }
}

impl Ident {
    // Returns `true` for reserved identifiers used internally for elided lifetimes,
    // unnamed method parameters, crate root module, error recovery etc.
    pub fn is_special(self) -> bool {
        self.name <= keywords::Underscore.name()
    }

    /// Returns `true` if the token is a keyword used in the language.
    pub fn is_used_keyword(self) -> bool {
        // Note: `span.edition()` is relatively expensive, don't call it unless necessary.
        self.name >= keywords::As.name() && self.name <= keywords::While.name() ||
        self.name.is_used_keyword_2018() && self.span.rust_2018()
    }

    /// Returns `true` if the token is a keyword reserved for possible future use.
    pub fn is_unused_keyword(self) -> bool {
        // Note: `span.edition()` is relatively expensive, don't call it unless necessary.
        self.name >= keywords::Abstract.name() && self.name <= keywords::Yield.name() ||
        self.name.is_unused_keyword_2018() && self.span.rust_2018()
    }

    /// Returns `true` if the token is either a special identifier or a keyword.
    pub fn is_reserved(self) -> bool {
        self.is_special() || self.is_used_keyword() || self.is_unused_keyword()
    }

    /// A keyword or reserved identifier that can be used as a path segment.
    pub fn is_path_segment_keyword(self) -> bool {
        self.name == keywords::Super.name() ||
        self.name == keywords::SelfLower.name() ||
        self.name == keywords::SelfUpper.name() ||
        self.name == keywords::Crate.name() ||
        self.name == keywords::PathRoot.name() ||
        self.name == keywords::DollarCrate.name()
    }

    /// This identifier can be a raw identifier.
    pub fn can_be_raw(self) -> bool {
        self.name != keywords::Invalid.name() && self.name != keywords::Underscore.name() &&
        !self.is_path_segment_keyword()
    }

    /// We see this identifier in a normal identifier position, like variable name or a type.
    /// How was it written originally? Did it use the raw form? Let's try to guess.
    pub fn is_raw_guess(self) -> bool {
        self.can_be_raw() && self.is_reserved()
    }
}

// If an interner exists, return it. Otherwise, prepare a fresh one.
#[inline]
fn with_interner<T, F: FnOnce(&mut Interner) -> T>(f: F) -> T {
    GLOBALS.with(|globals| f(&mut *globals.symbol_interner.lock()))
}

/// Represents a string stored in the interner. Because the interner outlives any thread
/// which uses this type, we can safely treat `string` which points to interner data,
/// as an immortal string, as long as this type never crosses between threads.
// FIXME: ensure that the interner outlives any thread which uses `LocalInternedString`,
// by creating a new thread right after constructing the interner.
#[derive(Clone, Copy, Hash, PartialOrd, Eq, Ord)]
pub struct LocalInternedString {
    string: &'static str,
}

impl LocalInternedString {
    pub fn as_interned_str(self) -> InternedString {
        InternedString {
            symbol: Symbol::intern(self.string)
        }
    }

    pub fn get(&self) -> &str {
        // This returns a valid string since we ensure that `self` outlives the interner
        // by creating the interner on a thread which outlives threads which can access it.
        // This type cannot move to a thread which outlives the interner since it does
        // not implement Send.
        self.string
    }
}

impl<U: ?Sized> std::convert::AsRef<U> for LocalInternedString
where
    str: std::convert::AsRef<U>
{
    fn as_ref(&self) -> &U {
        self.string.as_ref()
    }
}

impl<T: std::ops::Deref<Target = str>> std::cmp::PartialEq<T> for LocalInternedString {
    fn eq(&self, other: &T) -> bool {
        self.string == other.deref()
    }
}

impl std::cmp::PartialEq<LocalInternedString> for str {
    fn eq(&self, other: &LocalInternedString) -> bool {
        self == other.string
    }
}

impl<'a> std::cmp::PartialEq<LocalInternedString> for &'a str {
    fn eq(&self, other: &LocalInternedString) -> bool {
        *self == other.string
    }
}

impl std::cmp::PartialEq<LocalInternedString> for String {
    fn eq(&self, other: &LocalInternedString) -> bool {
        self == other.string
    }
}

impl<'a> std::cmp::PartialEq<LocalInternedString> for &'a String {
    fn eq(&self, other: &LocalInternedString) -> bool {
        *self == other.string
    }
}

impl !Send for LocalInternedString {}
impl !Sync for LocalInternedString {}

impl std::ops::Deref for LocalInternedString {
    type Target = str;
    fn deref(&self) -> &str { self.string }
}

impl fmt::Debug for LocalInternedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.string, f)
    }
}

impl fmt::Display for LocalInternedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.string, f)
    }
}

impl Decodable for LocalInternedString {
    fn decode<D: Decoder>(d: &mut D) -> Result<LocalInternedString, D::Error> {
        Ok(Symbol::intern(&d.read_str()?).as_str())
    }
}

impl Encodable for LocalInternedString {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        s.emit_str(self.string)
    }
}

/// Represents a string stored in the string interner.
#[derive(Clone, Copy, Eq)]
pub struct InternedString {
    symbol: Symbol,
}

impl InternedString {
    pub fn with<F: FnOnce(&str) -> R, R>(self, f: F) -> R {
        let str = with_interner(|interner| {
            interner.get(self.symbol) as *const str
        });
        // This is safe because the interner keeps string alive until it is dropped.
        // We can access it because we know the interner is still alive since we use a
        // scoped thread local to access it, and it was alive at the beginning of this scope
        unsafe { f(&*str) }
    }

    pub fn as_symbol(self) -> Symbol {
        self.symbol
    }

    pub fn as_str(self) -> LocalInternedString {
        self.symbol.as_str()
    }
}

impl Hash for InternedString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.with(|str| str.hash(state))
    }
}

impl PartialOrd<InternedString> for InternedString {
    fn partial_cmp(&self, other: &InternedString) -> Option<Ordering> {
        if self.symbol == other.symbol {
            return Some(Ordering::Equal);
        }
        self.with(|self_str| other.with(|other_str| self_str.partial_cmp(other_str)))
    }
}

impl Ord for InternedString {
    fn cmp(&self, other: &InternedString) -> Ordering {
        if self.symbol == other.symbol {
            return Ordering::Equal;
        }
        self.with(|self_str| other.with(|other_str| self_str.cmp(&other_str)))
    }
}

impl<T: std::ops::Deref<Target = str>> PartialEq<T> for InternedString {
    fn eq(&self, other: &T) -> bool {
        self.with(|string| string == other.deref())
    }
}

impl PartialEq<InternedString> for InternedString {
    fn eq(&self, other: &InternedString) -> bool {
        self.symbol == other.symbol
    }
}

impl PartialEq<InternedString> for str {
    fn eq(&self, other: &InternedString) -> bool {
        other.with(|string| self == string)
    }
}

impl<'a> PartialEq<InternedString> for &'a str {
    fn eq(&self, other: &InternedString) -> bool {
        other.with(|string| *self == string)
    }
}

impl PartialEq<InternedString> for String {
    fn eq(&self, other: &InternedString) -> bool {
        other.with(|string| self == string)
    }
}

impl<'a> PartialEq<InternedString> for &'a String {
    fn eq(&self, other: &InternedString) -> bool {
        other.with(|string| *self == string)
    }
}

impl std::convert::From<InternedString> for String {
    fn from(val: InternedString) -> String {
        val.as_symbol().to_string()
    }
}

impl fmt::Debug for InternedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.with(|str| fmt::Debug::fmt(&str, f))
    }
}

impl fmt::Display for InternedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.with(|str| fmt::Display::fmt(&str, f))
    }
}

impl Decodable for InternedString {
    fn decode<D: Decoder>(d: &mut D) -> Result<InternedString, D::Error> {
        Ok(Symbol::intern(&d.read_str()?).as_interned_str())
    }
}

impl Encodable for InternedString {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        self.with(|string| s.emit_str(string))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Globals;

    #[test]
    fn interner_tests() {
        let mut i: Interner = Interner::default();
        // first one is zero:
        assert_eq!(i.intern("dog"), Symbol::new(0));
        // re-use gets the same entry:
        assert_eq!(i.intern("dog"), Symbol::new(0));
        // different string gets a different #:
        assert_eq!(i.intern("cat"), Symbol::new(1));
        assert_eq!(i.intern("cat"), Symbol::new(1));
        // dog is still at zero
        assert_eq!(i.intern("dog"), Symbol::new(0));
        assert_eq!(i.gensym("zebra"), Symbol::new(SymbolIndex::MAX_AS_U32));
        // gensym of same string gets new number:
        assert_eq!(i.gensym("zebra"), Symbol::new(SymbolIndex::MAX_AS_U32 - 1));
        // gensym of *existing* string gets new number:
        assert_eq!(i.gensym("dog"), Symbol::new(SymbolIndex::MAX_AS_U32 - 2));
    }

    #[test]
    fn without_first_quote_test() {
        GLOBALS.set(&Globals::new(), || {
            let i = Ident::from_str("'break");
            assert_eq!(i.without_first_quote().name, keywords::Break.name());
        });
    }
}
