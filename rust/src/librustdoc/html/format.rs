// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! HTML formatting module
//!
//! This module contains a large number of `fmt::Display` implementations for
//! various types in `rustdoc::clean`. These implementations all currently
//! assume that HTML output is desired, although it may be possible to redesign
//! them in the future to instead emit any format desired.

use std::fmt;
use std::iter::repeat;

use syntax::ast;
use syntax::ast_util;

use clean;
use stability_summary::ModuleSummary;
use html::item_type::ItemType;
use html::render;
use html::escape::Escape;
use html::render::{cache, CURRENT_LOCATION_KEY};

/// Helper to render an optional visibility with a space after it (if the
/// visibility is preset)
#[derive(Copy)]
pub struct VisSpace(pub Option<ast::Visibility>);
/// Similarly to VisSpace, this structure is used to render a function style with a
/// space after it.
#[derive(Copy)]
pub struct UnsafetySpace(pub ast::Unsafety);
/// Wrapper struct for properly emitting a method declaration.
pub struct Method<'a>(pub &'a clean::SelfTy, pub &'a clean::FnDecl);
/// Similar to VisSpace, but used for mutability
#[derive(Copy)]
pub struct MutableSpace(pub clean::Mutability);
/// Similar to VisSpace, but used for mutability
#[derive(Copy)]
pub struct RawMutableSpace(pub clean::Mutability);
/// Wrapper struct for properly emitting the stability level.
pub struct Stability<'a>(pub &'a Option<clean::Stability>);
/// Wrapper struct for emitting the stability level concisely.
pub struct ConciseStability<'a>(pub &'a Option<clean::Stability>);
/// Wrapper struct for emitting a where clause from Generics.
pub struct WhereClause<'a>(pub &'a clean::Generics);
/// Wrapper struct for emitting type parameter bounds.
pub struct TyParamBounds<'a>(pub &'a [clean::TyParamBound]);
/// Wrapper struct for emitting a comma-separated list of items
pub struct CommaSep<'a, T: 'a>(pub &'a [T]);

impl VisSpace {
    pub fn get(&self) -> Option<ast::Visibility> {
        let VisSpace(v) = *self; v
    }
}

impl UnsafetySpace {
    pub fn get(&self) -> ast::Unsafety {
        let UnsafetySpace(v) = *self; v
    }
}

impl<'a, T: fmt::Display> fmt::Display for CommaSep<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i != 0 { try!(write!(f, ", ")); }
            try!(write!(f, "{}", item));
        }
        Ok(())
    }
}

impl<'a> fmt::Display for TyParamBounds<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let &TyParamBounds(bounds) = self;
        for (i, bound) in bounds.iter().enumerate() {
            if i > 0 {
                try!(f.write_str(" + "));
            }
            try!(write!(f, "{}", *bound));
        }
        Ok(())
    }
}

impl fmt::Display for clean::Generics {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.lifetimes.len() == 0 && self.type_params.len() == 0 { return Ok(()) }
        try!(f.write_str("&lt;"));

        for (i, life) in self.lifetimes.iter().enumerate() {
            if i > 0 {
                try!(f.write_str(", "));
            }
            try!(write!(f, "{}", *life));
        }

        if self.type_params.len() > 0 {
            if self.lifetimes.len() > 0 {
                try!(f.write_str(", "));
            }
            for (i, tp) in self.type_params.iter().enumerate() {
                if i > 0 {
                    try!(f.write_str(", "))
                }
                try!(f.write_str(&tp.name));

                if tp.bounds.len() > 0 {
                    try!(write!(f, ": {}", TyParamBounds(&tp.bounds)));
                }

                match tp.default {
                    Some(ref ty) => { try!(write!(f, " = {}", ty)); },
                    None => {}
                };
            }
        }
        try!(f.write_str("&gt;"));
        Ok(())
    }
}

