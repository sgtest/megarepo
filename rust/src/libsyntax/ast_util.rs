// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast::*;
use ast;
use ast_util;
use codemap::{span, spanned};
use parse::token;
use visit;
use opt_vec;

use core::to_bytes;

pub fn path_name_i(idents: &[ident], intr: @token::ident_interner) -> ~str {
    // FIXME: Bad copies (#2543 -- same for everything else that says "bad")
    str::connect(idents.map(|i| copy *intr.get(*i)), ~"::")
}


pub fn path_to_ident(p: @Path) -> ident { copy *p.idents.last() }

pub fn local_def(id: node_id) -> def_id {
    ast::def_id { crate: local_crate, node: id }
}

pub fn is_local(did: ast::def_id) -> bool { did.crate == local_crate }

pub fn stmt_id(s: &stmt) -> node_id {
    match s.node {
      stmt_decl(_, id) => id,
      stmt_expr(_, id) => id,
      stmt_semi(_, id) => id,
      stmt_mac(*) => fail!(~"attempted to analyze unexpanded stmt")
    }
}

pub fn variant_def_ids(d: def) -> Option<(def_id, def_id)> {
    match d {
      def_variant(enum_id, var_id) => {
          Some((enum_id, var_id))
      }
      _ => None
    }
}

pub fn def_id_of_def(d: def) -> def_id {
    match d {
      def_fn(id, _) | def_static_method(id, _, _) | def_mod(id) |
      def_foreign_mod(id) | def_const(id) |
      def_variant(_, id) | def_ty(id) | def_ty_param(id, _) |
      def_use(id) | def_struct(id) | def_trait(id) => {
        id
      }
      def_arg(id, _) | def_local(id, _) | def_self(id, _) | def_self_ty(id)
      | def_upvar(id, _, _, _) | def_binding(id, _) | def_region(id)
      | def_typaram_binder(id) | def_label(id) => {
        local_def(id)
      }

      def_prim_ty(_) => fail!()
    }
}

pub fn binop_to_str(op: binop) -> ~str {
    match op {
      add => return ~"+",
      subtract => return ~"-",
      mul => return ~"*",
      div => return ~"/",
      rem => return ~"%",
      and => return ~"&&",
      or => return ~"||",
      bitxor => return ~"^",
      bitand => return ~"&",
      bitor => return ~"|",
      shl => return ~"<<",
      shr => return ~">>",
      eq => return ~"==",
      lt => return ~"<",
      le => return ~"<=",
      ne => return ~"!=",
      ge => return ~">=",
      gt => return ~">"
    }
}

pub fn binop_to_method_name(op: binop) -> Option<~str> {
    match op {
      add => return Some(~"add"),
      subtract => return Some(~"sub"),
      mul => return Some(~"mul"),
      div => return Some(~"div"),
      rem => return Some(~"rem"),
      bitxor => return Some(~"bitxor"),
      bitand => return Some(~"bitand"),
      bitor => return Some(~"bitor"),
      shl => return Some(~"shl"),
      shr => return Some(~"shr"),
      lt => return Some(~"lt"),
      le => return Some(~"le"),
      ge => return Some(~"ge"),
      gt => return Some(~"gt"),
      eq => return Some(~"eq"),
      ne => return Some(~"ne"),
      and | or => return None
    }
}

pub fn lazy_binop(b: binop) -> bool {
    match b {
      and => true,
      or => true,
      _ => false
    }
}

pub fn is_shift_binop(b: binop) -> bool {
    match b {
      shl => true,
      shr => true,
      _ => false
    }
}

pub fn unop_to_str(op: unop) -> ~str {
    match op {
      box(mt) => if mt == m_mutbl { ~"@mut " } else { ~"@" },
      uniq(mt) => if mt == m_mutbl { ~"~mut " } else { ~"~" },
      deref => ~"*",
      not => ~"!",
      neg => ~"-"
    }
}

pub fn is_path(e: @expr) -> bool {
    return match e.node { expr_path(_) => true, _ => false };
}

pub fn int_ty_to_str(t: int_ty) -> ~str {
    match t {
      ty_char => ~"u8", // ???
      ty_i => ~"",
      ty_i8 => ~"i8",
      ty_i16 => ~"i16",
      ty_i32 => ~"i32",
      ty_i64 => ~"i64"
    }
}

pub fn int_ty_max(t: int_ty) -> u64 {
    match t {
      ty_i8 => 0x80u64,
      ty_i16 => 0x8000u64,
      ty_i | ty_char | ty_i32 => 0x80000000u64, // actually ni about ty_i
      ty_i64 => 0x8000000000000000u64
    }
}

