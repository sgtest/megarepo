// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// Detecting language items.
//
// Language items are items that represent concepts intrinsic to the language
// itself. Examples are:
//
// * Traits that specify "kinds"; e.g. "const", "copy", "owned".
//
// * Traits that represent operators; e.g. "add", "sub", "index".
//
// * Functions called by the compiler itself.

use core::prelude::*;

use driver::session::Session;
use metadata::csearch::{each_lang_item, get_item_attrs};
use metadata::cstore::{iter_crate_data};
use metadata::decoder::{dl_def, dl_field, dl_impl};
use syntax::ast::{crate, def_fn, def_id, def_ty, lit_str, meta_item};
use syntax::ast::{meta_list, meta_name_value, meta_word};
use syntax::ast_util::{local_def};
use syntax::visit::{default_simple_visitor, mk_simple_visitor, SimpleVisitor};
use syntax::visit::{visit_crate, visit_item};

use core::ptr;
use std::map::HashMap;
use str_eq = str::eq;

pub enum LangItem {
    ConstTraitLangItem,         // 0
    CopyTraitLangItem,          // 1
    OwnedTraitLangItem,         // 2
    DurableTraitLangItem,       // 3

    DropTraitLangItem,          // 4

    AddTraitLangItem,           // 5
    SubTraitLangItem,           // 6
    MulTraitLangItem,           // 7
    DivTraitLangItem,           // 8
    ModuloTraitLangItem,        // 9
    NegTraitLangItem,           // 10
    NotTraitLangItem,           // 11
    BitXorTraitLangItem,        // 12
    BitAndTraitLangItem,        // 13
    BitOrTraitLangItem,         // 14
    ShlTraitLangItem,           // 15
    ShrTraitLangItem,           // 16
    IndexTraitLangItem,         // 17

    EqTraitLangItem,            // 18
    OrdTraitLangItem,           // 19

    StrEqFnLangItem,            // 20
    UniqStrEqFnLangItem,        // 21
    AnnihilateFnLangItem,       // 22
    LogTypeFnLangItem,          // 23
    FailFnLangItem,             // 24
    FailBoundsCheckFnLangItem,  // 25
    ExchangeMallocFnLangItem,   // 26
    ExchangeFreeFnLangItem,     // 27
    MallocFnLangItem,           // 28
    FreeFnLangItem,             // 29
}

struct LanguageItems {
    items: [ Option<def_id> * 30 ]
}

impl LanguageItems {
    static pub fn new() -> LanguageItems {
        LanguageItems {
            items: [ None, ..30 ]
        }
    }

    fn each_item(&self, f: &fn(def_id: def_id, i: uint) -> bool) {
        for self.items.eachi |i, &item| {
            if !f(item.get(), i) {
                break;
            }
        }
    }

    static pub fn item_name(index: uint) -> &static/str {
        match index {
            0  => "const",
            1  => "copy",
            2  => "owned",
            3  => "durable",

            4  => "drop",

            5  => "add",
            6  => "sub",
            7  => "mul",
            8  => "div",
            9  => "modulo",
            10 => "neg",
            11 => "not",
            12 => "bitxor",
            13 => "bitand",
            14 => "bitor",
            15 => "shl",
            16 => "shr",
            17 => "index",
            18 => "eq",
            19 => "ord",

            20 => "str_eq",
            21 => "uniq_str_eq",
            22 => "annihilate",
            23 => "log_type",
            24 => "fail_",
            25 => "fail_bounds_check",
            26 => "exchange_malloc",
            27 => "exchange_free",
            28 => "malloc",
            29 => "free",

            _ => "???"
        }
    }

    // XXX: Method macros sure would be nice here.

    pub fn const_trait(&const self) -> def_id {
        self.items[ConstTraitLangItem as uint].get()
    }
    pub fn copy_trait(&const self) -> def_id {
        self.items[CopyTraitLangItem as uint].get()
    }
    pub fn owned_trait(&const self) -> def_id {
        self.items[OwnedTraitLangItem as uint].get()
    }
    pub fn durable_trait(&const self) -> def_id {
        self.items[DurableTraitLangItem as uint].get()
    }

    pub fn drop_trait(&const self) -> def_id {
        self.items[DropTraitLangItem as uint].get()
    }

