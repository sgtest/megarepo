// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast;
use codemap::{Span, respan};
use ext::base::*;
use ext::base;
use ext::build::AstBuilder;
use rsparse = parse;
use parse::token;

use std::fmt::parse;
use std::hashmap::{HashMap, HashSet};
use std::vec;

#[deriving(Eq)]
enum ArgumentType {
    Unknown,
    Known(@str),
    Unsigned,
    String,
}

struct Context {
    ecx: @ExtCtxt,
    fmtsp: Span,

    // Parsed argument expressions and the types that we've found so far for
    // them.
    args: ~[@ast::Expr],
    arg_types: ~[Option<ArgumentType>],
    // Parsed named expressions and the types that we've found for them so far
    names: HashMap<@str, @ast::Expr>,
    name_types: HashMap<@str, ArgumentType>,

    // Collection of the compiled `rt::Piece` structures
    pieces: ~[@ast::Expr],
    name_positions: HashMap<@str, uint>,
    method_statics: ~[@ast::item],

    // Updated as arguments are consumed or methods are entered
    nest_level: uint,
    next_arg: uint,
}

impl Context {
    /// Parses the arguments from the given list of tokens, returning None if
    /// there's a parse error so we can continue parsing other fmt! expressions.
    fn parse_args(&mut self, sp: Span,
                  tts: &[ast::token_tree]) -> (@ast::Expr, Option<@ast::Expr>) {
        let p = rsparse::new_parser_from_tts(self.ecx.parse_sess(),
                                             self.ecx.cfg(),
                                             tts.to_owned());
        // Parse the leading function expression (maybe a block, maybe a path)
        let extra = p.parse_expr();
        if !p.eat(&token::COMMA) {
            self.ecx.span_err(sp, "expected token: `,`");
            return (extra, None);
        }

        if *p.token == token::EOF {
            self.ecx.span_err(sp, "requires at least a format string argument");
            return (extra, None);
        }
        let fmtstr = p.parse_expr();
        let mut named = false;
        while *p.token != token::EOF {
            if !p.eat(&token::COMMA) {
                self.ecx.span_err(sp, "expected token: `,`");
                return (extra, None);
            }
            if *p.token == token::EOF { break } // accept trailing commas
            if named || (token::is_ident(p.token) &&
                         p.look_ahead(1, |t| *t == token::EQ)) {
                named = true;
                let ident = match *p.token {
                    token::IDENT(i, _) => {
                        p.bump();
                        i
                    }
                    _ if named => {
                        self.ecx.span_err(*p.span,
                                          "expected ident, positional arguments \
                                           cannot follow named arguments");
                        return (extra, None);
                    }
                    _ => {
                        self.ecx.span_err(*p.span,
                                          fmt!("expected ident for named \
                                                argument, but found `%s`",
                                               p.this_token_to_str()));
                        return (extra, None);
                    }
                };
                let name = self.ecx.str_of(ident);
                p.expect(&token::EQ);
                let e = p.parse_expr();
                match self.names.find(&name) {
                    None => {}
                    Some(prev) => {
                        self.ecx.span_err(e.span, fmt!("duplicate argument \
                                                        named `%s`", name));
                        self.ecx.parse_sess.span_diagnostic.span_note(
                            prev.span, "previously here");
                        loop
                    }
                }
                self.names.insert(name, e);
            } else {
                self.args.push(p.parse_expr());
                self.arg_types.push(None);
            }
        }
        return (extra, Some(fmtstr));
    }

