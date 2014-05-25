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
//! This module contains a large number of `fmt::Show` implementations for
//! various types in `rustdoc::clean`. These implementations all currently
//! assume that HTML output is desired, although it may be possible to redesign
//! them in the future to instead emit any format desired.

use std::fmt;
use std::string::String;

use syntax::ast;
use syntax::ast_util;

use clean;
use html::item_type;
use html::item_type::ItemType;
use html::render;
use html::render::{cache_key, current_location_key};

/// Helper to render an optional visibility with a space after it (if the
/// visibility is preset)
pub struct VisSpace(pub Option<ast::Visibility>);
/// Similarly to VisSpace, this structure is used to render a function style with a
/// space after it.
pub struct FnStyleSpace(pub ast::FnStyle);
/// Wrapper struct for properly emitting a method declaration.
pub struct Method<'a>(pub &'a clean::SelfTy, pub &'a clean::FnDecl);

impl VisSpace {
    pub fn get(&self) -> Option<ast::Visibility> {
        let VisSpace(v) = *self; v
    }
}

impl FnStyleSpace {
    pub fn get(&self) -> ast::FnStyle {
        let FnStyleSpace(v) = *self; v
    }
}

impl fmt::Show for clean::Generics {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.lifetimes.len() == 0 && self.type_params.len() == 0 { return Ok(()) }
        try!(f.write("&lt;".as_bytes()));

        for (i, life) in self.lifetimes.iter().enumerate() {
            if i > 0 {
                try!(f.write(", ".as_bytes()));
            }
            try!(write!(f, "{}", *life));
        }

        if self.type_params.len() > 0 {
            if self.lifetimes.len() > 0 {
                try!(f.write(", ".as_bytes()));
            }

            for (i, tp) in self.type_params.iter().enumerate() {
                if i > 0 {
                    try!(f.write(", ".as_bytes()))
                }
                try!(f.write(tp.name.as_bytes()));

                if tp.bounds.len() > 0 {
                    try!(f.write(": ".as_bytes()));
                    for (i, bound) in tp.bounds.iter().enumerate() {
                        if i > 0 {
                            try!(f.write(" + ".as_bytes()));
                        }
                        try!(write!(f, "{}", *bound));
                    }
                }
            }
        }
        try!(f.write("&gt;".as_bytes()));
        Ok(())
    }
}

impl fmt::Show for clean::Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(f.write("'".as_bytes()));
        try!(f.write(self.get_ref().as_bytes()));
        Ok(())
    }
}

impl fmt::Show for clean::TyParamBound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::RegionBound => {
                f.write("::".as_bytes())
            }
            clean::TraitBound(ref ty) => {
                write!(f, "{}", *ty)
            }
        }
    }
}

impl fmt::Show for clean::Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.global {
            try!(f.write("::".as_bytes()))
        }

        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                try!(f.write("::".as_bytes()))
            }
            try!(f.write(seg.name.as_bytes()));

            if seg.lifetimes.len() > 0 || seg.types.len() > 0 {
                try!(f.write("&lt;".as_bytes()));
                let mut comma = false;
                for lifetime in seg.lifetimes.iter() {
                    if comma {
                        try!(f.write(", ".as_bytes()));
                    }
                    comma = true;
                    try!(write!(f, "{}", *lifetime));
                }
                for ty in seg.types.iter() {
                    if comma {
                        try!(f.write(", ".as_bytes()));
                    }
                    comma = true;
                    try!(write!(f, "{}", *ty));
                }
                try!(f.write("&gt;".as_bytes()));
            }
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
            if ast_util::is_local(did) {
                Some(("../".repeat(loc.len())).to_strbuf())
            } else {
                match *cache.extern_locations.get(&did.krate) {
                    render::Remote(ref s) => Some(s.to_strbuf()),
                    render::Local => {
                        Some(("../".repeat(loc.len())).to_strbuf())
                    }
                    render::Unknown => None,
                }
            }
        },
        |cache| {
            match cache.paths.find(&did) {
                None => None,
                Some(&(ref fqp, shortty)) => Some((fqp.clone(), shortty))
            }
        })
}