pub fn uint_ty_to_str(t: uint_ty) -> ~str {
    match t {
      ty_u => ~"u",
      ty_u8 => ~"u8",
      ty_u16 => ~"u16",
      ty_u32 => ~"u32",
      ty_u64 => ~"u64"
    }
}

pub fn uint_ty_max(t: uint_ty) -> u64 {
    match t {
      ty_u8 => 0xffu64,
      ty_u16 => 0xffffu64,
      ty_u | ty_u32 => 0xffffffffu64, // actually ni about ty_u
      ty_u64 => 0xffffffffffffffffu64
    }
}

pub fn float_ty_to_str(t: float_ty) -> ~str {
    match t { ty_f => ~"f", ty_f32 => ~"f32", ty_f64 => ~"f64" }
}

pub fn is_call_expr(e: @expr) -> bool {
    match e.node { expr_call(_, _, _) => true, _ => false }
}

// This makes def_id hashable
impl to_bytes::IterBytes for def_id {
    #[inline(always)]
    fn iter_bytes(&self, lsb0: bool, f: to_bytes::Cb) {
        to_bytes::iter_bytes_2(&self.crate, &self.node, lsb0, f);
    }
}

pub fn block_from_expr(e: @expr) -> blk {
    let blk_ = default_block(~[], option::Some::<@expr>(e), e.id);
    return spanned {node: blk_, span: e.span};
}

pub fn default_block(
    stmts1: ~[@stmt],
    expr1: Option<@expr>,
    id1: node_id
) -> blk_ {
    ast::blk_ {
        view_items: ~[],
        stmts: stmts1,
        expr: expr1,
        id: id1,
        rules: default_blk,
    }
}

pub fn ident_to_path(s: span, i: ident) -> @Path {
    @ast::Path { span: s,
                 global: false,
                 idents: ~[i],
                 rp: None,
                 types: ~[] }
}

pub fn ident_to_pat(id: node_id, s: span, i: ident) -> @pat {
    @ast::pat { id: id,
                node: pat_ident(bind_by_copy, ident_to_path(s, i), None),
                span: s }
}

pub fn is_unguarded(a: &arm) -> bool {
    match a.guard {
      None => true,
      _    => false
    }
}

pub fn unguarded_pat(a: &arm) -> Option<~[@pat]> {
    if is_unguarded(a) { Some(/* FIXME (#2543) */ copy a.pats) } else { None }
}

pub fn public_methods(ms: ~[@method]) -> ~[@method] {
    do ms.filtered |m| {
        match m.vis {
            public => true,
            _   => false
        }
    }
}

// extract a ty_method from a trait_method. if the trait_method is
// a default, pull out the useful fields to make a ty_method
pub fn trait_method_to_ty_method(method: &trait_method) -> ty_method {
    match *method {
        required(ref m) => copy *m,
        provided(ref m) => {
            ty_method {
                ident: m.ident,
                attrs: copy m.attrs,
                purity: m.purity,
                decl: copy m.decl,
                generics: copy m.generics,
                self_ty: m.self_ty,
                id: m.id,
                span: m.span,
            }
        }
    }
}

pub fn split_trait_methods(trait_methods: &[trait_method])
    -> (~[ty_method], ~[@method]) {
    let mut reqd = ~[], provd = ~[];
    for trait_methods.each |trt_method| {
        match *trt_method {
          required(ref tm) => reqd.push(copy *tm),
          provided(m) => provd.push(m)
        }
    };
    (reqd, provd)
}

pub fn struct_field_visibility(field: ast::struct_field) -> visibility {
    match field.node.kind {
        ast::named_field(_, visibility) => visibility,
        ast::unnamed_field => ast::public
    }
}

pub trait inlined_item_utils {
    fn ident(&self) -> ident;
    fn id(&self) -> ast::node_id;
    fn accept<E: Copy>(&self, e: E, v: visit::vt<E>);
}

impl inlined_item_utils for inlined_item {
    fn ident(&self) -> ident {
        match *self {
            ii_item(i) => /* FIXME (#2543) */ copy i.ident,
            ii_foreign(i) => /* FIXME (#2543) */ copy i.ident,
            ii_method(_, m) => /* FIXME (#2543) */ copy m.ident,
        }
    }

    fn id(&self) -> ast::node_id {
        match *self {
            ii_item(i) => i.id,
            ii_foreign(i) => i.id,
            ii_method(_, m) => m.id,
        }
    }