    /// Verifies one piece of a parse string. All errors are not emitted as
    /// fatal so we can continue giving errors about this and possibly other
    /// format strings.
    fn verify_piece(&mut self, p: &parse::Piece) {
        match *p {
            parse::String(*) => {}
            parse::CurrentArgument => {
                if self.nest_level == 0 {
                    self.ecx.span_err(self.fmtsp,
                                      "`#` reference used with nothing to \
                                       reference back to");
                }
            }
            parse::Argument(ref arg) => {
                // width/precision first, if they have implicit positional
                // parameters it makes more sense to consume them first.
                self.verify_count(arg.format.width);
                self.verify_count(arg.format.precision);

                // argument second, if it's an implicit positional parameter
                // it's written second, so it should come after width/precision.
                let pos = match arg.position {
                    parse::ArgumentNext => {
                        let i = self.next_arg;
                        if self.check_positional_ok() {
                            self.next_arg += 1;
                        }
                        Left(i)
                    }
                    parse::ArgumentIs(i) => Left(i),
                    parse::ArgumentNamed(s) => Right(s.to_managed()),
                };
                let ty = if arg.format.ty == "" {
                    Unknown
                } else { Known(arg.format.ty.to_managed()) };
                self.verify_arg_type(pos, ty);

                // and finally the method being applied
                match arg.method {
                    None => {}
                    Some(ref method) => { self.verify_method(pos, *method); }
                }
            }
        }
    }

    fn verify_pieces(&mut self, pieces: &[parse::Piece]) {
        for piece in pieces.iter() {
            self.verify_piece(piece);
        }
    }

    fn verify_count(&mut self, c: parse::Count) {
        match c {
            parse::CountImplied | parse::CountIs(*) => {}
            parse::CountIsParam(i) => {
                self.verify_arg_type(Left(i), Unsigned);
            }
            parse::CountIsNextParam => {
                if self.check_positional_ok() {
                    self.verify_arg_type(Left(self.next_arg), Unsigned);
                    self.next_arg += 1;
                }
            }
        }
    }

    fn check_positional_ok(&mut self) -> bool {
        if self.nest_level != 0 {
            self.ecx.span_err(self.fmtsp, "cannot use implicit positional \
                                           arguments nested inside methods");
            false
        } else {
            true
        }
    }

    fn verify_method(&mut self, pos: Either<uint, @str>, m: &parse::Method) {
        self.nest_level += 1;
        match *m {
            parse::Plural(_, ref arms, ref default) => {
                let mut seen_cases = HashSet::new();
                self.verify_arg_type(pos, Unsigned);
                for arm in arms.iter() {
                    if !seen_cases.insert(arm.selector) {
                        match arm.selector {
                            Left(name) => {
                                self.ecx.span_err(self.fmtsp,
                                                  fmt!("duplicate selector \
                                                       `%?`", name));
                            }
                            Right(idx) => {
                                self.ecx.span_err(self.fmtsp,
                                                  fmt!("duplicate selector \
                                                       `=%u`", idx));
                            }
                        }
                    }
                    self.verify_pieces(arm.result);
                }
                self.verify_pieces(*default);
            }
            parse::Select(ref arms, ref default) => {
                self.verify_arg_type(pos, String);
                let mut seen_cases = HashSet::new();
                for arm in arms.iter() {
                    if !seen_cases.insert(arm.selector) {
                        self.ecx.span_err(self.fmtsp,
                                          fmt!("duplicate selector `%s`",
                                               arm.selector));
                    } else if arm.selector == "" {
                        self.ecx.span_err(self.fmtsp,
                                          "empty selector in `select`");
                    }
                    self.verify_pieces(arm.result);
                }
                self.verify_pieces(*default);
            }
        }
        self.nest_level -= 1;
    }

    fn verify_arg_type(&mut self, arg: Either<uint, @str>, ty: ArgumentType) {
        match arg {
            Left(arg) => {
                if arg < 0 || self.args.len() <= arg {
                    let msg = fmt!("invalid reference to argument `%u` (there \
                                    are %u arguments)", arg, self.args.len());
                    self.ecx.span_err(self.fmtsp, msg);
                    return;
                }
                self.verify_same(self.args[arg].span, ty, self.arg_types[arg]);
                if ty != Unknown || self.arg_types[arg].is_none() {
                    self.arg_types[arg] = Some(ty);
                }
            }

            Right(name) => {
                let span = match self.names.find(&name) {
                    Some(e) => e.span,
                    None => {
                        let msg = fmt!("there is no argument named `%s`", name);
                        self.ecx.span_err(self.fmtsp, msg);
                        return;
                    }
                };
                self.verify_same(span, ty,
                                 self.name_types.find(&name).map(|&x| *x));
                if ty != Unknown || !self.name_types.contains_key(&name) {
                    self.name_types.insert(name, ty);
                }
                // Assign this named argument a slot in the arguments array if
                // it hasn't already been assigned a slot.
                if !self.name_positions.contains_key(&name) {
                    let slot = self.name_positions.len();
                    self.name_positions.insert(name, slot);
                }
            }
        }
    }