impl<'a> fmt::Display for WhereClause<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let &WhereClause(gens) = self;
        if gens.where_predicates.len() == 0 {
            return Ok(());
        }
        try!(f.write_str(" <span class='where'>where "));
        for (i, pred) in gens.where_predicates.iter().enumerate() {
            if i > 0 {
                try!(f.write_str(", "));
            }
            match pred {
                &clean::WherePredicate::BoundPredicate { ref ty, ref bounds } => {
                    let bounds = bounds;
                    try!(write!(f, "{}: {}", ty, TyParamBounds(bounds)));
                }
                &clean::WherePredicate::RegionPredicate { ref lifetime,
                                                          ref bounds } => {
                    try!(write!(f, "{}: ", lifetime));
                    for (i, lifetime) in bounds.iter().enumerate() {
                        if i > 0 {
                            try!(f.write_str(" + "));
                        }

                        try!(write!(f, "{}", lifetime));
                    }
                }
                &clean::WherePredicate::EqPredicate { ref lhs, ref rhs } => {
                    try!(write!(f, "{} == {}", lhs, rhs));
                }
            }
        }
        try!(f.write_str("</span>"));
        Ok(())
    }
}

impl fmt::Display for clean::Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(f.write_str(self.get_ref()));
        Ok(())
    }
}

impl fmt::Display for clean::PolyTrait {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.lifetimes.len() > 0 {
            try!(f.write_str("for&lt;"));
            for (i, lt) in self.lifetimes.iter().enumerate() {
                if i > 0 {
                    try!(f.write_str(", "));
                }
                try!(write!(f, "{}", lt));
            }
            try!(f.write_str("&gt; "));
        }
        write!(f, "{}", self.trait_)
    }
}

impl fmt::Display for clean::TyParamBound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::RegionBound(ref lt) => {
                write!(f, "{}", *lt)
            }
            clean::TraitBound(ref ty, modifier) => {
                let modifier_str = match modifier {
                    ast::TraitBoundModifier::None => "",
                    ast::TraitBoundModifier::Maybe => "?",
                };
                write!(f, "{}{}", modifier_str, *ty)
            }
        }
    }
}

impl fmt::Display for clean::PathParameters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::PathParameters::AngleBracketed {
                ref lifetimes, ref types, ref bindings
            } => {
                if lifetimes.len() > 0 || types.len() > 0 || bindings.len() > 0 {
                    try!(f.write_str("&lt;"));
                    let mut comma = false;
                    for lifetime in lifetimes {
                        if comma {
                            try!(f.write_str(", "));
                        }
                        comma = true;
                        try!(write!(f, "{}", *lifetime));
                    }
                    for ty in types {
                        if comma {
                            try!(f.write_str(", "));
                        }
                        comma = true;
                        try!(write!(f, "{}", *ty));
                    }
                    for binding in bindings {
                        if comma {
                            try!(f.write_str(", "));
                        }
                        comma = true;
                        try!(write!(f, "{}", *binding));
                    }
                    try!(f.write_str("&gt;"));
                }
            }
            clean::PathParameters::Parenthesized { ref inputs, ref output } => {
                try!(f.write_str("("));
                let mut comma = false;
                for ty in inputs {
                    if comma {
                        try!(f.write_str(", "));
                    }
                    comma = true;
                    try!(write!(f, "{}", *ty));
                }
                try!(f.write_str(")"));
                if let Some(ref ty) = *output {
                    try!(f.write_str(" -&gt; "));
                    try!(write!(f, "{}", ty));
                }
            }
        }
        Ok(())
    }
}

impl fmt::Display for clean::PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(f.write_str(&self.name));
        write!(f, "{}", self.params)
    }
}

impl fmt::Display for clean::Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.global {
            try!(f.write_str("::"))
        }

        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                try!(f.write_str("::"))
            }
            try!(write!(f, "{}", seg));
        }
        Ok(())
    }
}

