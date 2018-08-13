// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use {AmbiguityError, CrateLint, Resolver, ResolutionError, is_known_tool, resolve_error};
use {Module, ModuleKind, NameBinding, NameBindingKind, PathResult, ToNameBinding};
use Namespace::{self, TypeNS, MacroNS};
use build_reduced_graph::{BuildReducedGraphVisitor, IsMacroExport};
use resolve_imports::ImportResolver;
use rustc::hir::def_id::{DefId, BUILTIN_MACROS_CRATE, CRATE_DEF_INDEX, DefIndex,
                         DefIndexAddressSpace};
use rustc::hir::def::{Def, Export, NonMacroAttrKind};
use rustc::hir::map::{self, DefCollector};
use rustc::{ty, lint};
use rustc::middle::cstore::CrateStore;
use syntax::ast::{self, Name, Ident};
use syntax::attr;
use syntax::errors::DiagnosticBuilder;
use syntax::ext::base::{self, Determinacy, MultiModifier, MultiDecorator};
use syntax::ext::base::{MacroKind, SyntaxExtension, Resolver as SyntaxResolver};
use syntax::ext::expand::{AstFragment, Invocation, InvocationKind};
use syntax::ext::hygiene::{self, Mark};
use syntax::ext::tt::macro_rules;
use syntax::feature_gate::{self, feature_err, emit_feature_err, is_builtin_attr_name, GateIssue};
use syntax::feature_gate::EXPLAIN_DERIVE_UNDERSCORE;
use syntax::fold::{self, Folder};
use syntax::parse::parser::PathStyle;
use syntax::parse::token::{self, Token};
use syntax::ptr::P;
use syntax::symbol::{Symbol, keywords};
use syntax::tokenstream::{TokenStream, TokenTree, Delimited};
use syntax::util::lev_distance::find_best_match_for_name;
use syntax_pos::{Span, DUMMY_SP};

use std::cell::Cell;
use std::mem;
use rustc_data_structures::sync::Lrc;

#[derive(Clone)]
pub struct InvocationData<'a> {
    pub module: Cell<Module<'a>>,
    pub def_index: DefIndex,
    // The scope in which the invocation path is resolved.
    pub legacy_scope: Cell<LegacyScope<'a>>,
    // The smallest scope that includes this invocation's expansion,
    // or `Empty` if this invocation has not been expanded yet.
    pub expansion: Cell<LegacyScope<'a>>,
}

impl<'a> InvocationData<'a> {
    pub fn root(graph_root: Module<'a>) -> Self {
        InvocationData {
            module: Cell::new(graph_root),
            def_index: CRATE_DEF_INDEX,
            legacy_scope: Cell::new(LegacyScope::Empty),
            expansion: Cell::new(LegacyScope::Empty),
        }
    }
}

#[derive(Copy, Clone)]
pub enum LegacyScope<'a> {
    Empty,
    Invocation(&'a InvocationData<'a>), // The scope of the invocation, not including its expansion
    Expansion(&'a InvocationData<'a>), // The scope of the invocation, including its expansion
    Binding(&'a LegacyBinding<'a>),
}

pub struct LegacyBinding<'a> {
    pub parent: Cell<LegacyScope<'a>>,
    pub ident: Ident,
    def_id: DefId,
    pub span: Span,
}

pub struct ProcMacError {
    crate_name: Symbol,
    name: Symbol,
    module: ast::NodeId,
    use_span: Span,
    warn_msg: &'static str,
}

#[derive(Copy, Clone)]
pub enum MacroBinding<'a> {
    Legacy(&'a LegacyBinding<'a>),
    Global(&'a NameBinding<'a>),
    Modern(&'a NameBinding<'a>),
}

impl<'a> MacroBinding<'a> {
    pub fn span(self) -> Span {
        match self {
            MacroBinding::Legacy(binding) => binding.span,
            MacroBinding::Global(binding) | MacroBinding::Modern(binding) => binding.span,
        }
    }

    pub fn binding(self) -> &'a NameBinding<'a> {
        match self {
            MacroBinding::Global(binding) | MacroBinding::Modern(binding) => binding,
            MacroBinding::Legacy(_) => panic!("unexpected MacroBinding::Legacy"),
        }
    }

    pub fn def_ignoring_ambiguity(self) -> Def {
        match self {
            MacroBinding::Legacy(binding) => Def::Macro(binding.def_id, MacroKind::Bang),
            MacroBinding::Global(binding) | MacroBinding::Modern(binding) =>
                binding.def_ignoring_ambiguity(),
        }
    }
}

