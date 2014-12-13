// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(unboxed_closures)]

use std::collections::{TrieMap, TreeMap, HashMap, HashSet};
use std::os;
use std::rand::{Rng, IsaacRng, SeedableRng};
use std::time::Duration;
use std::uint;

fn timed<F>(label: &str, f: F) where F: FnMut() {
    println!("  {}: {}", label, Duration::span(f));
}

trait MutableMap {
    fn insert(&mut self, k: uint, v: uint);
    fn remove(&mut self, k: &uint) -> bool;
    fn find(&self, k: &uint) -> Option<&uint>;
}

impl MutableMap for TreeMap<uint, uint> {
    fn insert(&mut self, k: uint, v: uint) { self.insert(k, v); }
    fn remove(&mut self, k: &uint) -> bool { self.remove(k).is_some() }
    fn find(&self, k: &uint) -> Option<&uint> { self.get(k) }
}
impl MutableMap for HashMap<uint, uint> {
    fn insert(&mut self, k: uint, v: uint) { self.insert(k, v); }
    fn remove(&mut self, k: &uint) -> bool { self.remove(k).is_some() }
    fn find(&self, k: &uint) -> Option<&uint> { self.get(k) }
}
impl MutableMap for TrieMap<uint> {
    fn insert(&mut self, k: uint, v: uint) { self.insert(k, v); }
    fn remove(&mut self, k: &uint) -> bool { self.remove(k).is_some() }
    fn find(&self, k: &uint) -> Option<&uint> { self.get(k) }
}

fn ascending<M: MutableMap>(map: &mut M, n_keys: uint) {
    println!(" Ascending integers:");

    timed("insert", || {
        for i in range(0u, n_keys) {
            map.insert(i, i + 1);
        }
    });

    timed("search", || {
        for i in range(0u, n_keys) {
            assert_eq!(map.find(&i).unwrap(), &(i + 1));
        }
    });

    timed("remove", || {
        for i in range(0, n_keys) {
            assert!(map.remove(&i));
        }
    });
}

fn descending<M: MutableMap>(map: &mut M, n_keys: uint) {
    println!(" Descending integers:");

    timed("insert", || {
        for i in range(0, n_keys).rev() {
            map.insert(i, i + 1);
        }
    });

    timed("search", || {
        for i in range(0, n_keys).rev() {
            assert_eq!(map.find(&i).unwrap(), &(i + 1));
        }
    });

    timed("remove", || {
        for i in range(0, n_keys) {
            assert!(map.remove(&i));
        }
    });
}

fn vector<M: MutableMap>(map: &mut M, n_keys: uint, dist: &[uint]) {
    timed("insert", || {
        for i in range(0u, n_keys) {
            map.insert(dist[i], i + 1);
        }
    });

    timed("search", || {
        for i in range(0u, n_keys) {
            assert_eq!(map.find(&dist[i]).unwrap(), &(i + 1));
        }
    });

    timed("remove", || {
        for i in range(0u, n_keys) {
            assert!(map.remove(&dist[i]));
        }
    });
}

fn main() {
    let args = os::args();
    let args = args.as_slice();
    let n_keys = {
        if args.len() == 2 {
            from_str::<uint>(args[1].as_slice()).unwrap()
        } else {
            1000000
        }
    };

    let mut rand = Vec::with_capacity(n_keys);

    {
        let seed: &[_] = &[1, 1, 1, 1, 1, 1, 1];
        let mut rng: IsaacRng = SeedableRng::from_seed(seed);
        let mut set = HashSet::new();
        while set.len() != n_keys {
            let next = rng.gen();
            if set.insert(next) {
                rand.push(next);
            }
        }
    }

    println!("{} keys", n_keys);

    // FIXME: #9970
    println!("{}", "\nTreeMap:");

    {
        let mut map: TreeMap<uint,uint> = TreeMap::new();
        ascending(&mut map, n_keys);
    }

    {
        let mut map: TreeMap<uint,uint> = TreeMap::new();
        descending(&mut map, n_keys);
    }

    {
        println!(" Random integers:");
        let mut map: TreeMap<uint,uint> = TreeMap::new();
        vector(&mut map, n_keys, rand.as_slice());
    }

    // FIXME: #9970
    println!("{}", "\nHashMap:");

    {
        let mut map: HashMap<uint,uint> = HashMap::new();
        ascending(&mut map, n_keys);
    }

    {
        let mut map: HashMap<uint,uint> = HashMap::new();
        descending(&mut map, n_keys);
    }

    {
        println!(" Random integers:");
        let mut map: HashMap<uint,uint> = HashMap::new();
        vector(&mut map, n_keys, rand.as_slice());
    }

    // FIXME: #9970
    println!("{}", "\nTrieMap:");

    {
        let mut map: TrieMap<uint> = TrieMap::new();
        ascending(&mut map, n_keys);
    }

    {
        let mut map: TrieMap<uint> = TrieMap::new();
        descending(&mut map, n_keys);
    }

    {
        println!(" Random integers:");
        let mut map: TrieMap<uint> = TrieMap::new();
        vector(&mut map, n_keys, rand.as_slice());
    }
}