    fn accept<E: Copy>(&self, e: E, v: visit::vt<E>) {
        match *self {
            ii_item(i) => (v.visit_item)(i, e, v),
            ii_foreign(i) => (v.visit_foreign_item)(i, e, v),
            ii_method(_, m) => visit::visit_method_helper(m, e, v),
        }
    }
}

/* True if d is either a def_self, or a chain of def_upvars
 referring to a def_self */
pub fn is_self(d: ast::def) -> bool {
  match d {
    def_self(*)           => true,
    def_upvar(_, d, _, _) => is_self(*d),
    _                     => false
  }
}

/// Maps a binary operator to its precedence
pub fn operator_prec(op: ast::binop) -> uint {
  match op {
      mul | div | rem   => 12u,
      // 'as' sits between here with 11
      add | subtract    => 10u,
      shl | shr         =>  9u,
      bitand            =>  8u,
      bitxor            =>  7u,
      bitor             =>  6u,
      lt | le | ge | gt =>  4u,
      eq | ne           =>  3u,
      and               =>  2u,
      or                =>  1u
  }
}

/// Precedence of the `as` operator, which is a binary operator
/// not appearing in the prior table.
pub static as_prec: uint = 11u;

pub fn empty_generics() -> Generics {
    Generics {lifetimes: opt_vec::Empty,
              ty_params: opt_vec::Empty}
}

// ______________________________________________________________________
// Enumerating the IDs which appear in an AST

#[auto_encode]
#[auto_decode]
pub struct id_range {
    min: node_id,
    max: node_id,
}

pub impl id_range {
    fn max() -> id_range {
        id_range {min: int::max_value,
                  max: int::min_value}
    }

    fn empty(&self) -> bool {
        self.min >= self.max
    }

    fn add(&mut self, id: node_id) {
        self.min = int::min(self.min, id);
        self.max = int::max(self.max, id + 1);
    }
}

pub fn id_visitor(vfn: @fn(node_id)) -> visit::vt<()> {
    let visit_generics: @fn(&Generics) = |generics| {
        for generics.ty_params.each |p| {
            vfn(p.id);
        }
        for generics.lifetimes.each |p| {
            vfn(p.id);
        }
    };
    visit::mk_simple_visitor(@visit::SimpleVisitor {
        visit_mod: |_m, _sp, id| vfn(id),

        visit_view_item: |vi| {
            match vi.node {
              view_item_extern_mod(_, _, id) => vfn(id),
              view_item_use(ref vps) => {
                  for vps.each |vp| {
                      match vp.node {
                          view_path_simple(_, _, id) => vfn(id),
                          view_path_glob(_, id) => vfn(id),
                          view_path_list(_, _, id) => vfn(id)
                      }
                  }
              }
            }
        },

        visit_foreign_item: |ni| vfn(ni.id),

        visit_item: |i| {
            vfn(i.id);
            match i.node {
              item_enum(ref enum_definition, _) =>
                for (*enum_definition).variants.each |v| { vfn(v.node.id); },
              _ => ()
            }
        },

        visit_local: |l| vfn(l.node.id),
        visit_block: |b| vfn(b.node.id),
        visit_stmt: |s| vfn(ast_util::stmt_id(s)),
        visit_arm: |_| {},
        visit_pat: |p| vfn(p.id),
        visit_decl: |_| {},

        visit_expr: |e| {
            vfn(e.callee_id);
            vfn(e.id);
        },

        visit_expr_post: |_| {},

        visit_ty: |t| {
            match t.node {
              ty_path(_, id) => vfn(id),
              _ => { /* fall through */ }
            }
        },

        visit_generics: visit_generics,

        visit_fn: |fk, d, _, _, id| {
            vfn(id);

            match *fk {
                visit::fk_item_fn(_, generics, _, _) => {
                    visit_generics(generics);
                }
                visit::fk_method(_, generics, m) => {
                    vfn(m.self_id);
                    visit_generics(generics);
                }
                visit::fk_anon(_) |
                visit::fk_fn_block => {
                }
            }

            for d.inputs.each |arg| {
                vfn(arg.id)
            }
        },

        visit_ty_method: |_| {},
        visit_trait_method: |_| {},
        visit_struct_def: |_, _, _, _| {},
        visit_struct_field: |f| vfn(f.node.id),
        visit_struct_method: |_| {}
    })
}

pub fn visit_ids_for_inlined_item(item: &inlined_item, vfn: @fn(node_id)) {
    item.accept((), id_visitor(vfn));
}