fn path(w: &mut fmt::Formatter, path: &clean::Path, print_all: bool,
        root: |&render::Cache, &[String]| -> Option<String>,
        info: |&render::Cache| -> Option<(Vec<String> , ItemType)>)
    -> fmt::Result
{
    // The generics will get written to both the title and link
    let mut generics = String::new();
    let last = path.segments.last().unwrap();
    if last.lifetimes.len() > 0 || last.types.len() > 0 {
        let mut counter = 0;
        generics.push_str("&lt;");
        for lifetime in last.lifetimes.iter() {
            if counter > 0 { generics.push_str(", "); }
            counter += 1;
            generics.push_str(format!("{}", *lifetime).as_slice());
        }
        for ty in last.types.iter() {
            if counter > 0 { generics.push_str(", "); }
            counter += 1;
            generics.push_str(format!("{}", *ty).as_slice());
        }
        generics.push_str("&gt;");
    }

    let loc = current_location_key.get().unwrap();
    let cache = cache_key.get().unwrap();
    let abs_root = root(&**cache, loc.as_slice());
    let rel_root = match path.segments.get(0).name.as_slice() {
        "self" => Some("./".to_owned()),
        _ => None,
    };

    if print_all {
        let amt = path.segments.len() - 1;
        match rel_root {
            Some(root) => {
                let mut root = String::from_str(root.as_slice());
                for seg in path.segments.slice_to(amt).iter() {
                    if "super" == seg.name.as_slice() ||
                            "self" == seg.name.as_slice() {
                        try!(write!(w, "{}::", seg.name));
                    } else {
                        root.push_str(seg.name.as_slice());
                        root.push_str("/");
                        try!(write!(w, "<a class='mod'
                                            href='{}index.html'>{}</a>::",
                                      root.as_slice(),
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

    match info(&**cache) {
        // This is a documented path, link to it!
        Some((ref fqp, shortty)) if abs_root.is_some() => {
            let mut url = String::from_str(abs_root.unwrap().as_slice());
            let to_link = fqp.slice_to(fqp.len() - 1);
            for component in to_link.iter() {
                url.push_str(component.as_slice());
                url.push_str("/");
            }
            match shortty {
                item_type::Module => {
                    url.push_str(fqp.last().unwrap().as_slice());
                    url.push_str("/index.html");
                }
                _ => {
                    url.push_str(shortty.to_static_str());
                    url.push_str(".");
                    url.push_str(fqp.last().unwrap().as_slice());
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
    try!(write!(w, "{}", generics.as_slice()));
    Ok(())
}

/// Helper to render type parameters
fn tybounds(w: &mut fmt::Formatter,
            typarams: &Option<Vec<clean::TyParamBound> >) -> fmt::Result {
    match *typarams {
        Some(ref params) => {
            try!(write!(w, ":"));
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    try!(write!(w, " + "));
                }
                try!(write!(w, "{}", *param));
            }
            Ok(())
        }
        None => Ok(())
    }
}

impl fmt::Show for clean::Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::TyParamBinder(id) => {
                let m = cache_key.get().unwrap();
                f.write(m.typarams.get(&ast_util::local_def(id)).as_bytes())
            }
            clean::Generic(did) => {
                let m = cache_key.get().unwrap();
                f.write(m.typarams.get(&did).as_bytes())
            }
            clean::ResolvedPath{ did, ref typarams, ref path } => {
                try!(resolved_path(f, did, path, false));
                tybounds(f, typarams)
            }
            clean::Self(..) => f.write("Self".as_bytes()),
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
                    ast::TyFloat(ast::TyF128) => "f128",
                    ast::TyStr => "str",
                    ast::TyBool => "bool",
                    ast::TyChar => "char",
                };
                f.write(s.as_bytes())
            }
            clean::Closure(ref decl, ref region) => {
                write!(f, "{style}{lifetimes}|{args}|{bounds}\
                           {arrow, select, yes{ -&gt; {ret}} other{}}",
                       style = FnStyleSpace(decl.fn_style),
                       lifetimes = if decl.lifetimes.len() == 0 {
                           "".to_strbuf()
                       } else {
                           format!("&lt;{:#}&gt;", decl.lifetimes)
                       },
                       args = decl.decl.inputs,
                       arrow = match decl.decl.output {
                           clean::Unit => "no",
                           _ => "yes",
                       },
                       ret = decl.decl.output,
                       bounds = {
                           let mut ret = String::new();
                           match *region {
                               Some(ref lt) => {
                                   ret.push_str(format!(": {}",
                                                        *lt).as_slice());
                               }
                               None => {}
                           }
                           for bound in decl.bounds.iter() {
                                match *bound {
                                    clean::RegionBound => {}
                                    clean::TraitBound(ref t) => {
                                        if ret.len() == 0 {
                                            ret.push_str(": ");
                                        } else {
                                            ret.push_str(" + ");
                                        }
                                        ret.push_str(format!("{}",
                                                             *t).as_slice());
                                    }
                                }
                           }
                           ret.into_owned()
                       })
            }
            clean::Proc(ref decl) => {
                write!(f, "{style}{lifetimes}proc({args}){bounds}\
                           {arrow, select, yes{ -&gt; {ret}} other{}}",
                       style = FnStyleSpace(decl.fn_style),
                       lifetimes = if decl.lifetimes.len() == 0 {
                           "".to_strbuf()
                       } else {
                           format_strbuf!("&lt;{:#}&gt;", decl.lifetimes)
                       },
                       args = decl.decl.inputs,
                       bounds = if decl.bounds.len() == 0 {
                           "".to_strbuf()
                       } else {
                           let mut m = decl.bounds
                                           .iter()
                                           .map(|s| s.to_str().to_strbuf());
                           format_strbuf!(
                               ": {}",
                               m.collect::<Vec<String>>().connect(" + "))
                       },
                       arrow = match decl.decl.output { clean::Unit => "no", _ => "yes" },
                       ret = decl.decl.output)
            }
            clean::BareFunction(ref decl) => {
                write!(f, "{}{}fn{}{}",
                       FnStyleSpace(decl.fn_style),
                       match decl.abi.as_slice() {
                           "" => " extern ".to_strbuf(),
                           "\"Rust\"" => "".to_strbuf(),
                           s => format_strbuf!(" extern {} ", s)
                       },
                       decl.generics,
                       decl.decl)
            }
            clean::Tuple(ref typs) => {
                try!(f.write("(".as_bytes()));
                for (i, typ) in typs.iter().enumerate() {
                    if i > 0 {
                        try!(f.write(", ".as_bytes()))
                    }
                    try!(write!(f, "{}", *typ));
                }
                f.write(")".as_bytes())
            }
            clean::Vector(ref t) => write!(f, "[{}]", **t),
            clean::FixedVector(ref t, ref s) => {
                write!(f, "[{}, ..{}]", **t, *s)
            }
            clean::String => f.write("str".as_bytes()),
            clean::Bool => f.write("bool".as_bytes()),
            clean::Unit => f.write("()".as_bytes()),
            clean::Bottom => f.write("!".as_bytes()),
            clean::Unique(ref t) => write!(f, "~{}", **t),
            clean::Managed(ref t) => write!(f, "@{}", **t),
            clean::RawPointer(m, ref t) => {
                write!(f, "*{}{}",
                       match m {
                           clean::Mutable => "mut ",
                           clean::Immutable => "",
                       }, **t)
            }
            clean::BorrowedRef{ lifetime: ref l, mutability, type_: ref ty} => {
                let lt = match *l {
                    Some(ref l) => format!("{} ", *l),
                    _ => "".to_strbuf(),
                };
                write!(f, "&amp;{}{}{}",
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
            if i > 0 { try!(write!(f, ", ")); }
            if input.name.len() > 0 {
                try!(write!(f, "{}: ", input.name));
            }
            try!(write!(f, "{}", input.type_));
        }
        Ok(())
    }
}

impl fmt::Show for clean::FnDecl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({args}){arrow, select, yes{ -&gt; {ret}} other{}}",
               args = self.inputs,
               arrow = match self.output { clean::Unit => "no", _ => "yes" },
               ret = self.output)
    }
}

impl<'a> fmt::Show for Method<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Method(selfty, d) = *self;
        let mut args = String::new();
        match *selfty {
            clean::SelfStatic => {},
            clean::SelfValue => args.push_str("self"),
            clean::SelfOwned => args.push_str("~self"),
            clean::SelfBorrowed(Some(ref lt), clean::Immutable) => {
                args.push_str(format!("&amp;{} self", *lt).as_slice());
            }
            clean::SelfBorrowed(Some(ref lt), clean::Mutable) => {
                args.push_str(format!("&amp;{} mut self", *lt).as_slice());
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
                args.push_str(format!("{}: ", input.name).as_slice());
            }
            args.push_str(format!("{}", input.type_).as_slice());
        }
        write!(f,
               "({args}){arrow, select, yes{ -&gt; {ret}} other{}}",
               args = args,
               arrow = match d.output { clean::Unit => "no", _ => "yes" },
               ret = d.output)
    }
}

impl fmt::Show for VisSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            Some(ast::Public) => write!(f, "pub "),
            Some(ast::Inherited) | None => Ok(())
        }
    }
}