/// Used when rendering a `ResolvedPath` structure. This invokes the `path`
/// rendering function with the necessary arguments for linking to a local path.
fn resolved_path(w: &mut fmt::Formatter, did: ast::DefId, p: &clean::Path,
                 print_all: bool) -> fmt::Result {
    path(w, p, print_all,
        |cache, loc| {
            if ast_util::is_local(did) || cache.inlined.contains(&did) {
                Some(repeat("../").take(loc.len()).collect::<String>())
            } else {
                match cache.extern_locations[&did.krate] {
                    render::Remote(ref s) => Some(s.to_string()),
                    render::Local => {
                        Some(repeat("../").take(loc.len()).collect::<String>())
                    }
                    render::Unknown => None,
                }
            }
        },
        |cache| {
            match cache.paths.get(&did) {
                None => None,
                Some(&(ref fqp, shortty)) => Some((fqp.clone(), shortty))
            }
        })
}

fn path<F, G>(w: &mut fmt::Formatter,
              path: &clean::Path,
              print_all: bool,
              root: F,
              info: G)
              -> fmt::Result where
    F: FnOnce(&render::Cache, &[String]) -> Option<String>,
    G: FnOnce(&render::Cache) -> Option<(Vec<String>, ItemType)>,
{
    // The generics will get written to both the title and link
    let last = path.segments.last().unwrap();
    let generics = format!("{}", last.params);

    let loc = CURRENT_LOCATION_KEY.with(|l| l.borrow().clone());
    let cache = cache();
    let abs_root = root(&*cache, &loc);
    let rel_root = match &*path.segments[0].name {
        "self" => Some("./".to_string()),
        _ => None,
    };

    if print_all {
        let amt = path.segments.len() - 1;
        match rel_root {
            Some(root) => {
                let mut root = String::from_str(&root);
                for seg in &path.segments[..amt] {
                    if "super" == seg.name ||
                            "self" == seg.name {
                        try!(write!(w, "{}::", seg.name));
                    } else {
                        root.push_str(&seg.name);
                        root.push_str("/");
                        try!(write!(w, "<a class='mod'
                                            href='{}index.html'>{}</a>::",
                                      root,
                                      seg.name));
                    }
                }
            }
            None => {
                for seg in &path.segments[..amt] {
                    try!(write!(w, "{}::", seg.name));
                }
            }
        }
    }

    match info(&*cache) {
        // This is a documented path, link to it!
        Some((ref fqp, shortty)) if abs_root.is_some() => {
            let mut url = String::from_str(&abs_root.unwrap());
            let to_link = &fqp[..fqp.len() - 1];
            for component in to_link {
                url.push_str(component);
                url.push_str("/");
            }
            match shortty {
                ItemType::Module => {
                    url.push_str(fqp.last().unwrap());
                    url.push_str("/index.html");
                }
                _ => {
                    url.push_str(shortty.to_static_str());
                    url.push_str(".");
                    url.push_str(fqp.last().unwrap());
                    url.push_str(".html");
                }
            }

            try!(write!(w, "<a class='{}' href='{}' title='{}'>{}</a>",
                          shortty, url, fqp.connect("::"), last.name));
        }

        _ => {
            try!(write!(w, "{}", last.name));
        }
    }
    try!(write!(w, "{}", generics));
    Ok(())
}

fn primitive_link(f: &mut fmt::Formatter,
                  prim: clean::PrimitiveType,
                  name: &str) -> fmt::Result {
    let m = cache();
    let mut needs_termination = false;
    match m.primitive_locations.get(&prim) {
        Some(&ast::LOCAL_CRATE) => {
            let len = CURRENT_LOCATION_KEY.with(|s| s.borrow().len());
            let len = if len == 0 {0} else {len - 1};
            try!(write!(f, "<a href='{}primitive.{}.html'>",
                        repeat("../").take(len).collect::<String>(),
                        prim.to_url_str()));
            needs_termination = true;
        }
        Some(&cnum) => {
            let path = &m.paths[&ast::DefId {
                krate: cnum,
                node: ast::CRATE_NODE_ID,
            }];
            let loc = match m.extern_locations[&cnum] {
                render::Remote(ref s) => Some(s.to_string()),
                render::Local => {
                    let len = CURRENT_LOCATION_KEY.with(|s| s.borrow().len());
                    Some(repeat("../").take(len).collect::<String>())
                }
                render::Unknown => None,
            };
            match loc {
                Some(root) => {
                    try!(write!(f, "<a href='{}{}/primitive.{}.html'>",
                                root,
                                path.0.first().unwrap(),
                                prim.to_url_str()));
                    needs_termination = true;
                }
                None => {}
            }
        }
        None => {}
    }
    try!(write!(f, "{}", name));
    if needs_termination {
        try!(write!(f, "</a>"));
    }
    Ok(())
}