pub fn compute_id_range(visit_ids_fn: &fn(@fn(node_id))) -> id_range {
    let result = @mut id_range::max();
    do visit_ids_fn |id| {
        result.add(id);
    }
    *result
}

pub fn compute_id_range_for_inlined_item(item: &inlined_item) -> id_range {
    compute_id_range(|f| visit_ids_for_inlined_item(item, f))
}

pub fn is_item_impl(item: @ast::item) -> bool {
    match item.node {
       item_impl(*) => true,
       _            => false
    }
}

pub fn walk_pat(pat: @pat, it: &fn(@pat)) {
    it(pat);
    match pat.node {
        pat_ident(_, _, Some(p)) => walk_pat(p, it),
        pat_struct(_, ref fields, _) => {
            for fields.each |f| {
                walk_pat(f.pat, it)
            }
        }
        pat_enum(_, Some(ref s)) | pat_tup(ref s) => {
            for s.each |p| {
                walk_pat(*p, it)
            }
        }
        pat_box(s) | pat_uniq(s) | pat_region(s) => {
            walk_pat(s, it)
        }
        pat_vec(ref before, ref slice, ref after) => {
            for before.each |p| {
                walk_pat(*p, it)
            }
            for slice.each |p| {
                walk_pat(*p, it)
            }
            for after.each |p| {
                walk_pat(*p, it)
            }
        }
        pat_wild | pat_lit(_) | pat_range(_, _) | pat_ident(_, _, _) |
        pat_enum(_, _) => { }
    }
}

pub fn view_path_id(p: @view_path) -> node_id {
    match p.node {
      view_path_simple(_, _, id) |
      view_path_glob(_, id) |
      view_path_list(_, _, id) => id
    }
}

/// Returns true if the given struct def is tuple-like; i.e. that its fields
/// are unnamed.
pub fn struct_def_is_tuple_like(struct_def: @ast::struct_def) -> bool {
    struct_def.ctor_id.is_some()
}

pub fn visibility_to_privacy(visibility: visibility) -> Privacy {
    match visibility {
        public => Public,
        inherited | private => Private
    }
}

pub fn variant_visibility_to_privacy(visibility: visibility,
                                     enclosing_is_public: bool)
                                  -> Privacy {
    if enclosing_is_public {
        match visibility {
            public | inherited => Public,
            private => Private
        }
    } else {
        visibility_to_privacy(visibility)
    }
}

#[deriving(Eq)]
pub enum Privacy {
    Private,
    Public
}

// HYGIENE FUNCTIONS

/// Construct an identifier with the given repr and an empty context:
pub fn mk_ident(repr: uint) -> ident { ident {repr: repr, ctxt: 0}}

/// Extend a syntax context with a given mark
pub fn mk_mark (m:Mrk,ctxt:SyntaxContext,table:&mut SCTable)
    -> SyntaxContext {
    idx_push(table,Mark(m,ctxt))
}

/// Extend a syntax context with a given rename
pub fn mk_rename (id:ident, to:Name, tail:SyntaxContext, table: &mut SCTable)
    -> SyntaxContext {
    idx_push(table,Rename(id,to,tail))
}

/// Make a fresh syntax context table with EmptyCtxt in slot zero
pub fn mk_sctable() -> SCTable { ~[EmptyCtxt] }

/// Add a value to the end of a vec, return its index
fn idx_push<T>(vec: &mut ~[T], val: T) -> uint {
    vec.push(val);
    vec.len() - 1
}

/// Resolve a syntax object to a name, per MTWT.
pub fn resolve (id : ident, table : &SCTable) -> Name {
    match table[id.ctxt] {
        EmptyCtxt => id.repr,
        // ignore marks here:
        Mark(_,subctxt) => resolve (ident{repr:id.repr, ctxt: subctxt},table),
        // do the rename if necessary:
        Rename(ident{repr,ctxt},toname,subctxt) => {
            // this could be cached or computed eagerly:
            let resolvedfrom = resolve(ident{repr:repr,ctxt:ctxt},table);
            let resolvedthis = resolve(ident{repr:id.repr,ctxt:subctxt},table);
            if ((resolvedthis == resolvedfrom)
                && (marksof (ctxt,resolvedthis,table)
                    == marksof (subctxt,resolvedthis,table))) {
                toname
            } else {
                resolvedthis
            }
        }
    }
}