    /// When we're keeping track of the types that are declared for certain
    /// arguments, we assume that `None` means we haven't seen this argument
    /// yet, `Some(None)` means that we've seen the argument, but no format was
    /// specified, and `Some(Some(x))` means that the argument was declared to
    /// have type `x`.
    ///
    /// Obviously `Some(Some(x)) != Some(Some(y))`, but we consider it true
    /// that: `Some(None) == Some(Some(x))`
    fn verify_same(&self, sp: Span, ty: ArgumentType,
                   before: Option<ArgumentType>) {
        if ty == Unknown { return }
        let cur = match before {
            Some(Unknown) | None => return,
            Some(t) => t,
        };
        if ty == cur { return }
        match (cur, ty) {
            (Known(cur), Known(ty)) => {
                self.ecx.span_err(sp,
                                  fmt!("argument redeclared with type `%s` when \
                                        it was previously `%s`", ty, cur));
            }
            (Known(cur), _) => {
                self.ecx.span_err(sp,
                                  fmt!("argument used to format with `%s` was \
                                        attempted to not be used for formatting",
                                        cur));
            }
            (_, Known(ty)) => {
                self.ecx.span_err(sp,
                                  fmt!("argument previously used as a format \
                                        argument attempted to be used as `%s`",
                                        ty));
            }
            (_, _) => {
                self.ecx.span_err(sp, "argument declared with multiple formats");
            }
        }
    }