    pub fn add_trait(&const self) -> def_id {
        self.items[AddTraitLangItem as uint].get()
    }
    pub fn sub_trait(&const self) -> def_id {
        self.items[SubTraitLangItem as uint].get()
    }
    pub fn mul_trait(&const self) -> def_id {
        self.items[MulTraitLangItem as uint].get()
    }
    pub fn div_trait(&const self) -> def_id {
        self.items[DivTraitLangItem as uint].get()
    }
    pub fn modulo_trait(&const self) -> def_id {
        self.items[ModuloTraitLangItem as uint].get()
    }
    pub fn neg_trait(&const self) -> def_id {
        self.items[NegTraitLangItem as uint].get()
    }
    pub fn not_trait(&const self) -> def_id {
        self.items[NotTraitLangItem as uint].get()
    }
    pub fn bitxor_trait(&const self) -> def_id {
        self.items[BitXorTraitLangItem as uint].get()
    }
    pub fn bitand_trait(&const self) -> def_id {
        self.items[BitAndTraitLangItem as uint].get()
    }
    pub fn bitor_trait(&const self) -> def_id {
        self.items[BitOrTraitLangItem as uint].get()
    }
    pub fn shl_trait(&const self) -> def_id {
        self.items[ShlTraitLangItem as uint].get()
    }
    pub fn shr_trait(&const self) -> def_id {
        self.items[ShrTraitLangItem as uint].get()
    }
    pub fn index_trait(&const self) -> def_id {
        self.items[IndexTraitLangItem as uint].get()
    }

    pub fn eq_trait(&const self) -> def_id {
        self.items[EqTraitLangItem as uint].get()
    }
    pub fn ord_trait(&const self) -> def_id {
        self.items[OrdTraitLangItem as uint].get()
    }

    pub fn str_eq_fn(&const self) -> def_id {
        self.items[StrEqFnLangItem as uint].get()
    }
    pub fn uniq_str_eq_fn(&const self) -> def_id {
        self.items[UniqStrEqFnLangItem as uint].get()
    }
    pub fn annihilate_fn(&const self) -> def_id {
        self.items[AnnihilateFnLangItem as uint].get()
    }
    pub fn log_type_fn(&const self) -> def_id {
        self.items[LogTypeFnLangItem as uint].get()
    }
    pub fn fail_fn(&const self) -> def_id {
        self.items[FailFnLangItem as uint].get()
    }
    pub fn fail_bounds_check_fn(&const self) -> def_id {
        self.items[FailBoundsCheckFnLangItem as uint].get()
    }
    pub fn exchange_malloc_fn(&const self) -> def_id {
        self.items[ExchangeMallocFnLangItem as uint].get()
    }
    pub fn exchange_free_fn(&const self) -> def_id {
        self.items[ExchangeFreeFnLangItem as uint].get()
    }
    pub fn malloc_fn(&const self) -> def_id {
        self.items[MallocFnLangItem as uint].get()
    }
    pub fn free_fn(&const self) -> def_id {
        self.items[FreeFnLangItem as uint].get()
    }
}