/// Compute the marks associated with a syntax context.
// it's not clear to me whether it's better to use a [] mutable
// vector or a cons-list for this.
pub fn marksof(ctxt: SyntaxContext, stopname: Name, table: &SCTable) -> ~[Mrk] {
    let mut result = ~[];
    let mut loopvar = ctxt;
    loop {
        match table[loopvar] {
            EmptyCtxt => {return result;},
            Mark(mark,tl) => {
                xorPush(&mut result,mark);
                loopvar = tl;
            },
            Rename(_,name,tl) => {
                // see MTWT for details on the purpose of the stopname.
                // short version: it prevents duplication of effort.
                if (name == stopname) {
                    return result;
                } else {
                    loopvar = tl;
                }
            }
        }
    }
}

/// Push a name... unless it matches the one on top, in which
/// case pop and discard (so two of the same marks cancel)
pub fn xorPush(marks: &mut ~[uint], mark: uint) {
    if ((marks.len() > 0) && (getLast(marks) == mark)) {
        marks.pop();
    } else {
        marks.push(mark);
    }
}

// get the last element of a mutable array.
// FIXME #4903: , must be a separate procedure for now.
pub fn getLast(arr: &~[Mrk]) -> uint {
    *arr.last()
}


#[cfg(test)]
mod test {
    use ast::*;
    use super::*;
    use core::io;