    /// Translate a `parse::Piece` to a static `rt::Piece`
    fn trans_piece(&mut self, piece: &parse::Piece) -> @ast::Expr {
        let sp = self.fmtsp;
        let parsepath = |s: &str| {
            ~[self.ecx.ident_of("std"), self.ecx.ident_of("fmt"),
              self.ecx.ident_of("parse"), self.ecx.ident_of(s)]
        };
        let rtpath = |s: &str| {
            ~[self.ecx.ident_of("std"), self.ecx.ident_of("fmt"),
              self.ecx.ident_of("rt"), self.ecx.ident_of(s)]
        };
        let ctpath = |s: &str| {
            ~[self.ecx.ident_of("std"), self.ecx.ident_of("fmt"),
              self.ecx.ident_of("parse"), self.ecx.ident_of(s)]
        };
        let none = self.ecx.path_global(sp, ~[
                self.ecx.ident_of("std"),
                self.ecx.ident_of("option"),
                self.ecx.ident_of("None")]);
        let none = self.ecx.expr_path(none);
        let some = |e: @ast::Expr| {
            let p = self.ecx.path_global(sp, ~[
                self.ecx.ident_of("std"),
                self.ecx.ident_of("option"),
                self.ecx.ident_of("Some")]);
            let p = self.ecx.expr_path(p);
            self.ecx.expr_call(sp, p, ~[e])
        };
        let trans_count = |c: parse::Count| {
            match c {
                parse::CountIs(i) => {
                    self.ecx.expr_call_global(sp, ctpath("CountIs"),
                                              ~[self.ecx.expr_uint(sp, i)])
                }
                parse::CountIsParam(i) => {
                    self.ecx.expr_call_global(sp, ctpath("CountIsParam"),
                                              ~[self.ecx.expr_uint(sp, i)])
                }
                parse::CountImplied => {
                    let path = self.ecx.path_global(sp, ctpath("CountImplied"));
                    self.ecx.expr_path(path)
                }
                parse::CountIsNextParam => {
                    let path = self.ecx.path_global(sp, ctpath("CountIsNextParam"));
                    self.ecx.expr_path(path)
                }
            }
        };
        let trans_method = |method: &parse::Method| {
            let method = match *method {
                parse::Select(ref arms, ref default) => {
                    let arms = arms.iter().map(|arm| {
                        let p = self.ecx.path_global(sp, rtpath("SelectArm"));
                        let result = arm.result.iter().map(|p| {
                            self.trans_piece(p)
                        }).collect();
                        let s = arm.selector.to_managed();
                        let selector = self.ecx.expr_str(sp, s);
                        self.ecx.expr_struct(sp, p, ~[
                            self.ecx.field_imm(sp,
                                               self.ecx.ident_of("selector"),
                                               selector),
                            self.ecx.field_imm(sp, self.ecx.ident_of("result"),
                                               self.ecx.expr_vec_slice(sp, result)),
                        ])
                    }).collect();
                    let default = default.iter().map(|p| {
                        self.trans_piece(p)
                    }).collect();
                    self.ecx.expr_call_global(sp, rtpath("Select"), ~[
                        self.ecx.expr_vec_slice(sp, arms),
                        self.ecx.expr_vec_slice(sp, default),
                    ])
                }
                parse::Plural(offset, ref arms, ref default) => {
                    let offset = match offset {
                        Some(i) => { some(self.ecx.expr_uint(sp, i)) }
                        None => { none.clone() }
                    };
                    let arms = arms.iter().map(|arm| {
                        let p = self.ecx.path_global(sp, rtpath("PluralArm"));
                        let result = arm.result.iter().map(|p| {
                            self.trans_piece(p)
                        }).collect();
                        let (lr, selarg) = match arm.selector {
                            Left(t) => {
                                let p = ctpath(fmt!("%?", t));
                                let p = self.ecx.path_global(sp, p);
                                (self.ecx.ident_of("Left"),
                                 self.ecx.expr_path(p))
                            }
                            Right(i) => {
                                (self.ecx.ident_of("Right"),
                                 self.ecx.expr_uint(sp, i))
                            }
                        };
                        let selector = self.ecx.expr_call_ident(sp,
                                lr, ~[selarg]);
                        self.ecx.expr_struct(sp, p, ~[
                            self.ecx.field_imm(sp,
                                               self.ecx.ident_of("selector"),
                                               selector),
                            self.ecx.field_imm(sp, self.ecx.ident_of("result"),
                                               self.ecx.expr_vec_slice(sp, result)),
                        ])
                    }).collect();
                    let default = default.iter().map(|p| {
                        self.trans_piece(p)
                    }).collect();
                    self.ecx.expr_call_global(sp, rtpath("Plural"), ~[
                        offset,
                        self.ecx.expr_vec_slice(sp, arms),
                        self.ecx.expr_vec_slice(sp, default),
                    ])
                }
            };
            let life = self.ecx.lifetime(sp, self.ecx.ident_of("static"));
            let ty = self.ecx.ty_path(self.ecx.path_all(
                sp,
                true,
                rtpath("Method"),
                Some(life),
                ~[]
            ), None);
            let st = ast::item_static(ty, ast::MutImmutable, method);
            let static_name = self.ecx.ident_of(fmt!("__static_method_%u",
                                                     self.method_statics.len()));
            // Flag these statics as `address_insignificant` so LLVM can
            // merge duplicate globals as much as possible (which we're
            // generating a whole lot of).
            let unnamed = self.ecx.meta_word(self.fmtsp, @"address_insignificant");
            let unnamed = self.ecx.attribute(self.fmtsp, unnamed);
            let item = self.ecx.item(sp, static_name, ~[unnamed], st);
            self.method_statics.push(item);
            self.ecx.expr_ident(sp, static_name)
        };

        match *piece {
            parse::String(s) => {
                self.ecx.expr_call_global(sp, rtpath("String"),
                                          ~[self.ecx.expr_str(sp, s.to_managed())])
            }
            parse::CurrentArgument => {
                let nil = self.ecx.expr_lit(sp, ast::lit_nil);
                self.ecx.expr_call_global(sp, rtpath("CurrentArgument"), ~[nil])
            }
            parse::Argument(ref arg) => {
                // Translate the position
                let pos = match arg.position {
                    // These two have a direct mapping
                    parse::ArgumentNext => {
                        let path = self.ecx.path_global(sp,
                                                        rtpath("ArgumentNext"));
                        self.ecx.expr_path(path)
                    }
                    parse::ArgumentIs(i) => {
                        self.ecx.expr_call_global(sp, rtpath("ArgumentIs"),
                                                  ~[self.ecx.expr_uint(sp, i)])
                    }
                    // Named arguments are converted to positional arguments at
                    // the end of the list of arguments
                    parse::ArgumentNamed(n) => {
                        let n = n.to_managed();
                        let i = match self.name_positions.find_copy(&n) {
                            Some(i) => i,
                            None => 0, // error already emitted elsewhere
                        };
                        let i = i + self.args.len();
                        self.ecx.expr_call_global(sp, rtpath("ArgumentIs"),
                                                  ~[self.ecx.expr_uint(sp, i)])
                    }
                };

                // Translate the format
                let fill = match arg.format.fill { Some(c) => c, None => ' ' };
                let fill = self.ecx.expr_lit(sp, ast::lit_char(fill as u32));
                let align = match arg.format.align {
                    parse::AlignLeft => {
                        self.ecx.path_global(sp, parsepath("AlignLeft"))
                    }
                    parse::AlignRight => {
                        self.ecx.path_global(sp, parsepath("AlignRight"))
                    }
                    parse::AlignUnknown => {
                        self.ecx.path_global(sp, parsepath("AlignUnknown"))
                    }
                };
                let align = self.ecx.expr_path(align);
                let flags = self.ecx.expr_uint(sp, arg.format.flags);
                let prec = trans_count(arg.format.precision);
                let width = trans_count(arg.format.width);
                let path = self.ecx.path_global(sp, rtpath("FormatSpec"));
                let fmt = self.ecx.expr_struct(sp, path, ~[
                    self.ecx.field_imm(sp, self.ecx.ident_of("fill"), fill),
                    self.ecx.field_imm(sp, self.ecx.ident_of("align"), align),
                    self.ecx.field_imm(sp, self.ecx.ident_of("flags"), flags),
                    self.ecx.field_imm(sp, self.ecx.ident_of("precision"), prec),
                    self.ecx.field_imm(sp, self.ecx.ident_of("width"), width),
                ]);

                // Translate the method (if any)
                let method = match arg.method {
                    None => { none.clone() }
                    Some(ref m) => {
                        let m = trans_method(*m);
                        some(self.ecx.expr_addr_of(sp, m))
                    }
                };
                let path = self.ecx.path_global(sp, rtpath("Argument"));
                let s = self.ecx.expr_struct(sp, path, ~[
                    self.ecx.field_imm(sp, self.ecx.ident_of("position"), pos),
                    self.ecx.field_imm(sp, self.ecx.ident_of("format"), fmt),
                    self.ecx.field_imm(sp, self.ecx.ident_of("method"), method),
                ]);
                self.ecx.expr_call_global(sp, rtpath("Argument"), ~[s])
            }
        }
    }

