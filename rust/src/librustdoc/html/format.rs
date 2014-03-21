// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
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
//! This module contains a large number of `fmt::Show` implementations for
//! various types in `rustdoc::clean`. These implementations all currently
//! assume that HTML output is desired, although it may be possible to redesign
//! them in the future to instead emit any format desired.

use std::fmt;
use std::local_data;
use std::io;

use syntax::ast;
use syntax::ast_util;

use clean;
use html::render;
use html::render::{cache_key, current_location_key};

/// Helper to render an optional visibility with a space after it (if the
/// visibility is preset)
pub struct VisSpace(Option<ast::Visibility>);
/// Similarly to VisSpace, this structure is used to render a purity with a
/// space after it.
pub struct PuritySpace(ast::Purity);
/// Wrapper struct for properly emitting a method declaration.
pub struct Method<'a>(&'a clean::SelfTy, &'a clean::FnDecl);

impl VisSpace {
    pub fn get(&self) -> Option<ast::Visibility> {
        let VisSpace(v) = *self; v
    }
}

impl PuritySpace {
    pub fn get(&self) -> ast::Purity {
        let PuritySpace(v) = *self; v
    }
}

impl fmt::Show for clean::Generics {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.lifetimes.len() == 0 && self.type_params.len() == 0 { return Ok(()) }
        try!(f.buf.write("&lt;".as_bytes()));

        for (i, life) in self.lifetimes.iter().enumerate() {
            if i > 0 {
                try!(f.buf.write(", ".as_bytes()));
            }
            try!(write!(f.buf, "{}", *life));
        }

        if self.type_params.len() > 0 {
            if self.lifetimes.len() > 0 {
                try!(f.buf.write(", ".as_bytes()));
            }

            for (i, tp) in self.type_params.iter().enumerate() {
                if i > 0 {
                    try!(f.buf.write(", ".as_bytes()))
                }
                try!(f.buf.write(tp.name.as_bytes()));

                if tp.bounds.len() > 0 {
                    try!(f.buf.write(": ".as_bytes()));
                    for (i, bound) in tp.bounds.iter().enumerate() {
                        if i > 0 {
                            try!(f.buf.write(" + ".as_bytes()));
                        }
                        try!(write!(f.buf, "{}", *bound));
                    }
                }
            }
        }
        try!(f.buf.write("&gt;".as_bytes()));
        Ok(())
    }
}

impl fmt::Show for clean::Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(f.buf.write("'".as_bytes()));
        try!(f.buf.write(self.get_ref().as_bytes()));
        Ok(())
    }
}

impl fmt::Show for clean::TyParamBound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::RegionBound => {
                f.buf.write("'static".as_bytes())
            }
            clean::TraitBound(ref ty) => {
                write!(f.buf, "{}", *ty)
            }
        }
    }
}

impl fmt::Show for clean::Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.global {
            try!(f.buf.write("::".as_bytes()))
        }
        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                try!(f.buf.write("::".as_bytes()))
            }
            try!(f.buf.write(seg.name.as_bytes()));

            if seg.lifetimes.len() > 0 || seg.types.len() > 0 {
                try!(f.buf.write("&lt;".as_bytes()));
                let mut comma = false;
                for lifetime in seg.lifetimes.iter() {
                    if comma {
                        try!(f.buf.write(", ".as_bytes()));
                    }
                    comma = true;
                    try!(write!(f.buf, "{}", *lifetime));
                }
                for ty in seg.types.iter() {
                    if comma {
                        try!(f.buf.write(", ".as_bytes()));
                    }
                    comma = true;
                    try!(write!(f.buf, "{}", *ty));
                }
                try!(f.buf.write("&gt;".as_bytes()));
            }
        }
        Ok(())
    }
}

/// Used when rendering a `ResolvedPath` structure. This invokes the `path`
/// rendering function with the necessary arguments for linking to a local path.
fn resolved_path(w: &mut io::Writer, id: ast::NodeId, p: &clean::Path,
                 print_all: bool) -> fmt::Result {
    path(w, p, print_all,
        |_cache, loc| { Some("../".repeat(loc.len())) },
        |cache| {
            match cache.paths.find(&id) {
                None => None,
                Some(&(ref fqp, shortty)) => Some((fqp.clone(), shortty))
            }
        })
}