    #[test] fn xorpush_test () {
        let mut s = ~[];
        xorPush(&mut s,14);
        assert_eq!(s,~[14]);
        xorPush(&mut s,14);
        assert_eq!(s,~[]);
        xorPush(&mut s,14);
        assert_eq!(s,~[14]);
        xorPush(&mut s,15);
        assert_eq!(s,~[14,15]);
        xorPush (&mut s,16);
        assert_eq! (s,~[14,15,16]);
        xorPush (&mut s,16);
        assert_eq! (s,~[14,15]);
        xorPush (&mut s,15);
        assert_eq! (s,~[14]);
    }

    // convert a list of uints to an @~[ident]
    // (ignores the interner completely)
    fn uints_to_idents (uints: &~[uint]) -> @~[ident] {
        @uints.map(|u|{ ident {repr:*u, ctxt: empty_ctxt} })
    }

    fn id (u : uint, s: SyntaxContext) -> ident {
        ident{repr:u, ctxt: s}
    }

    // because of the SCTable, I now need a tidy way of
    // creating syntax objects. Sigh.
    #[deriving(Eq)]
    enum TestSC {
        M(Mrk),
        R(ident,Name)
    }

    // unfold a vector of TestSC values into a SCTable,
    // returning the resulting index
    fn unfold_test_sc(tscs : ~[TestSC], tail: SyntaxContext, table : &mut SCTable)
        -> SyntaxContext {
        tscs.foldr(tail, |tsc : &TestSC,tail : SyntaxContext|
                  {match *tsc {
                      M(mrk) => mk_mark(mrk,tail,table),
                      R(ident,name) => mk_rename(ident,name,tail,table)}})
    }

    // gather a SyntaxContext back into a vector of TestSCs
    fn refold_test_sc(mut sc: SyntaxContext, table : &SCTable) -> ~[TestSC] {
        let mut result = ~[];
        loop {
            match table[sc] {
                EmptyCtxt => {return result;},
                Mark(mrk,tail) => {
                    result.push(M(mrk));
                    sc = tail;
                    loop;
                },
                Rename(id,name,tail) => {
                    result.push(R(id,name));
                    sc = tail;
                    loop;
                }
            }
        }
    }

    #[test] fn test_unfold_refold(){
        let mut t = mk_sctable();

        let test_sc = ~[M(3),R(id(101,0),14),M(9)];
        assert_eq!(unfold_test_sc(test_sc,empty_ctxt,&mut t),3);
        assert_eq!(t[1],Mark(9,0));
        assert_eq!(t[2],Rename(id(101,0),14,1));
        assert_eq!(t[3],Mark(3,2));
        assert_eq!(refold_test_sc(3,&t),test_sc);
    }

    // extend a syntax context with a sequence of marks given
    // in a vector. v[0] will be the outermost mark.
    fn unfold_marks(mrks:~[Mrk],tail:SyntaxContext,table: &mut SCTable) -> SyntaxContext {
        mrks.foldr(tail, |mrk:&Mrk,tail:SyntaxContext|
                   {mk_mark(*mrk,tail,table)})
    }

    #[test] fn unfold_marks_test() {
        let mut t = ~[EmptyCtxt];

        assert_eq!(unfold_marks(~[3,7],empty_ctxt,&mut t),2);
        assert_eq!(t[1],Mark(7,0));
        assert_eq!(t[2],Mark(3,1));
    }

    #[test] fn test_marksof () {
        let stopname = 242;
        let name1 = 243;
        let mut t = mk_sctable();
        assert_eq!(marksof (empty_ctxt,stopname,&t),~[]);
        // FIXME #5074: ANF'd to dodge nested calls
        { let ans = unfold_marks(~[4,98],empty_ctxt,&mut t);
         assert_eq! (marksof (ans,stopname,&t),~[4,98]);}
        // does xoring work?
        { let ans = unfold_marks(~[5,5,16],empty_ctxt,&mut t);
         assert_eq! (marksof (ans,stopname,&t), ~[16]);}
        // does nested xoring work?
        { let ans = unfold_marks(~[5,10,10,5,16],empty_ctxt,&mut t);
         assert_eq! (marksof (ans, stopname,&t), ~[16]);}
        // rename where stop doesn't match:
        { let chain = ~[M(9),
                        R(id(name1,
                             mk_mark (4, empty_ctxt,&mut t)),
                          100101102),
                        M(14)];
         let ans = unfold_test_sc(chain,empty_ctxt,&mut t);
         assert_eq! (marksof (ans, stopname, &t), ~[9,14]);}
        // rename where stop does match
        { let name1sc = mk_mark(4, empty_ctxt, &mut t);
         let chain = ~[M(9),
                       R(id(name1, name1sc),
                         stopname),
                       M(14)];
         let ans = unfold_test_sc(chain,empty_ctxt,&mut t);
         assert_eq! (marksof (ans, stopname, &t), ~[9]); }
    }


    #[test] fn resolve_tests () {
        let a = 40;
        let mut t = mk_sctable();
        // - ctxt is MT
        assert_eq!(resolve(id(a,empty_ctxt),&t),a);
        // - simple ignored marks
        { let sc = unfold_marks(~[1,2,3],empty_ctxt,&mut t);
         assert_eq!(resolve(id(a,sc),&t),a);}
        // - orthogonal rename where names don't match
        { let sc = unfold_test_sc(~[R(id(50,empty_ctxt),51),M(12)],empty_ctxt,&mut t);
         assert_eq!(resolve(id(a,sc),&t),a);}
        // - rename where names do match, but marks don't
        { let sc1 = mk_mark(1,empty_ctxt,&mut t);
         let sc = unfold_test_sc(~[R(id(a,sc1),50),
                                   M(1),
                                   M(2)],
                                 empty_ctxt,&mut t);
        assert_eq!(resolve(id(a,sc),&t), a);}
        // - rename where names and marks match
        { let sc1 = unfold_test_sc(~[M(1),M(2)],empty_ctxt,&mut t);
         let sc = unfold_test_sc(~[R(id(a,sc1),50),M(1),M(2)],empty_ctxt,&mut t);
         assert_eq!(resolve(id(a,sc),&t), 50); }
        // - rename where names and marks match by literal sharing
        { let sc1 = unfold_test_sc(~[M(1),M(2)],empty_ctxt,&mut t);
         let sc = unfold_test_sc(~[R(id(a,sc1),50)],sc1,&mut t);
         assert_eq!(resolve(id(a,sc),&t), 50); }
        // - two renames of the same var.. can only happen if you use
        // local-expand to prevent the inner binding from being renamed
        // during the rename-pass caused by the first:
        io::println("about to run bad test");
        { let sc = unfold_test_sc(~[R(id(a,empty_ctxt),50),
                                    R(id(a,empty_ctxt),51)],
                                  empty_ctxt,&mut t);
         assert_eq!(resolve(id(a,sc),&t), 51); }
        // the simplest double-rename:
        { let a_to_a50 = mk_rename(id(a,empty_ctxt),50,empty_ctxt,&mut t);
         let a50_to_a51 = mk_rename(id(a,a_to_a50),51,a_to_a50,&mut t);
         assert_eq!(resolve(id(a,a50_to_a51),&t),51);
         // mark on the outside doesn't stop rename:
         let sc = mk_mark(9,a50_to_a51,&mut t);
         assert_eq!(resolve(id(a,sc),&t),51);
         // but mark on the inside does:
         let a50_to_a51_b = unfold_test_sc(~[R(id(a,a_to_a50),51),
                                              M(9)],
                                           a_to_a50,
                                           &mut t);
         assert_eq!(resolve(id(a,a50_to_a51_b),&t),50);}
    }

}
