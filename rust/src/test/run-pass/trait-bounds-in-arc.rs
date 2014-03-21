// ignore-pretty

// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that a heterogeneous list of existential types can be put inside an Arc
// and shared between tasks as long as all types fulfill Freeze+Send.

// ignore-fast

extern crate sync;

use sync::Arc;
use std::task;

trait Pet {
    fn name(&self, blk: |&str|);
    fn num_legs(&self) -> uint;
    fn of_good_pedigree(&self) -> bool;
}

struct Catte {
    num_whiskers: uint,
    name: ~str,
}

struct Dogge {
    bark_decibels: uint,
    tricks_known: uint,
    name: ~str,
}

struct Goldfyshe {
    swim_speed: uint,
    name: ~str,
}

impl Pet for Catte {
    fn name(&self, blk: |&str|) { blk(self.name) }
    fn num_legs(&self) -> uint { 4 }
    fn of_good_pedigree(&self) -> bool { self.num_whiskers >= 4 }
}
impl Pet for Dogge {
    fn name(&self, blk: |&str|) { blk(self.name) }
    fn num_legs(&self) -> uint { 4 }
    fn of_good_pedigree(&self) -> bool {
        self.bark_decibels < 70 || self.tricks_known > 20
    }
}
impl Pet for Goldfyshe {
    fn name(&self, blk: |&str|) { blk(self.name) }
    fn num_legs(&self) -> uint { 0 }
    fn of_good_pedigree(&self) -> bool { self.swim_speed >= 500 }
}

pub fn main() {
    let catte = Catte { num_whiskers: 7, name: ~"alonzo_church" };
    let dogge1 = Dogge { bark_decibels: 100, tricks_known: 42, name: ~"alan_turing" };
    let dogge2 = Dogge { bark_decibels: 55,  tricks_known: 11, name: ~"albert_einstein" };
    let fishe = Goldfyshe { swim_speed: 998, name: ~"alec_guinness" };
    let arc = Arc::new(vec!(~catte  as ~Pet:Share+Send,
                         ~dogge1 as ~Pet:Share+Send,
                         ~fishe  as ~Pet:Share+Send,
                         ~dogge2 as ~Pet:Share+Send));
    let (tx1, rx1) = channel();
    let arc1 = arc.clone();
    task::spawn(proc() { check_legs(arc1); tx1.send(()); });
    let (tx2, rx2) = channel();
    let arc2 = arc.clone();
    task::spawn(proc() { check_names(arc2); tx2.send(()); });
    let (tx3, rx3) = channel();
    let arc3 = arc.clone();
    task::spawn(proc() { check_pedigree(arc3); tx3.send(()); });
    rx1.recv();
    rx2.recv();
    rx3.recv();
}

fn check_legs(arc: Arc<Vec<~Pet:Share+Send>>) {
    let mut legs = 0;
    for pet in arc.get().iter() {
        legs += pet.num_legs();
    }
    assert!(legs == 12);
}
fn check_names(arc: Arc<Vec<~Pet:Share+Send>>) {
    for pet in arc.get().iter() {
        pet.name(|name| {
            assert!(name[0] == 'a' as u8 && name[1] == 'l' as u8);
        })
    }
}
fn check_pedigree(arc: Arc<Vec<~Pet:Share+Send>>) {
    for pet in arc.get().iter() {
        assert!(pet.of_good_pedigree());
    }
}