/// Used when rendering an `ExternalPath` structure. Like `resolved_path` this
/// will invoke `path` with proper linking-style arguments.
fn external_path(w: &mut io::Writer, p: &clean::Path, print_all: bool,
                 fqn: &[~str], kind: clean::TypeKind,
                 krate: ast::CrateNum) -> fmt::Result {
    path(w, p, print_all,
        |cache, loc| {
            match *cache.extern_locations.get(&krate) {
                render::Remote(ref s) => Some(s.clone()),
                render::Local => Some("../".repeat(loc.len())),
                render::Unknown => None,
            }
        },
        |_cache| {
            Some((Vec::from_slice(fqn), match kind {
                clean::TypeStruct => "struct",
                clean::TypeEnum => "enum",
                clean::TypeFunction => "fn",
                clean::TypeTrait => "trait",
            }))
        })
}

fn path(w: &mut io::Writer, path: &clean::Path, print_all: bool,
        root: |&render::Cache, &[~str]| -> Option<~str>,
        info: |&render::Cache| -> Option<(Vec<~str> , &'static str)>)
    -> fmt::Result
{
    // The generics will get written to both the title and link
    let mut generics = ~"";
    let last = path.segments.last().unwrap();
    if last.lifetimes.len() > 0 || last.types.len() > 0 {
        let mut counter = 0;
        generics.push_str("&lt;");
        for lifetime in last.lifetimes.iter() {
            if counter > 0 { generics.push_str(", "); }
            counter += 1;
            generics.push_str(format!("{}", *lifetime));
        }
        for ty in last.types.iter() {
            if counter > 0 { generics.push_str(", "); }
            counter += 1;
            generics.push_str(format!("{}", *ty));
        }
        generics.push_str("&gt;");
    }

    // Did someone say rightward-drift?
    local_data::get(current_location_key, |loc| {
        let loc = loc.unwrap();

        local_data::get(cache_key, |cache| {
            let cache = cache.unwrap().get();
            let abs_root = root(cache, loc.as_slice());
            let rel_root = match path.segments.get(0).name.as_slice() {
                "self" => Some(~"./"),
                _ => None,
            };

            if print_all {
                let amt = path.segments.len() - 1;
                match rel_root {
                    Some(root) => {
                        let mut root = root;
                        for seg in path.segments.slice_to(amt).iter() {
                            if "super" == seg.name || "self" == seg.name {
                                try!(write!(w, "{}::", seg.name));
                            } else {
                                root.push_str(seg.name);
                                root.push_str("/");
                                try!(write!(w, "<a class='mod'
                                                    href='{}index.html'>{}</a>::",
                                              root,
                                              seg.name));
                            }
                        }
                    }
                    None => {
                        for seg in path.segments.slice_to(amt).iter() {
                            try!(write!(w, "{}::", seg.name));
                        }
                    }
                }
            }

            match info(cache) {
                // This is a documented path, link to it!
                Some((ref fqp, shortty)) if abs_root.is_some() => {
                    let mut url = abs_root.unwrap();
                    let to_link = fqp.slice_to(fqp.len() - 1);
                    for component in to_link.iter() {
                        url.push_str(*component);
                        url.push_str("/");
                    }
                    match shortty {
                        "mod" => {
                            url.push_str(*fqp.last().unwrap());
                            url.push_str("/index.html");
                        }
                        _ => {
                            url.push_str(shortty);
                            url.push_str(".");
                            url.push_str(*fqp.last().unwrap());
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
        })
    })
}

/// Helper to render type parameters
fn typarams(w: &mut io::Writer,
            typarams: &Option<Vec<clean::TyParamBound> >) -> fmt::Result {
    match *typarams {
        Some(ref params) => {
            try!(write!(w, "&lt;"));
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    try!(write!(w, ", "));
                }
                try!(write!(w, "{}", *param));
            }
            try!(write!(w, "&gt;"));
            Ok(())
        }
        None => Ok(())
    }
}

impl fmt::Show for clean::Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::TyParamBinder(id) | clean::Generic(id) => {
                local_data::get(cache_key, |cache| {
                    let m = cache.unwrap().get();
                    f.buf.write(m.typarams.get(&id).as_bytes())
                })
            }
            clean::ResolvedPath{id, typarams: ref tp, path: ref path} => {
                try!(resolved_path(f.buf, id, path, false));
                typarams(f.buf, tp)
            }
            clean::ExternalPath{path: ref path, typarams: ref tp,
                                fqn: ref fqn, kind, krate} => {
                try!(external_path(f.buf, path, false, fqn.as_slice(), kind,
                                     krate))
                typarams(f.buf, tp)
            }
            clean::Self(..) => f.buf.write("Self".as_bytes()),
            clean::Primitive(prim) => {
                let s = match prim {
                    ast::TyInt(ast::TyI) => "int",
                    ast::TyInt(ast::TyI8) => "i8",
                    ast::TyInt(ast::TyI16) => "i16",
                    ast::TyInt(ast::TyI32) => "i32",
                    ast::TyInt(ast::TyI64) => "i64",
                    ast::TyUint(ast::TyU) => "uint",
                    ast::TyUint(ast::TyU8) => "u8",
                    ast::TyUint(ast::TyU16) => "u16",
                    ast::TyUint(ast::TyU32) => "u32",
                    ast::TyUint(ast::TyU64) => "u64",
                    ast::TyFloat(ast::TyF32) => "f32",
                    ast::TyFloat(ast::TyF64) => "f64",
                    ast::TyStr => "str",
                    ast::TyBool => "bool",
                    ast::TyChar => "char",
                };
                f.buf.write(s.as_bytes())
            }
            clean::Closure(ref decl) => {
                let region = match decl.region {
                    Some(ref region) => format!("{} ", *region),
                    None => ~"",
                };

                write!(f.buf, "{}{}{arrow, select, yes{ -&gt; {ret}} other{}}",
                       PuritySpace(decl.purity),
                       match decl.sigil {
                           ast::OwnedSigil => format!("proc({})", decl.decl.inputs),
                           ast::BorrowedSigil => format!("{}|{}|", region, decl.decl.inputs),
                           ast::ManagedSigil => format!("@{}fn({})", region, decl.decl.inputs),
                       },
                       arrow = match decl.decl.output { clean::Unit => "no", _ => "yes" },
                       ret = decl.decl.output)
                // FIXME: where are bounds and lifetimes printed?!
            }
            clean::BareFunction(ref decl) => {
                write!(f.buf, "{}{}fn{}{}",
                       PuritySpace(decl.purity),
                       match decl.abi {
                           ref x if "" == *x => ~"",
                           ref x if "\"Rust\"" == *x => ~"",
                           ref s => " " + *s + " ",
                       },
                       decl.generics,
                       decl.decl)
            }
            clean::Tuple(ref typs) => {
                try!(f.buf.write("(".as_bytes()));
                for (i, typ) in typs.iter().enumerate() {
                    if i > 0 {
                        try!(f.buf.write(", ".as_bytes()))
                    }
                    try!(write!(f.buf, "{}", *typ));
                }
                f.buf.write(")".as_bytes())
            }
            clean::Vector(ref t) => write!(f.buf, "[{}]", **t),
            clean::FixedVector(ref t, ref s) => {
                write!(f.buf, "[{}, ..{}]", **t, *s)
            }
            clean::String => f.buf.write("str".as_bytes()),
            clean::Bool => f.buf.write("bool".as_bytes()),
            clean::Unit => f.buf.write("()".as_bytes()),
            clean::Bottom => f.buf.write("!".as_bytes()),
            clean::Unique(ref t) => write!(f.buf, "~{}", **t),
            clean::Managed(ref t) => write!(f.buf, "@{}", **t),
            clean::RawPointer(m, ref t) => {
                write!(f.buf, "*{}{}",
                       match m {
                           clean::Mutable => "mut ",
                           clean::Immutable => "",
                       }, **t)
            }
            clean::BorrowedRef{ lifetime: ref l, mutability, type_: ref ty} => {
                let lt = match *l { Some(ref l) => format!("{} ", *l), _ => ~"" };
                write!(f.buf, "&amp;{}{}{}",
                       lt,
                       match mutability {
                           clean::Mutable => "mut ",
                           clean::Immutable => "",
                       },
                       **ty)
            }
        }
    }
}

impl fmt::Show for clean::Arguments {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, input) in self.values.iter().enumerate() {
            if i > 0 { try!(write!(f.buf, ", ")); }
            if input.name.len() > 0 {
                try!(write!(f.buf, "{}: ", input.name));
            }
            try!(write!(f.buf, "{}", input.type_));
        }
        Ok(())
    }
}