impl<'a, 'crateloader: 'a> base::Resolver for Resolver<'a, 'crateloader> {
    fn next_node_id(&mut self) -> ast::NodeId {
        self.session.next_node_id()
    }

    fn get_module_scope(&mut self, id: ast::NodeId) -> Mark {
        let mark = Mark::fresh(Mark::root());
        let module = self.module_map[&self.definitions.local_def_id(id)];
        self.invocations.insert(mark, self.arenas.alloc_invocation_data(InvocationData {
            module: Cell::new(module),
            def_index: module.def_id().unwrap().index,
            legacy_scope: Cell::new(LegacyScope::Empty),
            expansion: Cell::new(LegacyScope::Empty),
        }));
        mark
    }

    fn eliminate_crate_var(&mut self, item: P<ast::Item>) -> P<ast::Item> {
        struct EliminateCrateVar<'b, 'a: 'b, 'crateloader: 'a>(
            &'b mut Resolver<'a, 'crateloader>, Span
        );

        impl<'a, 'b, 'crateloader> Folder for EliminateCrateVar<'a, 'b, 'crateloader> {
            fn fold_path(&mut self, path: ast::Path) -> ast::Path {
                match self.fold_qpath(None, path) {
                    (None, path) => path,
                    _ => unreachable!(),
                }
            }

            fn fold_qpath(&mut self, mut qself: Option<ast::QSelf>, mut path: ast::Path)
                          -> (Option<ast::QSelf>, ast::Path) {
                qself = qself.map(|ast::QSelf { ty, path_span, position }| {
                    ast::QSelf {
                        ty: self.fold_ty(ty),
                        path_span: self.new_span(path_span),
                        position,
                    }
                });

                if path.segments[0].ident.name == keywords::DollarCrate.name() {
                    let module = self.0.resolve_crate_root(path.segments[0].ident);
                    path.segments[0].ident.name = keywords::CrateRoot.name();
                    if !module.is_local() {
                        let span = path.segments[0].ident.span;
                        path.segments.insert(1, match module.kind {
                            ModuleKind::Def(_, name) => ast::PathSegment::from_ident(
                                ast::Ident::with_empty_ctxt(name).with_span_pos(span)
                            ),
                            _ => unreachable!(),
                        });
                        if let Some(qself) = &mut qself {
                            qself.position += 1;
                        }
                    }
                }
                (qself, path)
            }

            fn fold_mac(&mut self, mac: ast::Mac) -> ast::Mac {
                fold::noop_fold_mac(mac, self)
            }
        }

        EliminateCrateVar(self, item.span).fold_item(item).expect_one("")
    }

    fn is_whitelisted_legacy_custom_derive(&self, name: Name) -> bool {
        self.whitelisted_legacy_custom_derives.contains(&name)
    }

    fn visit_ast_fragment_with_placeholders(&mut self, mark: Mark, fragment: &AstFragment,
                                            derives: &[Mark]) {
        let invocation = self.invocations[&mark];
        self.collect_def_ids(mark, invocation, fragment);

        self.current_module = invocation.module.get();
        self.current_module.unresolved_invocations.borrow_mut().remove(&mark);
        self.current_module.unresolved_invocations.borrow_mut().extend(derives);
        for &derive in derives {
            self.invocations.insert(derive, invocation);
        }
        let mut visitor = BuildReducedGraphVisitor {
            resolver: self,
            legacy_scope: LegacyScope::Invocation(invocation),
            expansion: mark,
        };
        fragment.visit_with(&mut visitor);
        invocation.expansion.set(visitor.legacy_scope);
    }

    fn add_builtin(&mut self, ident: ast::Ident, ext: Lrc<SyntaxExtension>) {
        let def_id = DefId {
            krate: BUILTIN_MACROS_CRATE,
            index: DefIndex::from_array_index(self.macro_map.len(),
                                              DefIndexAddressSpace::Low),
        };
        let kind = ext.kind();
        self.macro_map.insert(def_id, ext);
        let binding = self.arenas.alloc_name_binding(NameBinding {
            kind: NameBindingKind::Def(Def::Macro(def_id, kind), false),
            span: DUMMY_SP,
            vis: ty::Visibility::Invisible,
            expansion: Mark::root(),
        });
        self.macro_prelude.insert(ident.name, binding);
    }

    fn resolve_imports(&mut self) {
        ImportResolver { resolver: self }.resolve_imports()
    }

    // Resolves attribute and derive legacy macros from `#![plugin(..)]`.
    fn find_legacy_attr_invoc(&mut self, attrs: &mut Vec<ast::Attribute>, allow_derive: bool)
                              -> Option<ast::Attribute> {
        for i in 0..attrs.len() {
            let name = attrs[i].name();

            if self.session.plugin_attributes.borrow().iter()
                    .any(|&(ref attr_nm, _)| name == &**attr_nm) {
                attr::mark_known(&attrs[i]);
            }

            match self.macro_prelude.get(&name).cloned() {
                Some(binding) => match *binding.get_macro(self) {
                    MultiModifier(..) | MultiDecorator(..) | SyntaxExtension::AttrProcMacro(..) => {
                        return Some(attrs.remove(i))
                    }
                    _ => {}
                },
                None => {}
            }
        }

        if !allow_derive { return None }

        // Check for legacy derives
        for i in 0..attrs.len() {
            let name = attrs[i].name();

            if name == "derive" {
                let result = attrs[i].parse_list(&self.session.parse_sess, |parser| {
                    parser.parse_path_allowing_meta(PathStyle::Mod)
                });

                let mut traits = match result {
                    Ok(traits) => traits,
                    Err(mut e) => {
                        e.cancel();
                        continue
                    }
                };

                for j in 0..traits.len() {
                    if traits[j].segments.len() > 1 {
                        continue
                    }
                    let trait_name = traits[j].segments[0].ident.name;
                    let legacy_name = Symbol::intern(&format!("derive_{}", trait_name));
                    if !self.macro_prelude.contains_key(&legacy_name) {
                        continue
                    }
                    let span = traits.remove(j).span;
                    self.gate_legacy_custom_derive(legacy_name, span);
                    if traits.is_empty() {
                        attrs.remove(i);
                    } else {
                        let mut tokens = Vec::new();
                        for (j, path) in traits.iter().enumerate() {
                            if j > 0 {
                                tokens.push(TokenTree::Token(attrs[i].span, Token::Comma).into());
                            }
                            for (k, segment) in path.segments.iter().enumerate() {
                                if k > 0 {
                                    tokens.push(TokenTree::Token(path.span, Token::ModSep).into());
                                }
                                let tok = Token::from_ast_ident(segment.ident);
                                tokens.push(TokenTree::Token(path.span, tok).into());
                            }
                        }
                        attrs[i].tokens = TokenTree::Delimited(attrs[i].span, Delimited {
                            delim: token::Paren,
                            tts: TokenStream::concat(tokens).into(),
                        }).into();
                    }
                    return Some(ast::Attribute {
                        path: ast::Path::from_ident(Ident::new(legacy_name, span)),
                        tokens: TokenStream::empty(),
                        id: attr::mk_attr_id(),
                        style: ast::AttrStyle::Outer,
                        is_sugared_doc: false,
                        span,
                    });
                }
            }
        }

        None
    }

    fn resolve_invoc(&mut self, invoc: &Invocation, scope: Mark, force: bool)
                     -> Result<Option<Lrc<SyntaxExtension>>, Determinacy> {
        let def = match invoc.kind {
            InvocationKind::Attr { attr: None, .. } => return Ok(None),
            _ => self.resolve_invoc_to_def(invoc, scope, force)?,
        };
        if let Def::Macro(_, MacroKind::ProcMacroStub) = def {
            self.report_proc_macro_stub(invoc.span());
            return Err(Determinacy::Determined);
        } else if let Def::NonMacroAttr(attr_kind) = def {
            // Note that not only attributes, but anything in macro namespace can result in a
            // `Def::NonMacroAttr` definition (e.g. `inline!()`), so we must report the error
            // below for these cases.
            let is_attr_invoc =
                if let InvocationKind::Attr { .. } = invoc.kind { true } else { false };
            let path = invoc.path().expect("no path for non-macro attr");
            match attr_kind {
                NonMacroAttrKind::Tool | NonMacroAttrKind::DeriveHelper |
                NonMacroAttrKind::Custom if is_attr_invoc => {
                    let features = self.session.features_untracked();
                    if attr_kind == NonMacroAttrKind::Tool &&
                       !features.tool_attributes {
                        feature_err(&self.session.parse_sess, "tool_attributes",
                                    invoc.span(), GateIssue::Language,
                                    "tool attributes are unstable").emit();
                    }
                    if attr_kind == NonMacroAttrKind::Custom {
                        assert!(path.segments.len() == 1);
                        let name = path.segments[0].ident.name.as_str();
                        if name.starts_with("rustc_") {
                            if !features.rustc_attrs {
                                let msg = "unless otherwise specified, attributes with the prefix \
                                        `rustc_` are reserved for internal compiler diagnostics";
                                feature_err(&self.session.parse_sess, "rustc_attrs", invoc.span(),
                                            GateIssue::Language, &msg).emit();
                            }
                        } else if name.starts_with("derive_") {
                            if !features.custom_derive {
                                feature_err(&self.session.parse_sess, "custom_derive", invoc.span(),
                                            GateIssue::Language, EXPLAIN_DERIVE_UNDERSCORE).emit();
                            }
                        } else if !features.custom_attribute {
                            let msg = format!("The attribute `{}` is currently unknown to the \
                                               compiler and may have meaning added to it in the \
                                               future", path);
                            feature_err(&self.session.parse_sess, "custom_attribute", invoc.span(),
                                        GateIssue::Language, &msg).emit();
                        }
                    }

                    return Ok(Some(Lrc::new(SyntaxExtension::NonMacroAttr {
                        mark_used: attr_kind == NonMacroAttrKind::Tool,
                    })));
                }
                _ => {
                    self.report_non_macro_attr(path.span, def);
                    return Err(Determinacy::Determined);
                }
            }
        }
        let def_id = def.def_id();

        self.macro_defs.insert(invoc.expansion_data.mark, def_id);
        let normal_module_def_id =
            self.macro_def_scope(invoc.expansion_data.mark).normal_ancestor_id;
        self.definitions.add_parent_module_of_macro_def(invoc.expansion_data.mark,
                                                        normal_module_def_id);

        self.unused_macros.remove(&def_id);
        let ext = self.get_macro(def);
        invoc.expansion_data.mark.set_default_transparency(ext.default_transparency());
        invoc.expansion_data.mark.set_is_builtin(def_id.krate == BUILTIN_MACROS_CRATE);
        Ok(Some(ext))
    }

    fn resolve_macro(&mut self, scope: Mark, path: &ast::Path, kind: MacroKind, force: bool)
                     -> Result<Lrc<SyntaxExtension>, Determinacy> {
        self.resolve_macro_to_def(scope, path, kind, force).and_then(|def| {
            if let Def::Macro(_, MacroKind::ProcMacroStub) = def {
                self.report_proc_macro_stub(path.span);
                return Err(Determinacy::Determined);
            } else if let Def::NonMacroAttr(..) = def {
                self.report_non_macro_attr(path.span, def);
                return Err(Determinacy::Determined);
            }
            self.unused_macros.remove(&def.def_id());
            Ok(self.get_macro(def))
        })
    }

    fn check_unused_macros(&self) {
        for did in self.unused_macros.iter() {
            let id_span = match *self.macro_map[did] {
                SyntaxExtension::NormalTT { def_info, .. } |
                SyntaxExtension::DeclMacro { def_info, .. } => def_info,
                _ => None,
            };
            if let Some((id, span)) = id_span {
                let lint = lint::builtin::UNUSED_MACROS;
                let msg = "unused macro definition";
                self.session.buffer_lint(lint, id, span, msg);
            } else {
                bug!("attempted to create unused macro error, but span not available");
            }
        }
    }
}