    /// Actually builds the expression which the ifmt! block will be expanded
    /// to
    fn to_expr(&self, extra: @ast::Expr) -> @ast::Expr {
        let mut lets = ~[];
        let mut locals = ~[];
        let mut names = vec::from_fn(self.name_positions.len(), |_| None);

        // First, declare all of our methods that are statics
        for &method in self.method_statics.iter() {
            let decl = respan(self.fmtsp, ast::DeclItem(method));
            lets.push(@respan(self.fmtsp,
                              ast::StmtDecl(@decl, ast::DUMMY_NODE_ID)));
        }

        // Next, build up the static array which will become our precompiled
        // format "string"
        let fmt = self.ecx.expr_vec(self.fmtsp, self.pieces.clone());
        let piece_ty = self.ecx.ty_path(self.ecx.path_all(
                self.fmtsp,
                true, ~[
                    self.ecx.ident_of("std"),
                    self.ecx.ident_of("fmt"),
                    self.ecx.ident_of("rt"),
                    self.ecx.ident_of("Piece"),
                ],
                Some(self.ecx.lifetime(self.fmtsp, self.ecx.ident_of("static"))),
                ~[]
            ), None);
        let ty = ast::ty_fixed_length_vec(
            self.ecx.ty_mt(piece_ty.clone(), ast::MutImmutable),
            self.ecx.expr_uint(self.fmtsp, self.pieces.len())
        );
        let ty = self.ecx.ty(self.fmtsp, ty);
        let st = ast::item_static(ty, ast::MutImmutable, fmt);
        let static_name = self.ecx.ident_of("__static_fmtstr");
        // see above comment for `address_insignificant` and why we do it
        let unnamed = self.ecx.meta_word(self.fmtsp, @"address_insignificant");
        let unnamed = self.ecx.attribute(self.fmtsp, unnamed);
        let item = self.ecx.item(self.fmtsp, static_name, ~[unnamed], st);
        let decl = respan(self.fmtsp, ast::DeclItem(item));
        lets.push(@respan(self.fmtsp, ast::StmtDecl(@decl, ast::DUMMY_NODE_ID)));

        // Right now there is a bug such that for the expression:
        //      foo(bar(&1))
        // the lifetime of `1` doesn't outlast the call to `bar`, so it's not
        // vald for the call to `foo`. To work around this all arguments to the
        // fmt! string are shoved into locals. Furthermore, we shove the address
        // of each variable because we don't want to move out of the arguments
        // passed to this function.
        for (i, &e) in self.args.iter().enumerate() {
            if self.arg_types[i].is_none() { loop } // error already generated

            let name = self.ecx.ident_of(fmt!("__arg%u", i));
            let e = self.ecx.expr_addr_of(e.span, e);
            lets.push(self.ecx.stmt_let(e.span, false, name, e));
            locals.push(self.format_arg(e.span, Left(i),
                                        self.ecx.expr_ident(e.span, name)));
        }
        for (&name, &e) in self.names.iter() {
            if !self.name_types.contains_key(&name) { loop }

            let lname = self.ecx.ident_of(fmt!("__arg%s", name));
            let e = self.ecx.expr_addr_of(e.span, e);
            lets.push(self.ecx.stmt_let(e.span, false, lname, e));
            names[*self.name_positions.get(&name)] =
                Some(self.format_arg(e.span, Right(name),
                                     self.ecx.expr_ident(e.span, lname)));
        }

        // Now create the fmt::Arguments struct with all our locals we created.
        let args = names.move_iter().map(|a| a.unwrap());
        let mut args = locals.move_iter().chain(args);
        let fmt = self.ecx.expr_ident(self.fmtsp, static_name);
        let args = self.ecx.expr_vec_slice(self.fmtsp, args.collect());
        let result = self.ecx.expr_call_global(self.fmtsp, ~[
                self.ecx.ident_of("std"),
                self.ecx.ident_of("fmt"),
                self.ecx.ident_of("Arguments"),
                self.ecx.ident_of("new"),
            ], ~[fmt, args]);

        // We did all the work of making sure that the arguments
        // structure is safe, so we can safely have an unsafe block.
        let result = self.ecx.expr_block(ast::Block {
           view_items: ~[],
           stmts: ~[],
           expr: Some(result),
           id: ast::DUMMY_NODE_ID,
           rules: ast::UnsafeBlock(ast::CompilerGenerated),
           span: self.fmtsp,
        });
        let resname = self.ecx.ident_of("__args");
        lets.push(self.ecx.stmt_let(self.fmtsp, false, resname, result));
        let res = self.ecx.expr_ident(self.fmtsp, resname);
        let result = self.ecx.expr_call(extra.span, extra, ~[
                            self.ecx.expr_addr_of(extra.span, res)]);
        self.ecx.expr_block(self.ecx.block(self.fmtsp, lets,
                                           Some(result)))
    }