impl fmt::Show for clean::FnDecl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f.buf, "({args}){arrow, select, yes{ -&gt; {ret}} other{}}",
               args = self.inputs,
               arrow = match self.output { clean::Unit => "no", _ => "yes" },
               ret = self.output)
    }
}

impl<'a> fmt::Show for Method<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Method(selfty, d) = *self;
        let mut args = ~"";
        match *selfty {
            clean::SelfStatic => {},
            clean::SelfValue => args.push_str("self"),
            clean::SelfOwned => args.push_str("~self"),
            clean::SelfBorrowed(Some(ref lt), clean::Immutable) => {
                args.push_str(format!("&amp;{} self", *lt));
            }
            clean::SelfBorrowed(Some(ref lt), clean::Mutable) => {
                args.push_str(format!("&amp;{} mut self", *lt));
            }
            clean::SelfBorrowed(None, clean::Mutable) => {
                args.push_str("&amp;mut self");
            }
            clean::SelfBorrowed(None, clean::Immutable) => {
                args.push_str("&amp;self");
            }
        }
        for (i, input) in d.inputs.values.iter().enumerate() {
            if i > 0 || args.len() > 0 { args.push_str(", "); }
            if input.name.len() > 0 {
                args.push_str(format!("{}: ", input.name));
            }
            args.push_str(format!("{}", input.type_));
        }
        write!(f.buf, "({args}){arrow, select, yes{ -&gt; {ret}} other{}}",
               args = args,
               arrow = match d.output { clean::Unit => "no", _ => "yes" },
               ret = d.output)
    }
}