impl<'a, 'cl> Resolver<'a, 'cl> {
    fn report_proc_macro_stub(&self, span: Span) {
        self.session.span_err(span,
                              "can't use a procedural macro from the same crate that defines it");
    }

    fn report_non_macro_attr(&self, span: Span, def: Def) {
        self.session.span_err(span, &format!("expected a macro, found {}", def.kind_name()));
    }

    fn resolve_invoc_to_def(&mut self, invoc: &Invocation, scope: Mark, force: bool)
                            -> Result<Def, Determinacy> {
        let (attr, traits) = match invoc.kind {
            InvocationKind::Attr { ref attr, ref traits, .. } => (attr, traits),
            InvocationKind::Bang { ref mac, .. } => {
                return self.resolve_macro_to_def(scope, &mac.node.path, MacroKind::Bang, force);
            }
            InvocationKind::Derive { ref path, .. } => {
                return self.resolve_macro_to_def(scope, path, MacroKind::Derive, force);
            }
        };

        let path = attr.as_ref().unwrap().path.clone();
        let def = self.resolve_macro_to_def(scope, &path, MacroKind::Attr, force);
        if let Ok(Def::NonMacroAttr(NonMacroAttrKind::Custom)) = def {} else {
            return def;
        }

        // At this point we've found that the `attr` is determinately unresolved and thus can be
        // interpreted as a custom attribute. Normally custom attributes are feature gated, but
        // it may be a custom attribute whitelisted by a derive macro and they do not require
        // a feature gate.
        //
        // So here we look through all of the derive annotations in scope and try to resolve them.
        // If they themselves successfully resolve *and* one of the resolved derive macros
        // whitelists this attribute's name, then this is a registered attribute and we can convert
        // it from a "generic custom attrite" into a "known derive helper attribute".
        enum ConvertToDeriveHelper { Yes, No, DontKnow }
        let mut convert_to_derive_helper = ConvertToDeriveHelper::No;
        let attr_name = path.segments[0].ident.name;
        for path in traits {
            match self.resolve_macro(scope, path, MacroKind::Derive, force) {
                Ok(ext) => if let SyntaxExtension::ProcMacroDerive(_, ref inert_attrs, _) = *ext {
                    if inert_attrs.contains(&attr_name) {
                        convert_to_derive_helper = ConvertToDeriveHelper::Yes;
                        break
                    }
                },
                Err(Determinacy::Undetermined) =>
                    convert_to_derive_helper = ConvertToDeriveHelper::DontKnow,
                Err(Determinacy::Determined) => {}
            }
        }

        match convert_to_derive_helper {
            ConvertToDeriveHelper::Yes => Ok(Def::NonMacroAttr(NonMacroAttrKind::DeriveHelper)),
            ConvertToDeriveHelper::No => def,
            ConvertToDeriveHelper::DontKnow => Err(Determinacy::determined(force)),
        }
    }