/// Helper to render type parameters
fn tybounds(w: &mut fmt::Formatter,
            typarams: &Option<Vec<clean::TyParamBound> >) -> fmt::Result {
    match *typarams {
        Some(ref params) => {
            for param in params {
                try!(write!(w, " + "));
                try!(write!(w, "{}", *param));
            }
            Ok(())
        }
        None => Ok(())
    }
}

impl fmt::Display for clean::Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::Generic(ref name) => {
                f.write_str(name)
            }
            clean::ResolvedPath{ did, ref typarams, ref path } => {
                // Paths like Self::Output should be rendered with all segments
                try!(resolved_path(f, did, path, path.segments[0].name == "Self"));
                tybounds(f, typarams)
            }
            clean::Infer => write!(f, "_"),
            clean::Primitive(prim) => primitive_link(f, prim, prim.to_string()),
            clean::BareFunction(ref decl) => {
                write!(f, "{}{}fn{}{}",
                       UnsafetySpace(decl.unsafety),
                       match &*decl.abi {
                           "" => " extern ".to_string(),
                           "\"Rust\"" => "".to_string(),
                           s => format!(" extern {} ", s)
                       },
                       decl.generics,
                       decl.decl)
            }
            clean::Tuple(ref typs) => {
                primitive_link(f, clean::PrimitiveTuple,
                               &*match &**typs {
                                    [ref one] => format!("({},)", one),
                                    many => format!("({})", CommaSep(&many)),
                               })
            }
            clean::Vector(ref t) => {
                primitive_link(f, clean::Slice, &format!("[{}]", **t))
            }
            clean::FixedVector(ref t, ref s) => {
                primitive_link(f, clean::PrimitiveType::Array,
                               &format!("[{}; {}]", **t, *s))
            }
            clean::Bottom => f.write_str("!"),
            clean::RawPointer(m, ref t) => {
                write!(f, "*{}{}", RawMutableSpace(m), **t)
            }
            clean::BorrowedRef{ lifetime: ref l, mutability, type_: ref ty} => {
                let lt = match *l {
                    Some(ref l) => format!("{} ", *l),
                    _ => "".to_string(),
                };
                let m = MutableSpace(mutability);
                match **ty {
                    clean::Vector(ref bt) => { // BorrowedRef{ ... Vector(T) } is &[T]
                        match **bt {
                            clean::Generic(_) =>
                                primitive_link(f, clean::Slice,
                                    &format!("&amp;{}{}[{}]", lt, m, **bt)),
                            _ => {
                                try!(primitive_link(f, clean::Slice,
                                    &format!("&amp;{}{}[", lt, m)));
                                try!(write!(f, "{}", **bt));
                                primitive_link(f, clean::Slice, "]")
                            }
                        }
                    }
                    _ => {
                        write!(f, "&amp;{}{}{}", lt, m, **ty)
                    }
                }
            }
            clean::PolyTraitRef(ref bounds) => {
                for (i, bound) in bounds.iter().enumerate() {
                    if i != 0 {
                        try!(write!(f, " + "));
                    }
                    try!(write!(f, "{}", *bound));
                }
                Ok(())
            }
            clean::QPath { ref name, ref self_type, ref trait_ } => {
                write!(f, "&lt;{} as {}&gt;::{}", self_type, trait_, name)
            }
            clean::Unique(..) => {
                panic!("should have been cleaned")
            }
        }
    }
}

impl fmt::Display for clean::Arguments {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, input) in self.values.iter().enumerate() {
            if i > 0 { try!(write!(f, ", ")); }
            if input.name.len() > 0 {
                try!(write!(f, "{}: ", input.name));
            }
            try!(write!(f, "{}", input.type_));
        }
        Ok(())
    }
}