impl fmt::Show for VisSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            Some(ast::Public) => write!(f.buf, "pub "),
            Some(ast::Private) => write!(f.buf, "priv "),
            Some(ast::Inherited) | None => Ok(())
        }
    }
}

impl fmt::Show for PuritySpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            ast::UnsafeFn => write!(f.buf, "unsafe "),
            ast::ExternFn => write!(f.buf, "extern "),
            ast::ImpureFn => Ok(())
        }
    }
}

impl fmt::Show for clean::ViewPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::SimpleImport(ref name, ref src) => {
                if *name == src.path.segments.last().unwrap().name {
                    write!(f.buf, "use {};", *src)
                } else {
                    write!(f.buf, "use {} = {};", *name, *src)
                }
            }
            clean::GlobImport(ref src) => {
                write!(f.buf, "use {}::*;", *src)
            }
            clean::ImportList(ref src, ref names) => {
                try!(write!(f.buf, "use {}::\\{", *src));
                for (i, n) in names.iter().enumerate() {
                    if i > 0 {
                        try!(write!(f.buf, ", "));
                    }
                    try!(write!(f.buf, "{}", *n));
                }
                write!(f.buf, "\\};")
            }
        }
    }
}

impl fmt::Show for clean::ImportSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.did {
            // FIXME: shouldn't be restricted to just local imports
            Some(did) if ast_util::is_local(did) => {
                resolved_path(f.buf, did.node, &self.path, true)
            }
            _ => {
                for (i, seg) in self.path.segments.iter().enumerate() {
                    if i > 0 {
                        try!(write!(f.buf, "::"))
                    }
                    try!(write!(f.buf, "{}", seg.name));
                }
                Ok(())
            }
        }
    }
}

impl fmt::Show for clean::ViewListIdent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.source {
            // FIXME: shouldn't be limited to just local imports
            Some(did) if ast_util::is_local(did) => {
                let path = clean::Path {
                    global: false,
                    segments: vec!(clean::PathSegment {
                        name: self.name.clone(),
                        lifetimes: Vec::new(),
                        types: Vec::new(),
                    })
                };
                resolved_path(f.buf, did.node, &path, false)
            }
            _ => write!(f.buf, "{}", self.name),
        }
    }
}