    fn resolve_macro_to_def(&mut self, scope: Mark, path: &ast::Path, kind: MacroKind, force: bool)
                            -> Result<Def, Determinacy> {
        let def = self.resolve_macro_to_def_inner(scope, path, kind, force);
        if def != Err(Determinacy::Undetermined) {
            // Do not report duplicated errors on every undetermined resolution.
            path.segments.iter().find(|segment| segment.args.is_some()).map(|segment| {
                self.session.span_err(segment.args.as_ref().unwrap().span(),
                                      "generic arguments in macro path");
            });
        }
        if kind != MacroKind::Bang && path.segments.len() > 1 &&
           def != Ok(Def::NonMacroAttr(NonMacroAttrKind::Tool)) {
            if !self.session.features_untracked().proc_macro_path_invoc {
                emit_feature_err(
                    &self.session.parse_sess,
                    "proc_macro_path_invoc",
                    path.span,
                    GateIssue::Language,
                    "paths of length greater than one in macro invocations are \
                     currently unstable",
                );
            }
        }
        def
    }

    pub fn resolve_macro_to_def_inner(&mut self, scope: Mark, path: &ast::Path,
                                  kind: MacroKind, force: bool)
                                  -> Result<Def, Determinacy> {
        let ast::Path { ref segments, span } = *path;
        let mut path: Vec<_> = segments.iter().map(|seg| seg.ident).collect();
        let invocation = self.invocations[&scope];
        let module = invocation.module.get();
        self.current_module = if module.is_trait() { module.parent.unwrap() } else { module };

        // Possibly apply the macro helper hack
        if self.use_extern_macros && kind == MacroKind::Bang && path.len() == 1 &&
           path[0].span.ctxt().outer().expn_info().map_or(false, |info| info.local_inner_macros) {
            let root = Ident::new(keywords::DollarCrate.name(), path[0].span);
            path.insert(0, root);
        }

        if path.len() > 1 {
            if !self.use_extern_macros && self.gated_errors.insert(span) {
                let msg = "non-ident macro paths are experimental";
                let feature = "use_extern_macros";
                emit_feature_err(&self.session.parse_sess, feature, span, GateIssue::Language, msg);
                self.found_unresolved_macro = true;
                return Err(Determinacy::Determined);
            }

            let def = match self.resolve_path(&path, Some(MacroNS), false, span, CrateLint::No) {
                PathResult::NonModule(path_res) => match path_res.base_def() {
                    Def::Err => Err(Determinacy::Determined),
                    def @ _ => {
                        if path_res.unresolved_segments() > 0 {
                            self.found_unresolved_macro = true;
                            self.session.span_err(span, "fail to resolve non-ident macro path");
                            Err(Determinacy::Determined)
                        } else {
                            Ok(def)
                        }
                    }
                },
                PathResult::Module(..) => unreachable!(),
                PathResult::Indeterminate if !force => return Err(Determinacy::Undetermined),
                _ => {
                    self.found_unresolved_macro = true;
                    Err(Determinacy::Determined)
                },
            };
            self.current_module.nearest_item_scope().macro_resolutions.borrow_mut()
                .push((path.into_boxed_slice(), span));
            return def;
        }

        let legacy_resolution = self.resolve_legacy_scope(&invocation.legacy_scope, path[0], false);
        let result = if let Some(MacroBinding::Legacy(binding)) = legacy_resolution {
            Ok(Def::Macro(binding.def_id, MacroKind::Bang))
        } else {
            match self.resolve_lexical_macro_path_segment(path[0], MacroNS, false, force,
                                                          kind == MacroKind::Attr, span) {
                Ok(binding) => Ok(binding.binding().def_ignoring_ambiguity()),
                Err(Determinacy::Undetermined) => return Err(Determinacy::Undetermined),
                Err(Determinacy::Determined) => {
                    self.found_unresolved_macro = true;
                    Err(Determinacy::Determined)
                }
            }
        };

        self.current_module.nearest_item_scope().legacy_macro_resolutions.borrow_mut()
            .push((scope, path[0], kind, result.ok()));

        result
    }