fn LanguageItemCollector(crate: @crate,
                         session: Session,
                         items: &r/mut LanguageItems)
                      -> LanguageItemCollector/&r {
    let item_refs = HashMap();

    item_refs.insert(~"const", ConstTraitLangItem as uint);
    item_refs.insert(~"copy", CopyTraitLangItem as uint);
    item_refs.insert(~"owned", OwnedTraitLangItem as uint);
    item_refs.insert(~"durable", DurableTraitLangItem as uint);

    item_refs.insert(~"drop", DropTraitLangItem as uint);

    item_refs.insert(~"add", AddTraitLangItem as uint);
    item_refs.insert(~"sub", SubTraitLangItem as uint);
    item_refs.insert(~"mul", MulTraitLangItem as uint);
    item_refs.insert(~"div", DivTraitLangItem as uint);
    item_refs.insert(~"modulo", ModuloTraitLangItem as uint);
    item_refs.insert(~"neg", NegTraitLangItem as uint);
    item_refs.insert(~"not", NotTraitLangItem as uint);
    item_refs.insert(~"bitxor", BitXorTraitLangItem as uint);
    item_refs.insert(~"bitand", BitAndTraitLangItem as uint);
    item_refs.insert(~"bitor", BitOrTraitLangItem as uint);
    item_refs.insert(~"shl", ShlTraitLangItem as uint);
    item_refs.insert(~"shr", ShrTraitLangItem as uint);
    item_refs.insert(~"index", IndexTraitLangItem as uint);

    item_refs.insert(~"eq", EqTraitLangItem as uint);
    item_refs.insert(~"ord", OrdTraitLangItem as uint);

    item_refs.insert(~"str_eq", StrEqFnLangItem as uint);
    item_refs.insert(~"uniq_str_eq", UniqStrEqFnLangItem as uint);
    item_refs.insert(~"annihilate", AnnihilateFnLangItem as uint);
    item_refs.insert(~"log_type", LogTypeFnLangItem as uint);
    item_refs.insert(~"fail_", FailFnLangItem as uint);
    item_refs.insert(~"fail_bounds_check", FailBoundsCheckFnLangItem as uint);
    item_refs.insert(~"exchange_malloc", ExchangeMallocFnLangItem as uint);
    item_refs.insert(~"exchange_free", ExchangeFreeFnLangItem as uint);
    item_refs.insert(~"malloc", MallocFnLangItem as uint);
    item_refs.insert(~"free", FreeFnLangItem as uint);

    LanguageItemCollector {
        crate: crate,
        session: session,
        items: items,
        item_refs: item_refs
    }
}

struct LanguageItemCollector {
    items: &mut LanguageItems,

    crate: @crate,
    session: Session,

    item_refs: HashMap<~str,uint>,
}

impl LanguageItemCollector {
    fn match_and_collect_meta_item(item_def_id: def_id,
                                   meta_item: meta_item) {
        match meta_item.node {
            meta_name_value(ref key, literal) => {
                match literal.node {
                    lit_str(value) => {
                        self.match_and_collect_item(item_def_id,
                                                    (/*bad*/copy *key),
                                                    /*bad*/copy *value);
                    }
                    _ => {} // Skip.
                }
            }
            meta_word(*) | meta_list(*) => {} // Skip.
        }
    }

    fn collect_item(item_index: uint, item_def_id: def_id) {
        // Check for duplicates.
        match self.items.items[item_index] {
            Some(original_def_id) if original_def_id != item_def_id => {
                self.session.err(fmt!("duplicate entry for `%s`",
                                      LanguageItems::item_name(item_index)));
            }
            Some(_) | None => {
                // OK.
            }
        }

        // Matched.
        self.items.items[item_index] = Some(item_def_id);
    }

    fn match_and_collect_item(item_def_id: def_id, key: ~str, value: ~str) {
        if key != ~"lang" {
            return;    // Didn't match.
        }

        match self.item_refs.find(value) {
            None => {
                // Didn't match.
            }
            Some(item_index) => {
                self.collect_item(item_index, item_def_id)
            }
        }
    }

    fn collect_local_language_items() {
        let this = unsafe { ptr::addr_of(&self) };
        visit_crate(*self.crate, (), mk_simple_visitor(@SimpleVisitor {
            visit_item: |item| {
                for item.attrs.each |attribute| {
                    unsafe {
                        (*this).match_and_collect_meta_item(local_def(item
                                                                      .id),
                                                            attribute.node
                                                                     .value);
                    }
                }
            },
            .. *default_simple_visitor()
        }));
    }

    fn collect_external_language_items() {
        let crate_store = self.session.cstore;
        do iter_crate_data(crate_store) |crate_number, _crate_metadata| {
            for each_lang_item(crate_store, crate_number)
                    |node_id, item_index| {
                let def_id = { crate: crate_number, node: node_id };
                self.collect_item(item_index, def_id);
            }
        }
    }

    fn check_completeness() {
        for self.item_refs.each |key, item_ref| {
            match self.items.items[item_ref] {
                None => {
                    self.session.err(fmt!("no item found for `%s`", key));
                }
                Some(_) => {
                    // OK.
                }
            }
        }
    }

    fn collect() {
        self.collect_local_language_items();
        self.collect_external_language_items();
        self.check_completeness();
    }
}

fn collect_language_items(crate: @crate, session: Session) -> LanguageItems {
    let mut items = LanguageItems::new();
    let collector = LanguageItemCollector(crate, session, &mut items);
    collector.collect();
    copy items
}