impl fmt::Display for clean::FunctionRetTy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::Return(clean::Tuple(ref tys)) if tys.is_empty() => Ok(()),
            clean::Return(ref ty) => write!(f, " -&gt; {}", ty),
            clean::DefaultReturn => Ok(()),
            clean::NoReturn => write!(f, " -&gt; !")
        }
    }
}

impl fmt::Display for clean::FnDecl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({args}){arrow}", args = self.inputs, arrow = self.output)
    }
}

impl<'a> fmt::Display for Method<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Method(selfty, d) = *self;
        let mut args = String::new();
        match *selfty {
            clean::SelfStatic => {},
            clean::SelfValue => args.push_str("self"),
            clean::SelfBorrowed(Some(ref lt), mtbl) => {
                args.push_str(&format!("&amp;{} {}self", *lt, MutableSpace(mtbl)));
            }
            clean::SelfBorrowed(None, mtbl) => {
                args.push_str(&format!("&amp;{}self", MutableSpace(mtbl)));
            }
            clean::SelfExplicit(ref typ) => {
                args.push_str(&format!("self: {}", *typ));
            }
        }
        for (i, input) in d.inputs.values.iter().enumerate() {
            if i > 0 || args.len() > 0 { args.push_str(", "); }
            if input.name.len() > 0 {
                args.push_str(&format!("{}: ", input.name));
            }
            args.push_str(&format!("{}", input.type_));
        }
        write!(f, "({args}){arrow}", args = args, arrow = d.output)
    }
}

impl fmt::Display for VisSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            Some(ast::Public) => write!(f, "pub "),
            Some(ast::Inherited) | None => Ok(())
        }
    }
}

impl fmt::Display for UnsafetySpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            ast::Unsafety::Unsafe => write!(f, "unsafe "),
            ast::Unsafety::Normal => Ok(())
        }
    }
}

impl fmt::Display for clean::Import {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::SimpleImport(ref name, ref src) => {
                if *name == src.path.segments.last().unwrap().name {
                    write!(f, "use {};", *src)
                } else {
                    write!(f, "use {} as {};", *src, *name)
                }
            }
            clean::GlobImport(ref src) => {
                write!(f, "use {}::*;", *src)
            }
            clean::ImportList(ref src, ref names) => {
                try!(write!(f, "use {}::{{", *src));
                for (i, n) in names.iter().enumerate() {
                    if i > 0 {
                        try!(write!(f, ", "));
                    }
                    try!(write!(f, "{}", *n));
                }
                write!(f, "}};")
            }
        }
    }
}

impl fmt::Display for clean::ImportSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.did {
            Some(did) => resolved_path(f, did, &self.path, true),
            _ => {
                for (i, seg) in self.path.segments.iter().enumerate() {
                    if i > 0 {
                        try!(write!(f, "::"))
                    }
                    try!(write!(f, "{}", seg.name));
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for clean::ViewListIdent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.source {
            Some(did) => {
                let path = clean::Path {
                    global: false,
                    segments: vec!(clean::PathSegment {
                        name: self.name.clone(),
                        params: clean::PathParameters::AngleBracketed {
                            lifetimes: Vec::new(),
                            types: Vec::new(),
                            bindings: Vec::new()
                        }
                    })
                };
                resolved_path(f, did, &path, false)
            }
            _ => write!(f, "{}", self.name),
        }
    }
}

impl fmt::Display for clean::TypeBinding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}={}", self.name, self.ty)
    }
}

impl fmt::Display for MutableSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            MutableSpace(clean::Immutable) => Ok(()),
            MutableSpace(clean::Mutable) => write!(f, "mut "),
        }
    }
}

impl fmt::Display for RawMutableSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            RawMutableSpace(clean::Immutable) => write!(f, "const "),
            RawMutableSpace(clean::Mutable) => write!(f, "mut "),
        }
    }
}