    // Resolve the initial segment of a non-global macro path
    // (e.g. `foo` in `foo::bar!(); or `foo!();`).
    // This is a variation of `fn resolve_ident_in_lexical_scope` that can be run during
    // expansion and import resolution (perhaps they can be merged in the future).
    pub fn resolve_lexical_macro_path_segment(&mut self,
                                              mut ident: Ident,
                                              ns: Namespace,
                                              record_used: bool,
                                              force: bool,
                                              is_attr: bool,
                                              path_span: Span)
                                              -> Result<MacroBinding<'a>, Determinacy> {
        // General principles:
        // 1. Not controlled (user-defined) names should have higher priority than controlled names
        //    built into the language or standard library. This way we can add new names into the
        //    language or standard library without breaking user code.
        // 2. "Closed set" below means new names can appear after the current resolution attempt.
        // Places to search (in order of decreasing priority):
        // (Type NS)
        // 1. FIXME: Ribs (type parameters), there's no necessary infrastructure yet
        //    (open set, not controlled).
        // 2. Names in modules (both normal `mod`ules and blocks), loop through hygienic parents
        //    (open, not controlled).
        // 3. Extern prelude (closed, not controlled).
        // 4. Tool modules (closed, controlled right now, but not in the future).
        // 5. Standard library prelude (de-facto closed, controlled).
        // 6. Language prelude (closed, controlled).
        // (Macro NS)
        // 1. Names in modules (both normal `mod`ules and blocks), loop through hygienic parents
        //    (open, not controlled).
        // 2. Macro prelude (language, standard library, user-defined legacy plugins lumped into
        //    one set) (open, the open part is from macro expansions, not controlled).
        // 2a. User-defined prelude from macro-use
        //    (open, the open part is from macro expansions, not controlled).
        // 2b. Standard library prelude, currently just a macro-use (closed, controlled)
        // 2c. Language prelude, perhaps including builtin attributes
        //    (closed, controlled, except for legacy plugins).
        // 3. Builtin attributes (closed, controlled).

        assert!(ns == TypeNS  || ns == MacroNS);
        assert!(force || !record_used); // `record_used` implies `force`
        ident = ident.modern();

        // Names from inner scope that can't shadow names from outer scopes, e.g.
        // mod m { ... }
        // {
        //     use prefix::*; // if this imports another `m`, then it can't shadow the outer `m`
        //                    // and we have and ambiguity error
        //     m::mac!();
        // }
        // This includes names from globs and from macro expansions.
        let mut potentially_ambiguous_result: Option<MacroBinding> = None;

        enum WhereToResolve<'a> {
            Module(Module<'a>),
            MacroPrelude,
            BuiltinAttrs,
            ExternPrelude,
            ToolPrelude,
            StdLibPrelude,
            PrimitiveTypes,
        }