    fn format_arg(&self, sp: Span, argno: Either<uint, @str>,
                  arg: @ast::Expr) -> @ast::Expr {
        let ty = match argno {
            Left(i) => self.arg_types[i].unwrap(),
            Right(s) => *self.name_types.get(&s)
        };

        let fmt_trait = match ty {
            Unknown => "Default",
            Known(tyname) => {
                match tyname.as_slice() {
                    "?" => "Poly",
                    "b" => "Bool",
                    "c" => "Char",
                    "d" | "i" => "Signed",
                    "f" => "Float",
                    "o" => "Octal",
                    "p" => "Pointer",
                    "s" => "String",
                    "t" => "Binary",
                    "u" => "Unsigned",
                    "x" => "LowerHex",
                    "X" => "UpperHex",
                    _ => {
                        self.ecx.span_err(sp, fmt!("unknown format trait \
                                                    `%s`", tyname));
                        "Dummy"
                    }
                }
            }
            String => {
                return self.ecx.expr_call_global(sp, ~[
                        self.ecx.ident_of("std"),
                        self.ecx.ident_of("fmt"),
                        self.ecx.ident_of("argumentstr"),
                    ], ~[arg])
            }
            Unsigned => {
                return self.ecx.expr_call_global(sp, ~[
                        self.ecx.ident_of("std"),
                        self.ecx.ident_of("fmt"),
                        self.ecx.ident_of("argumentuint"),
                    ], ~[arg])
            }
        };

        let format_fn = self.ecx.path_global(sp, ~[
                self.ecx.ident_of("std"),
                self.ecx.ident_of("fmt"),
                self.ecx.ident_of(fmt_trait),
                self.ecx.ident_of("fmt"),
            ]);
        self.ecx.expr_call_global(sp, ~[
                self.ecx.ident_of("std"),
                self.ecx.ident_of("fmt"),
                self.ecx.ident_of("argument"),
            ], ~[self.ecx.expr_path(format_fn), arg])
    }
}