impl<'a> fmt::Display for Stability<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Stability(stab) = *self;
        match *stab {
            Some(ref stability) => {
                let lvl = if stability.deprecated_since.is_empty() {
                    format!("{}", stability.level)
                } else {
                    "Deprecated".to_string()
                };
                write!(f, "<a class='stability {lvl}' title='{reason}'>{lvl}</a>",
                       lvl = Escape(&*lvl),
                       reason = Escape(&*stability.reason))
            }
            None => Ok(())
        }
    }
}

impl<'a> fmt::Display for ConciseStability<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ConciseStability(stab) = *self;
        match *stab {
            Some(ref stability) => {
                let lvl = if stability.deprecated_since.is_empty() {
                    format!("{}", stability.level)
                } else {
                    "Deprecated".to_string()
                };
                write!(f, "<a class='stability {lvl}' title='{lvl}{colon}{reason}'></a>",
                       lvl = Escape(&*lvl),
                       colon = if stability.reason.len() > 0 { ": " } else { "" },
                       reason = Escape(&*stability.reason))
            }
            None => {
                write!(f, "<a class='stability Unmarked' title='No stability level'></a>")
            }
        }
    }
}

impl fmt::Display for ModuleSummary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn fmt_inner<'a>(f: &mut fmt::Formatter,
                         context: &mut Vec<&'a str>,
                         m: &'a ModuleSummary)
                     -> fmt::Result {
            let cnt = m.counts;
            let tot = cnt.total();
            if tot == 0 { return Ok(()) }

            context.push(&m.name);
            let path = context.connect("::");

            try!(write!(f, "<tr>"));
            try!(write!(f, "<td><a href='{}'>{}</a></td>", {
                            let mut url = context[1..].to_vec();
                            url.push("index.html");
                            url.connect("/")
                        },
                        path));
            try!(write!(f, "<td class='summary-column'>"));
            try!(write!(f, "<span class='summary Stable' \
                            style='width: {:.4}%; display: inline-block'>&nbsp</span>",
                        (100 * cnt.stable) as f64/tot as f64));
            try!(write!(f, "<span class='summary Unstable' \
                            style='width: {:.4}%; display: inline-block'>&nbsp</span>",
                        (100 * cnt.unstable) as f64/tot as f64));
            try!(write!(f, "<span class='summary Deprecated' \
                            style='width: {:.4}%; display: inline-block'>&nbsp</span>",
                        (100 * cnt.deprecated) as f64/tot as f64));
            try!(write!(f, "<span class='summary Unmarked' \
                            style='width: {:.4}%; display: inline-block'>&nbsp</span>",
                        (100 * cnt.unmarked) as f64/tot as f64));
            try!(write!(f, "</td></tr>"));

            for submodule in &m.submodules {
                try!(fmt_inner(f, context, submodule));
            }
            context.pop();
            Ok(())
        }

        let mut context = Vec::new();

        let tot = self.counts.total();
        let (stable, unstable, deprecated, unmarked) = if tot == 0 {
            (0, 0, 0, 0)
        } else {
            ((100 * self.counts.stable)/tot,
             (100 * self.counts.unstable)/tot,
             (100 * self.counts.deprecated)/tot,
             (100 * self.counts.unmarked)/tot)
        };

        try!(write!(f,
r"<h1 class='fqn'>Stability dashboard: crate <a class='mod' href='index.html'>{name}</a></h1>
This dashboard summarizes the stability levels for all of the public modules of
the crate, according to the total number of items at each level in the module and
its children (percentages total for {name}):
<blockquote>
<a class='stability Stable'></a> stable ({}%),<br/>
<a class='stability Unstable'></a> unstable ({}%),<br/>
<a class='stability Deprecated'></a> deprecated ({}%),<br/>
<a class='stability Unmarked'></a> unmarked ({}%)
</blockquote>
The counts do not include methods or trait
implementations that are visible only through a re-exported type.",
stable, unstable, deprecated, unmarked,
name=self.name));
        try!(write!(f, "<table>"));
        try!(fmt_inner(f, &mut context, self));
        write!(f, "</table>")
    }
}