        // Go through all the scopes and try to resolve the name.
        let mut where_to_resolve = WhereToResolve::Module(self.current_module);
        let mut use_prelude = !self.current_module.no_implicit_prelude;
        loop {
            let result = match where_to_resolve {
                WhereToResolve::Module(module) => {
                    let orig_current_module = mem::replace(&mut self.current_module, module);
                    let binding = self.resolve_ident_in_module_unadjusted(
                            module, ident, ns, true, record_used, path_span,
                    );
                    self.current_module = orig_current_module;
                    binding.map(MacroBinding::Modern)
                }
                WhereToResolve::MacroPrelude => {
                    match self.macro_prelude.get(&ident.name).cloned() {
                        Some(binding) => Ok(MacroBinding::Global(binding)),
                        None => Err(Determinacy::Determined),
                    }
                }
                WhereToResolve::BuiltinAttrs => {
                    // FIXME: Only built-in attributes are not considered as candidates for
                    // non-attributes to fight off regressions on stable channel (#53205).
                    // We need to come up with some more principled approach instead.
                    if is_attr && is_builtin_attr_name(ident.name) {
                        let binding = (Def::NonMacroAttr(NonMacroAttrKind::Builtin),
                                       ty::Visibility::Public, ident.span, Mark::root())
                                       .to_name_binding(self.arenas);
                        Ok(MacroBinding::Global(binding))
                    } else {
                        Err(Determinacy::Determined)
                    }
                }
                WhereToResolve::ExternPrelude => {
                    if use_prelude && self.extern_prelude.contains(&ident.name) {
                        if !self.session.features_untracked().extern_prelude &&
                           !self.ignore_extern_prelude_feature {
                            feature_err(&self.session.parse_sess, "extern_prelude",
                                        ident.span, GateIssue::Language,
                                        "access to extern crates through prelude is experimental")
                                        .emit();
                        }

                        let crate_id =
                            self.crate_loader.process_path_extern(ident.name, ident.span);
                        let crate_root =
                            self.get_module(DefId { krate: crate_id, index: CRATE_DEF_INDEX });
                        self.populate_module_if_necessary(crate_root);

                        let binding = (crate_root, ty::Visibility::Public,
                                       ident.span, Mark::root()).to_name_binding(self.arenas);
                        Ok(MacroBinding::Global(binding))
                    } else {
                        Err(Determinacy::Determined)
                    }
                }
                WhereToResolve::ToolPrelude => {
                    if use_prelude && is_known_tool(ident.name) {
                        let binding = (Def::ToolMod, ty::Visibility::Public,
                                       ident.span, Mark::root()).to_name_binding(self.arenas);
                        Ok(MacroBinding::Global(binding))
                    } else {
                        Err(Determinacy::Determined)
                    }
                }
                WhereToResolve::StdLibPrelude => {
                    let mut result = Err(Determinacy::Determined);
                    if use_prelude {
                        if let Some(prelude) = self.prelude {
                            if let Ok(binding) =
                                    self.resolve_ident_in_module_unadjusted(prelude, ident, ns,
                                                                          false, false, path_span) {
                                result = Ok(MacroBinding::Global(binding));
                            }
                        }
                    }
                    result
                }
                WhereToResolve::PrimitiveTypes => {
                    if let Some(prim_ty) =
                            self.primitive_type_table.primitive_types.get(&ident.name).cloned() {
                        let binding = (Def::PrimTy(prim_ty), ty::Visibility::Public,
                                       ident.span, Mark::root()).to_name_binding(self.arenas);
                        Ok(MacroBinding::Global(binding))
                    } else {
                        Err(Determinacy::Determined)
                    }
                }
            };

            macro_rules! continue_search { () => {
                where_to_resolve = match where_to_resolve {
                    WhereToResolve::Module(module) => {
                        match self.hygienic_lexical_parent(module, &mut ident.span) {
                            Some(parent_module) => WhereToResolve::Module(parent_module),
                            None => {
                                use_prelude = !module.no_implicit_prelude;
                                if ns == MacroNS {
                                    WhereToResolve::MacroPrelude
                                } else {
                                    WhereToResolve::ExternPrelude
                                }
                            }
                        }
                    }
                    WhereToResolve::MacroPrelude => WhereToResolve::BuiltinAttrs,
                    WhereToResolve::BuiltinAttrs => break, // nowhere else to search
                    WhereToResolve::ExternPrelude => WhereToResolve::ToolPrelude,
                    WhereToResolve::ToolPrelude => WhereToResolve::StdLibPrelude,
                    WhereToResolve::StdLibPrelude => WhereToResolve::PrimitiveTypes,
                    WhereToResolve::PrimitiveTypes => break, // nowhere else to search
                };

                continue;
            }}

            match result {
                Ok(result) => {
                    if !record_used {
                        return Ok(result);
                    }

                    let binding = result.binding();

                    // Found a solution that is ambiguous with a previously found solution.
                    // Push an ambiguity error for later reporting and
                    // return something for better recovery.
                    if let Some(previous_result) = potentially_ambiguous_result {
                        if binding.def() != previous_result.binding().def() {
                            self.ambiguity_errors.push(AmbiguityError {
                                span: path_span,
                                name: ident.name,
                                b1: previous_result.binding(),
                                b2: binding,
                                lexical: true,
                            });
                            return Ok(previous_result);
                        }
                    }

                    // Found a solution that's not an ambiguity yet, but is "suspicious" and
                    // can participate in ambiguities later on.
                    // Remember it and go search for other solutions in outer scopes.
                    if binding.is_glob_import() || binding.expansion != Mark::root() {
                        potentially_ambiguous_result = Some(result);

                        continue_search!();
                    }

                    // Found a solution that can't be ambiguous, great success.
                    return Ok(result);
                },
                Err(Determinacy::Determined) => {
                    continue_search!();
                }
                Err(Determinacy::Undetermined) => return Err(Determinacy::determined(force)),
            }
        }

        // Previously found potentially ambiguous result turned out to not be ambiguous after all.
        if let Some(previous_result) = potentially_ambiguous_result {
            return Ok(previous_result);
        }