pub fn expand_args(ecx: @ExtCtxt, sp: Span,
                   tts: &[ast::token_tree]) -> base::MacResult {
    let mut cx = Context {
        ecx: ecx,
        args: ~[],
        arg_types: ~[],
        names: HashMap::new(),
        name_positions: HashMap::new(),
        name_types: HashMap::new(),
        nest_level: 0,
        next_arg: 0,
        pieces: ~[],
        method_statics: ~[],
        fmtsp: sp,
    };
    let (extra, efmt) = match cx.parse_args(sp, tts) {
        (extra, Some(e)) => (extra, e),
        (_, None) => { return MRExpr(ecx.expr_uint(sp, 2)); }
    };
    cx.fmtsp = efmt.span;
    let fmt = expr_to_str(ecx, efmt,
                          "format argument must be a string literal.");

    let mut err = false;
    do parse::parse_error::cond.trap(|m| {
        if !err {
            err = true;
            ecx.span_err(efmt.span, m);
        }
    }).inside {
        for piece in parse::Parser::new(fmt) {
            if !err {
                cx.verify_piece(&piece);
                let piece = cx.trans_piece(&piece);
                cx.pieces.push(piece);
            }
        }
    }
    if err { return MRExpr(efmt) }

    // Make sure that all arguments were used and all arguments have types.
    for (i, ty) in cx.arg_types.iter().enumerate() {
        if ty.is_none() {
            ecx.span_err(cx.args[i].span, "argument never used");
        }
    }
    for (name, e) in cx.names.iter() {
        if !cx.name_types.contains_key(name) {
            ecx.span_err(e.span, "named argument never used");
        }
    }

    MRExpr(cx.to_expr(extra))
}