impl fmt::Show for FnStyleSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            ast::UnsafeFn => write!(f, "unsafe "),
            ast::NormalFn => Ok(())
        }
    }
}

impl fmt::Show for clean::ViewPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            clean::SimpleImport(ref name, ref src) => {
                if *name == src.path.segments.last().unwrap().name {
                    write!(f, "use {};", *src)
                } else {
                    write!(f, "use {} = {};", *name, *src)
                }
            }
            clean::GlobImport(ref src) => {
                write!(f, "use {}::*;", *src)
            }
            clean::ImportList(ref src, ref names) => {
                try!(write!(f, "use {}::\\{", *src));
                for (i, n) in names.iter().enumerate() {
                    if i > 0 {
                        try!(write!(f, ", "));
                    }
                    try!(write!(f, "{}", *n));
                }
                write!(f, "\\};")
            }
        }
    }
}

impl fmt::Show for clean::ImportSource {
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

impl fmt::Show for clean::ViewListIdent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.source {
            Some(did) => {
                let path = clean::Path {
                    global: false,
                    segments: vec!(clean::PathSegment {
                        name: self.name.clone(),
                        lifetimes: Vec::new(),
                        types: Vec::new(),
                    })
                };
                resolved_path(f, did, &path, false)
            }
            _ => write!(f, "{}", self.name),
        }
    }
}