        let determinacy = Determinacy::determined(force);
        if determinacy == Determinacy::Determined && is_attr {
            // For single-segment attributes interpret determinate "no resolution" as a custom
            // attribute. (Lexical resolution implies the first segment and is_attr should imply
            // the last segment, so we are certainly working with a single-segment attribute here.)
            assert!(ns == MacroNS);
            let binding = (Def::NonMacroAttr(NonMacroAttrKind::Custom),
                           ty::Visibility::Public, ident.span, Mark::root())
                           .to_name_binding(self.arenas);
            Ok(MacroBinding::Global(binding))
        } else {
            Err(determinacy)
        }
    }

    pub fn resolve_legacy_scope(&mut self,
                                mut scope: &'a Cell<LegacyScope<'a>>,
                                ident: Ident,
                                record_used: bool)
                                -> Option<MacroBinding<'a>> {
        let ident = ident.modern();
        let mut possible_time_travel = None;
        let mut relative_depth: u32 = 0;
        let mut binding = None;
        loop {
            match scope.get() {
                LegacyScope::Empty => break,
                LegacyScope::Expansion(invocation) => {
                    match invocation.expansion.get() {
                        LegacyScope::Invocation(_) => scope.set(invocation.legacy_scope.get()),
                        LegacyScope::Empty => {
                            if possible_time_travel.is_none() {
                                possible_time_travel = Some(scope);
                            }
                            scope = &invocation.legacy_scope;
                        }
                        _ => {
                            relative_depth += 1;
                            scope = &invocation.expansion;
                        }
                    }
                }
                LegacyScope::Invocation(invocation) => {
                    relative_depth = relative_depth.saturating_sub(1);
                    scope = &invocation.legacy_scope;
                }
                LegacyScope::Binding(potential_binding) => {
                    if potential_binding.ident == ident {
                        if (!self.use_extern_macros || record_used) && relative_depth > 0 {
                            self.disallowed_shadowing.push(potential_binding);
                        }
                        binding = Some(potential_binding);
                        break
                    }
                    scope = &potential_binding.parent;
                }
            };
        }

        let binding = if let Some(binding) = binding {
            MacroBinding::Legacy(binding)
        } else if let Some(binding) = self.macro_prelude.get(&ident.name).cloned() {
            if !self.use_extern_macros {
                self.record_use(ident, MacroNS, binding, DUMMY_SP);
            }
            MacroBinding::Global(binding)
        } else {
            return None;
        };

        if !self.use_extern_macros {
            if let Some(scope) = possible_time_travel {
                // Check for disallowed shadowing later
                self.lexical_macro_resolutions.push((ident, scope));
            }
        }

        Some(binding)
    }

    pub fn finalize_current_module_macro_resolutions(&mut self) {
        let module = self.current_module;
        for &(ref path, span) in module.macro_resolutions.borrow().iter() {
            match self.resolve_path(&path, Some(MacroNS), true, span, CrateLint::No) {
                PathResult::NonModule(_) => {},
                PathResult::Failed(span, msg, _) => {
                    resolve_error(self, span, ResolutionError::FailedToResolve(&msg));
                }
                _ => unreachable!(),
            }
        }

        for &(mark, ident, kind, def) in module.legacy_macro_resolutions.borrow().iter() {
            let span = ident.span;
            let legacy_scope = &self.invocations[&mark].legacy_scope;
            let legacy_resolution = self.resolve_legacy_scope(legacy_scope, ident, true);
            let resolution = self.resolve_lexical_macro_path_segment(ident, MacroNS, true, true,
                                                                     kind == MacroKind::Attr, span);

            let check_consistency = |this: &Self, binding: MacroBinding| {
                if let Some(def) = def {
                    if this.ambiguity_errors.is_empty() && this.disallowed_shadowing.is_empty() &&
                       binding.def_ignoring_ambiguity() != def {
                        // Make sure compilation does not succeed if preferred macro resolution
                        // has changed after the macro had been expanded. In theory all such
                        // situations should be reported as ambiguity errors, so this is span-bug.
                        span_bug!(span, "inconsistent resolution for a macro");
                    }
                } else {
                    // It's possible that the macro was unresolved (indeterminate) and silently
                    // expanded into a dummy fragment for recovery during expansion.
                    // Now, post-expansion, the resolution may succeed, but we can't change the
                    // past and need to report an error.
                    let msg =
                        format!("cannot determine resolution for the {} `{}`", kind.descr(), ident);
                    let msg_note = "import resolution is stuck, try simplifying macro imports";
                    this.session.struct_span_err(span, &msg).note(msg_note).emit();
                }
            };

            match (legacy_resolution, resolution) {
                (Some(MacroBinding::Legacy(legacy_binding)), Ok(MacroBinding::Modern(binding))) => {
                    if legacy_binding.def_id != binding.def_ignoring_ambiguity().def_id() {
                        let msg1 = format!("`{}` could refer to the macro defined here", ident);
                        let msg2 =
                            format!("`{}` could also refer to the macro imported here", ident);
                        self.session.struct_span_err(span, &format!("`{}` is ambiguous", ident))
                            .span_note(legacy_binding.span, &msg1)
                            .span_note(binding.span, &msg2)
                            .emit();
                    }
                },
                (None, Err(_)) => {
                    assert!(def.is_none());
                    let bang = if kind == MacroKind::Bang { "!" } else { "" };
                    let msg =
                        format!("cannot find {} `{}{}` in this scope", kind.descr(), ident, bang);
                    let mut err = self.session.struct_span_err(span, &msg);
                    self.suggest_macro_name(&ident.as_str(), kind, &mut err, span);
                    err.emit();
                },
                (Some(MacroBinding::Modern(_)), _) | (_, Ok(MacroBinding::Legacy(_))) => {
                    span_bug!(span, "impossible macro resolution result");
                }
                // OK, unambiguous resolution
                (Some(binding), Err(_)) | (None, Ok(binding)) |
                // OK, legacy wins over global even if their definitions are different
                (Some(binding @ MacroBinding::Legacy(_)), Ok(MacroBinding::Global(_))) |
                // OK, modern wins over global even if their definitions are different
                (Some(MacroBinding::Global(_)), Ok(binding @ MacroBinding::Modern(_))) => {
                    check_consistency(self, binding);
                }
                (Some(MacroBinding::Global(binding1)), Ok(MacroBinding::Global(binding2))) => {
                    if binding1.def() != binding2.def() {
                        span_bug!(span, "mismatch between same global macro resolutions");
                    }
                    check_consistency(self, MacroBinding::Global(binding1));

                    self.record_use(ident, MacroNS, binding1, span);
                    self.err_if_macro_use_proc_macro(ident.name, span, binding1);
                },
            };
        }
    }

    fn suggest_macro_name(&mut self, name: &str, kind: MacroKind,
                          err: &mut DiagnosticBuilder<'a>, span: Span) {
        // First check if this is a locally-defined bang macro.
        let suggestion = if let MacroKind::Bang = kind {
            find_best_match_for_name(self.macro_names.iter().map(|ident| &ident.name), name, None)
        } else {
            None
        // Then check global macros.
        }.or_else(|| {
            // FIXME: get_macro needs an &mut Resolver, can we do it without cloning?
            let macro_prelude = self.macro_prelude.clone();
            let names = macro_prelude.iter().filter_map(|(name, binding)| {
                if binding.get_macro(self).kind() == kind {
                    Some(name)
                } else {
                    None
                }
            });
            find_best_match_for_name(names, name, None)
        // Then check modules.
        }).or_else(|| {
            if !self.use_extern_macros {
                return None;
            }
            let is_macro = |def| {
                if let Def::Macro(_, def_kind) = def {
                    def_kind == kind
                } else {
                    false
                }
            };
            let ident = Ident::new(Symbol::intern(name), span);
            self.lookup_typo_candidate(&[ident], MacroNS, is_macro, span)
        });

        if let Some(suggestion) = suggestion {
            if suggestion != name {
                if let MacroKind::Bang = kind {
                    err.span_suggestion(span, "you could try the macro", suggestion.to_string());
                } else {
                    err.span_suggestion(span, "try", suggestion.to_string());
                }
            } else {
                err.help("have you added the `#[macro_use]` on the module/import?");
            }
        }
    }

    fn collect_def_ids(&mut self,
                       mark: Mark,
                       invocation: &'a InvocationData<'a>,
                       fragment: &AstFragment) {
        let Resolver { ref mut invocations, arenas, graph_root, .. } = *self;
        let InvocationData { def_index, .. } = *invocation;

        let visit_macro_invoc = &mut |invoc: map::MacroInvocationData| {
            invocations.entry(invoc.mark).or_insert_with(|| {
                arenas.alloc_invocation_data(InvocationData {
                    def_index: invoc.def_index,
                    module: Cell::new(graph_root),
                    expansion: Cell::new(LegacyScope::Empty),
                    legacy_scope: Cell::new(LegacyScope::Empty),
                })
            });
        };

        let mut def_collector = DefCollector::new(&mut self.definitions, mark);
        def_collector.visit_macro_invoc = Some(visit_macro_invoc);
        def_collector.with_parent(def_index, |def_collector| {
            fragment.visit_with(def_collector)
        });
    }

    pub fn define_macro(&mut self,
                        item: &ast::Item,
                        expansion: Mark,
                        legacy_scope: &mut LegacyScope<'a>) {
        self.local_macro_def_scopes.insert(item.id, self.current_module);
        let ident = item.ident;
        if ident.name == "macro_rules" {
            self.session.span_err(item.span, "user-defined macros may not be named `macro_rules`");
        }

        let def_id = self.definitions.local_def_id(item.id);
        let ext = Lrc::new(macro_rules::compile(&self.session.parse_sess,
                                               &self.session.features_untracked(),
                                               item, hygiene::default_edition()));
        self.macro_map.insert(def_id, ext);

        let def = match item.node { ast::ItemKind::MacroDef(ref def) => def, _ => unreachable!() };
        if def.legacy {
            let ident = ident.modern();
            self.macro_names.insert(ident);
            *legacy_scope = LegacyScope::Binding(self.arenas.alloc_legacy_binding(LegacyBinding {
                parent: Cell::new(*legacy_scope), ident: ident, def_id: def_id, span: item.span,
            }));
            let def = Def::Macro(def_id, MacroKind::Bang);
            self.all_macros.insert(ident.name, def);
            if attr::contains_name(&item.attrs, "macro_export") {
                if self.use_extern_macros {
                    let module = self.graph_root;
                    let vis = ty::Visibility::Public;
                    self.define(module, ident, MacroNS,
                                (def, vis, item.span, expansion, IsMacroExport));
                } else {
                    self.macro_exports.push(Export {
                        ident: ident.modern(),
                        def: def,
                        vis: ty::Visibility::Public,
                        span: item.span,
                    });
                }
            } else {
                self.unused_macros.insert(def_id);
            }
        } else {
            let module = self.current_module;
            let def = Def::Macro(def_id, MacroKind::Bang);
            let vis = self.resolve_visibility(&item.vis);
            if vis != ty::Visibility::Public {
                self.unused_macros.insert(def_id);
            }
            self.define(module, ident, MacroNS, (def, vis, item.span, expansion));
        }
    }

    /// Error if `ext` is a Macros 1.1 procedural macro being imported by `#[macro_use]`
    fn err_if_macro_use_proc_macro(&mut self, name: Name, use_span: Span,
                                   binding: &NameBinding<'a>) {
        let krate = binding.def().def_id().krate;

        // Plugin-based syntax extensions are exempt from this check
        if krate == BUILTIN_MACROS_CRATE { return; }

        let ext = binding.get_macro(self);

        match *ext {
            // If `ext` is a procedural macro, check if we've already warned about it
            SyntaxExtension::AttrProcMacro(..) | SyntaxExtension::ProcMacro { .. } =>
                if !self.warned_proc_macros.insert(name) { return; },
            _ => return,
        }

        let warn_msg = match *ext {
            SyntaxExtension::AttrProcMacro(..) =>
                "attribute procedural macros cannot be imported with `#[macro_use]`",
            SyntaxExtension::ProcMacro { .. } =>
                "procedural macros cannot be imported with `#[macro_use]`",
            _ => return,
        };

        let def_id = self.current_module.normal_ancestor_id;
        let node_id = self.definitions.as_local_node_id(def_id).unwrap();

        self.proc_mac_errors.push(ProcMacError {
            crate_name: self.cstore.crate_name_untracked(krate),
            name,
            module: node_id,
            use_span,
            warn_msg,
        });
    }

    pub fn report_proc_macro_import(&mut self, krate: &ast::Crate) {
        for err in self.proc_mac_errors.drain(..) {
            let (span, found_use) = ::UsePlacementFinder::check(krate, err.module);

            if let Some(span) = span {
                let found_use = if found_use { "" } else { "\n" };
                self.session.struct_span_err(err.use_span, err.warn_msg)
                    .span_suggestion(
                        span,
                        "instead, import the procedural macro like any other item",
                        format!("use {}::{};{}", err.crate_name, err.name, found_use),
                    ).emit();
            } else {
                self.session.struct_span_err(err.use_span, err.warn_msg)
                    .help(&format!("instead, import the procedural macro like any other item: \
                                    `use {}::{};`", err.crate_name, err.name))
                    .emit();
            }
        }
    }

    fn gate_legacy_custom_derive(&mut self, name: Symbol, span: Span) {
        if !self.session.features_untracked().custom_derive {
            let sess = &self.session.parse_sess;
            let explain = feature_gate::EXPLAIN_CUSTOM_DERIVE;
            emit_feature_err(sess, "custom_derive", span, GateIssue::Language, explain);
        } else if !self.is_whitelisted_legacy_custom_derive(name) {
            self.session.span_warn(span, feature_gate::EXPLAIN_DEPR_CUSTOM_DERIVE);
        }
    }
}
