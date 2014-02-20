// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! An ordered map and set implemented as self-balancing binary search
//! trees. The only requirement for the types is that the key implements
//! `TotalOrd`.

use std::iter::{Peekable};
use std::cmp::Ordering;
use std::mem::{replace, swap};
use std::ptr;

use serialize::{Encodable, Decodable, Encoder, Decoder};

// This is implemented as an AA tree, which is a simplified variation of
// a red-black tree where red (horizontal) nodes can only be added
// as a right child. The time complexity is the same, and re-balancing
// operations are more frequent but also cheaper.

// Future improvements:

// range search - O(log n) retrieval of an iterator from some key

// (possibly) implement the overloads Python does for sets:
//   * intersection: &
//   * difference: -
//   * symmetric difference: ^
//   * union: |
// These would be convenient since the methods work like `each`

#[allow(missing_doc)]
#[deriving(Clone)]
pub struct TreeMap<K, V> {
    priv root: Option<~TreeNode<K, V>>,
    priv length: uint
}

impl<K: Eq + TotalOrd, V: Eq> Eq for TreeMap<K, V> {
    fn eq(&self, other: &TreeMap<K, V>) -> bool {
        self.len() == other.len() &&
            self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

// Lexicographical comparison
fn lt<K: Ord + TotalOrd, V: Ord>(a: &TreeMap<K, V>,
                                 b: &TreeMap<K, V>) -> bool {
    // the Zip iterator is as long as the shortest of a and b.
    for ((key_a, value_a), (key_b, value_b)) in a.iter().zip(b.iter()) {
        if *key_a < *key_b { return true; }
        if *key_a > *key_b { return false; }
        if *value_a < *value_b { return true; }
        if *value_a > *value_b { return false; }
    }

    a.len() < b.len()
}

impl<K: Ord + TotalOrd, V: Ord> Ord for TreeMap<K, V> {
    #[inline]
    fn lt(&self, other: &TreeMap<K, V>) -> bool { lt(self, other) }
    #[inline]
    fn le(&self, other: &TreeMap<K, V>) -> bool { !lt(other, self) }
    #[inline]
    fn ge(&self, other: &TreeMap<K, V>) -> bool { !lt(self, other) }
    #[inline]
    fn gt(&self, other: &TreeMap<K, V>) -> bool { lt(other, self) }
}

impl<K: TotalOrd, V> Container for TreeMap<K, V> {
    /// Return the number of elements in the map
    fn len(&self) -> uint { self.length }

    /// Return true if the map contains no elements
    fn is_empty(&self) -> bool { self.root.is_none() }
}

impl<K: TotalOrd, V> Mutable for TreeMap<K, V> {
    /// Clear the map, removing all key-value pairs.
    fn clear(&mut self) {
        self.root = None;
        self.length = 0
    }
}

impl<K: TotalOrd, V> Map<K, V> for TreeMap<K, V> {
    /// Return a reference to the value corresponding to the key
    fn find<'a>(&'a self, key: &K) -> Option<&'a V> {
        let mut current: &'a Option<~TreeNode<K, V>> = &self.root;
        loop {
            match *current {
              Some(ref r) => {
                match key.cmp(&r.key) {
                  Less => current = &r.left,
                  Greater => current = &r.right,
                  Equal => return Some(&r.value)
                }
              }
              None => return None
            }
        }
    }
}

impl<K: TotalOrd, V> MutableMap<K, V> for TreeMap<K, V> {
    /// Return a mutable reference to the value corresponding to the key
    #[inline]
    fn find_mut<'a>(&'a mut self, key: &K) -> Option<&'a mut V> {
        find_mut(&mut self.root, key)
    }

    /// Insert a key-value pair from the map. If the key already had a value
    /// present in the map, that value is returned. Otherwise None is returned.
    fn swap(&mut self, key: K, value: V) -> Option<V> {
        let ret = insert(&mut self.root, key, value);
        if ret.is_none() { self.length += 1 }
        ret
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    fn pop(&mut self, key: &K) -> Option<V> {
        let ret = remove(&mut self.root, key);
        if ret.is_some() { self.length -= 1 }
        ret
    }
}

impl<K: TotalOrd, V> TreeMap<K, V> {
    /// Create an empty TreeMap
    pub fn new() -> TreeMap<K, V> { TreeMap{root: None, length: 0} }

    /// Get a lazy iterator over the key-value pairs in the map.
    /// Requires that it be frozen (immutable).
    pub fn iter<'a>(&'a self) -> Entries<'a, K, V> {
        Entries {
            stack: ~[],
            node: deref(&self.root),
            remaining_min: self.length,
            remaining_max: self.length
        }
    }

    /// Get a lazy reverse iterator over the key-value pairs in the map.
    /// Requires that it be frozen (immutable).
    pub fn rev_iter<'a>(&'a self) -> RevEntries<'a, K, V> {
        RevEntries{iter: self.iter()}
    }

    /// Get a lazy forward iterator over the key-value pairs in the
    /// map, with the values being mutable.
    pub fn mut_iter<'a>(&'a mut self) -> MutEntries<'a, K, V> {
        MutEntries {
            stack: ~[],
            node: mut_deref(&mut self.root),
            remaining_min: self.length,
            remaining_max: self.length
        }
    }
    /// Get a lazy reverse iterator over the key-value pairs in the
    /// map, with the values being mutable.
    pub fn mut_rev_iter<'a>(&'a mut self) -> RevMutEntries<'a, K, V> {
        RevMutEntries{iter: self.mut_iter()}
    }


    /// Get a lazy iterator that consumes the treemap.
    pub fn move_iter(self) -> MoveEntries<K, V> {
        let TreeMap { root: root, length: length } = self;
        let stk = match root {
            None => ~[],
            Some(~tn) => ~[tn]
        };
        MoveEntries {
            stack: stk,
            remaining: length
        }
    }
}

// range iterators.

macro_rules! bound_setup {
    // initialiser of the iterator to manipulate
    ($iter:expr,
     // whether we are looking for the lower or upper bound.
     $is_lower_bound:expr) => {
        {
            let mut iter = $iter;
            loop {
                if !iter.node.is_null() {
                    let node_k = unsafe {&(*iter.node).key};
                    match k.cmp(node_k) {
                        Less => iter.traverse_left(),
                        Greater => iter.traverse_right(),
                        Equal => {
                            if $is_lower_bound {
                                iter.traverse_complete();
                                return iter;
                            } else {
                                iter.traverse_right()
                            }
                        }
                    }
                } else {
                    iter.traverse_complete();
                    return iter;
                }
            }
        }
    }
}


impl<K: TotalOrd, V> TreeMap<K, V> {
    /// Get a lazy iterator that should be initialized using
    /// `traverse_left`/`traverse_right`/`traverse_complete`.
    fn iter_for_traversal<'a>(&'a self) -> Entries<'a, K, V> {
        Entries {
            stack: ~[],
            node: deref(&self.root),
            remaining_min: 0,
            remaining_max: self.length
        }
    }

    /// Return a lazy iterator to the first key-value pair whose key is not less than `k`
    /// If all keys in map are less than `k` an empty iterator is returned.
    pub fn lower_bound<'a>(&'a self, k: &K) -> Entries<'a, K, V> {
        bound_setup!(self.iter_for_traversal(), true)
    }

    /// Return a lazy iterator to the first key-value pair whose key is greater than `k`
    /// If all keys in map are not greater than `k` an empty iterator is returned.
    pub fn upper_bound<'a>(&'a self, k: &K) -> Entries<'a, K, V> {
        bound_setup!(self.iter_for_traversal(), false)
    }

    /// Get a lazy iterator that should be initialized using
    /// `traverse_left`/`traverse_right`/`traverse_complete`.
    fn mut_iter_for_traversal<'a>(&'a mut self) -> MutEntries<'a, K, V> {
        MutEntries {
            stack: ~[],
            node: mut_deref(&mut self.root),
            remaining_min: 0,
            remaining_max: self.length
        }
    }

    /// Return a lazy value iterator to the first key-value pair (with
    /// the value being mutable) whose key is not less than `k`.
    ///
    /// If all keys in map are less than `k` an empty iterator is
    /// returned.
    pub fn mut_lower_bound<'a>(&'a mut self, k: &K) -> MutEntries<'a, K, V> {
        bound_setup!(self.mut_iter_for_traversal(), true)
    }

    /// Return a lazy iterator to the first key-value pair (with the
    /// value being mutable) whose key is greater than `k`.
    ///
    /// If all keys in map are not greater than `k` an empty iterator
    /// is returned.
    pub fn mut_upper_bound<'a>(&'a mut self, k: &K) -> MutEntries<'a, K, V> {
        bound_setup!(self.mut_iter_for_traversal(), false)
    }
}

/// Lazy forward iterator over a map
pub struct Entries<'a, K, V> {
    priv stack: ~[&'a TreeNode<K, V>],
    // See the comment on MutEntries; this is just to allow
    // code-sharing (for this immutable-values iterator it *could* very
    // well be Option<&'a TreeNode<K,V>>).
    priv node: *TreeNode<K, V>,
    priv remaining_min: uint,
    priv remaining_max: uint
}

/// Lazy backward iterator over a map
pub struct RevEntries<'a, K, V> {
    priv iter: Entries<'a, K, V>,
}

/// Lazy forward iterator over a map that allows for the mutation of
/// the values.
pub struct MutEntries<'a, K, V> {
    priv stack: ~[&'a mut TreeNode<K, V>],
    // Unfortunately, we require some unsafe-ness to get around the
    // fact that we would be storing a reference *into* one of the
    // nodes in the stack.
    //
    // As far as the compiler knows, this would let us invalidate the
    // reference by assigning a new value to this node's position in
    // its parent, which would cause this current one to be
    // deallocated so this reference would be invalid. (i.e. the
    // compilers complaints are 100% correct.)
    //
    // However, as far as you humans reading this code know (or are
    // about to know, if you haven't read far enough down yet), we are
    // only reading from the TreeNode.{left,right} fields. the only
    // thing that is ever mutated is the .value field (although any
    // actual mutation that happens is done externally, by the
    // iterator consumer). So, don't be so concerned, rustc, we've got
    // it under control.
    //
    // (This field can legitimately be null.)
    priv node: *mut TreeNode<K, V>,
    priv remaining_min: uint,
    priv remaining_max: uint
}

/// Lazy backward iterator over a map
pub struct RevMutEntries<'a, K, V> {
    priv iter: MutEntries<'a, K, V>,
}


// FIXME #5846 we want to be able to choose between &x and &mut x
// (with many different `x`) below, so we need to optionally pass mut
// as a tt, but the only thing we can do with a `tt` is pass them to
// other macros, so this takes the `& <mutability> <operand>` token
// sequence and forces their evalutation as an expression.
macro_rules! addr { ($e:expr) => { $e }}
// putting an optional mut into type signatures
macro_rules! item { ($i:item) => { $i }}

macro_rules! define_iterator {
    ($name:ident,
     $rev_name:ident,

     // the function to go from &m Option<~TreeNode> to *m TreeNode
     deref = $deref:ident,

     // see comment on `addr!`, this is just an optional `mut`, but
     // there's no support for 0-or-1 repeats.
     addr_mut = $($addr_mut:tt)*
     ) => {
        // private methods on the forward iterator (item!() for the
        // addr_mut in the next_ return value)
        item!(impl<'a, K, V> $name<'a, K, V> {
            #[inline(always)]
            fn next_(&mut self, forward: bool) -> Option<(&'a K, &'a $($addr_mut)* V)> {
                while !self.stack.is_empty() || !self.node.is_null() {
                    if !self.node.is_null() {
                        let node = unsafe {addr!(& $($addr_mut)* *self.node)};
                        {
                            let next_node = if forward {
                                addr!(& $($addr_mut)* node.left)
                            } else {
                                addr!(& $($addr_mut)* node.right)
                            };
                            self.node = $deref(next_node);
                        }
                        self.stack.push(node);
                    } else {
                        let node = self.stack.pop().unwrap();
                        let next_node = if forward {
                            addr!(& $($addr_mut)* node.right)
                        } else {
                            addr!(& $($addr_mut)* node.left)
                        };
                        self.node = $deref(next_node);
                        self.remaining_max -= 1;
                        if self.remaining_min > 0 {
                            self.remaining_min -= 1;
                        }
                        return Some((&node.key, addr!(& $($addr_mut)* node.value)));
                    }
                }
                None
            }

            /// traverse_left, traverse_right and traverse_complete are
            /// used to initialize Entries/MutEntries
            /// pointing to element inside tree structure.
            ///
            /// They should be used in following manner:
            ///   - create iterator using TreeMap::[mut_]iter_for_traversal
            ///   - find required node using `traverse_left`/`traverse_right`
            ///     (current node is `Entries::node` field)
            ///   - complete initialization with `traverse_complete`
            ///
            /// After this, iteration will start from `self.node`.  If
            /// `self.node` is None iteration will start from last
            /// node from which we traversed left.
            #[inline]
            fn traverse_left(&mut self) {
                let node = unsafe {addr!(& $($addr_mut)* *self.node)};
                self.node = $deref(addr!(& $($addr_mut)* node.left));
                self.stack.push(node);
            }

            #[inline]
            fn traverse_right(&mut self) {
                let node = unsafe {addr!(& $($addr_mut)* *self.node)};
                self.node = $deref(addr!(& $($addr_mut)* node.right));
            }

            #[inline]
            fn traverse_complete(&mut self) {
                if !self.node.is_null() {
                    unsafe {
                        self.stack.push(addr!(& $($addr_mut)* *self.node));
                    }
                    self.node = ptr::RawPtr::null();
                }
            }
        })

        // the forward Iterator impl.
        item!(impl<'a, K, V> Iterator<(&'a K, &'a $($addr_mut)* V)> for $name<'a, K, V> {
            /// Advance the iterator to the next node (in order) and return a
            /// tuple with a reference to the key and value. If there are no
            /// more nodes, return `None`.
            fn next(&mut self) -> Option<(&'a K, &'a $($addr_mut)* V)> {
                self.next_(true)
            }

            #[inline]
            fn size_hint(&self) -> (uint, Option<uint>) {
                (self.remaining_min, Some(self.remaining_max))
            }
        })

        // the reverse Iterator impl.
        item!(impl<'a, K, V> Iterator<(&'a K, &'a $($addr_mut)* V)> for $rev_name<'a, K, V> {
            fn next(&mut self) -> Option<(&'a K, &'a $($addr_mut)* V)> {
                self.iter.next_(false)
            }

            #[inline]
            fn size_hint(&self) -> (uint, Option<uint>) {
                self.iter.size_hint()
            }
        })
    }
} // end of define_iterator

define_iterator! {
    Entries,
    RevEntries,
    deref = deref,

    // immutable, so no mut
    addr_mut =
}
define_iterator! {
    MutEntries,
    RevMutEntries,
    deref = mut_deref,

    addr_mut = mut
}

fn deref<'a, K, V>(node: &'a Option<~TreeNode<K, V>>) -> *TreeNode<K, V> {
    match *node {
        Some(ref n) => {
            let n: &TreeNode<K, V> = *n;
            n as *TreeNode<K, V>
        }
        None => ptr::null()
    }
}

fn mut_deref<K, V>(x: &mut Option<~TreeNode<K, V>>) -> *mut TreeNode<K, V> {
    match *x {
        Some(ref mut n) => {
            let n: &mut TreeNode<K, V> = *n;
            n as *mut TreeNode<K, V>
        }
        None => ptr::mut_null()
    }
}



/// Lazy forward iterator over a map that consumes the map while iterating
pub struct MoveEntries<K, V> {
    priv stack: ~[TreeNode<K, V>],
    priv remaining: uint
}

impl<K, V> Iterator<(K, V)> for MoveEntries<K,V> {
    #[inline]
    fn next(&mut self) -> Option<(K, V)> {
        while !self.stack.is_empty() {
            let TreeNode {
                key: key,
                value: value,
                left: left,
                right: right,
                level: level
            } = self.stack.pop().unwrap();

            match left {
                Some(~left) => {
                    let n = TreeNode {
                        key: key,
                        value: value,
                        left: None,
                        right: right,
                        level: level
                    };
                    self.stack.push(n);
                    self.stack.push(left);
                }
                None => {
                    match right {
                        Some(~right) => self.stack.push(right),
                        None => ()
                    }
                    self.remaining -= 1;
                    return Some((key, value))
                }
            }
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (self.remaining, Some(self.remaining))
    }

}

impl<'a, T> Iterator<&'a T> for SetItems<'a, T> {
    /// Advance the iterator to the next node (in order). If there are no more nodes, return `None`.
    #[inline]
    fn next(&mut self) -> Option<&'a T> {
        self.iter.next().map(|(value, _)| value)
    }
}

impl<'a, T> Iterator<&'a T> for RevSetItems<'a, T> {
    /// Advance the iterator to the next node (in order). If there are no more nodes, return `None`.
    #[inline]
    fn next(&mut self) -> Option<&'a T> {
        self.iter.next().map(|(value, _)| value)
    }
}

/// A implementation of the `Set` trait on top of the `TreeMap` container. The
/// only requirement is that the type of the elements contained ascribes to the
/// `TotalOrd` trait.
#[deriving(Clone)]
pub struct TreeSet<T> {
    priv map: TreeMap<T, ()>
}

impl<T: Eq + TotalOrd> Eq for TreeSet<T> {
    #[inline]
    fn eq(&self, other: &TreeSet<T>) -> bool { self.map == other.map }
    #[inline]
    fn ne(&self, other: &TreeSet<T>) -> bool { self.map != other.map }
}

impl<T: Ord + TotalOrd> Ord for TreeSet<T> {
    #[inline]
    fn lt(&self, other: &TreeSet<T>) -> bool { self.map < other.map }
    #[inline]
    fn le(&self, other: &TreeSet<T>) -> bool { self.map <= other.map }
    #[inline]
    fn ge(&self, other: &TreeSet<T>) -> bool { self.map >= other.map }
    #[inline]
    fn gt(&self, other: &TreeSet<T>) -> bool { self.map > other.map }
}

impl<T: TotalOrd> Container for TreeSet<T> {
    /// Return the number of elements in the set
    #[inline]
    fn len(&self) -> uint { self.map.len() }

    /// Return true if the set contains no elements
    #[inline]
    fn is_empty(&self) -> bool { self.map.is_empty() }
}

impl<T: TotalOrd> Mutable for TreeSet<T> {
    /// Clear the set, removing all values.
    #[inline]
    fn clear(&mut self) { self.map.clear() }
}

impl<T: TotalOrd> Set<T> for TreeSet<T> {
    /// Return true if the set contains a value
    #[inline]
    fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    /// Return true if the set has no elements in common with `other`.
    /// This is equivalent to checking for an empty intersection.
    fn is_disjoint(&self, other: &TreeSet<T>) -> bool {
        self.intersection(other).next().is_none()
    }

    /// Return true if the set is a subset of another
    #[inline]
    fn is_subset(&self, other: &TreeSet<T>) -> bool {
        other.is_superset(self)
    }

    /// Return true if the set is a superset of another
    fn is_superset(&self, other: &TreeSet<T>) -> bool {
        let mut x = self.iter();
        let mut y = other.iter();
        let mut a = x.next();
        let mut b = y.next();
        while b.is_some() {
            if a.is_none() {
                return false
            }

            let a1 = a.unwrap();
            let b1 = b.unwrap();

            match a1.cmp(b1) {
              Less => (),
              Greater => return false,
              Equal => b = y.next(),
            }

            a = x.next();
        }
        true
    }
}

impl<T: TotalOrd> MutableSet<T> for TreeSet<T> {
    /// Add a value to the set. Return true if the value was not already
    /// present in the set.
    #[inline]
    fn insert(&mut self, value: T) -> bool { self.map.insert(value, ()) }

    /// Remove a value from the set. Return true if the value was
    /// present in the set.
    #[inline]
    fn remove(&mut self, value: &T) -> bool { self.map.remove(value) }
}

impl<T: TotalOrd> TreeSet<T> {
    /// Create an empty TreeSet
    #[inline]
    pub fn new() -> TreeSet<T> { TreeSet{map: TreeMap::new()} }

    /// Get a lazy iterator over the values in the set.
    /// Requires that it be frozen (immutable).
    #[inline]
    pub fn iter<'a>(&'a self) -> SetItems<'a, T> {
        SetItems{iter: self.map.iter()}
    }

    /// Get a lazy iterator over the values in the set.
    /// Requires that it be frozen (immutable).
    #[inline]
    pub fn rev_iter<'a>(&'a self) -> RevSetItems<'a, T> {
        RevSetItems{iter: self.map.rev_iter()}
    }

    /// Get a lazy iterator pointing to the first value not less than `v` (greater or equal).
    /// If all elements in the set are less than `v` empty iterator is returned.
    #[inline]
    pub fn lower_bound<'a>(&'a self, v: &T) -> SetItems<'a, T> {
        SetItems{iter: self.map.lower_bound(v)}
    }

    /// Get a lazy iterator pointing to the first value greater than `v`.
    /// If all elements in the set are not greater than `v` empty iterator is returned.
    #[inline]
    pub fn upper_bound<'a>(&'a self, v: &T) -> SetItems<'a, T> {
        SetItems{iter: self.map.upper_bound(v)}
    }

    /// Visit the values (in-order) representing the difference
    pub fn difference<'a>(&'a self, other: &'a TreeSet<T>) -> DifferenceItems<'a, T> {
        DifferenceItems{a: self.iter().peekable(), b: other.iter().peekable()}
    }

    /// Visit the values (in-order) representing the symmetric difference
    pub fn symmetric_difference<'a>(&'a self, other: &'a TreeSet<T>)
        -> SymDifferenceItems<'a, T> {
        SymDifferenceItems{a: self.iter().peekable(), b: other.iter().peekable()}
    }

    /// Visit the values (in-order) representing the intersection
    pub fn intersection<'a>(&'a self, other: &'a TreeSet<T>)
        -> IntersectionItems<'a, T> {
        IntersectionItems{a: self.iter().peekable(), b: other.iter().peekable()}
    }

    /// Visit the values (in-order) representing the union
    pub fn union<'a>(&'a self, other: &'a TreeSet<T>) -> UnionItems<'a, T> {
        UnionItems{a: self.iter().peekable(), b: other.iter().peekable()}
    }
}

/// Lazy forward iterator over a set
pub struct SetItems<'a, T> {
    priv iter: Entries<'a, T, ()>
}

/// Lazy backward iterator over a set
pub struct RevSetItems<'a, T> {
    priv iter: RevEntries<'a, T, ()>
}

/// Lazy iterator producing elements in the set difference (in-order)
pub struct DifferenceItems<'a, T> {
    priv a: Peekable<&'a T, SetItems<'a, T>>,
    priv b: Peekable<&'a T, SetItems<'a, T>>,
}

/// Lazy iterator producing elements in the set symmetric difference (in-order)
pub struct SymDifferenceItems<'a, T> {
    priv a: Peekable<&'a T, SetItems<'a, T>>,
    priv b: Peekable<&'a T, SetItems<'a, T>>,
}

/// Lazy iterator producing elements in the set intersection (in-order)
pub struct IntersectionItems<'a, T> {
    priv a: Peekable<&'a T, SetItems<'a, T>>,
    priv b: Peekable<&'a T, SetItems<'a, T>>,
}

/// Lazy iterator producing elements in the set intersection (in-order)
pub struct UnionItems<'a, T> {
    priv a: Peekable<&'a T, SetItems<'a, T>>,
    priv b: Peekable<&'a T, SetItems<'a, T>>,
}

/// Compare `x` and `y`, but return `short` if x is None and `long` if y is None
fn cmp_opt<T: TotalOrd>(x: Option<&T>, y: Option<&T>,
                        short: Ordering, long: Ordering) -> Ordering {
    match (x, y) {
        (None    , _       ) => short,
        (_       , None    ) => long,
        (Some(x1), Some(y1)) => x1.cmp(y1),
    }
}

impl<'a, T: TotalOrd> Iterator<&'a T> for DifferenceItems<'a, T> {
    fn next(&mut self) -> Option<&'a T> {
        loop {
            match cmp_opt(self.a.peek(), self.b.peek(), Less, Less) {
                Less    => return self.a.next(),
                Equal   => { self.a.next(); self.b.next(); }
                Greater => { self.b.next(); }
            }
        }
    }
}

impl<'a, T: TotalOrd> Iterator<&'a T> for SymDifferenceItems<'a, T> {
    fn next(&mut self) -> Option<&'a T> {
        loop {
            match cmp_opt(self.a.peek(), self.b.peek(), Greater, Less) {
                Less    => return self.a.next(),
                Equal   => { self.a.next(); self.b.next(); }
                Greater => return self.b.next(),
            }
        }
    }
}

impl<'a, T: TotalOrd> Iterator<&'a T> for IntersectionItems<'a, T> {
    fn next(&mut self) -> Option<&'a T> {
        loop {
            let o_cmp = match (self.a.peek(), self.b.peek()) {
                (None    , _       ) => None,
                (_       , None    ) => None,
                (Some(a1), Some(b1)) => Some(a1.cmp(b1)),
            };
            match o_cmp {
                None          => return None,
                Some(Less)    => { self.a.next(); }
                Some(Equal)   => { self.b.next(); return self.a.next() }
                Some(Greater) => { self.b.next(); }
            }
        }
    }
}

impl<'a, T: TotalOrd> Iterator<&'a T> for UnionItems<'a, T> {
    fn next(&mut self) -> Option<&'a T> {
        loop {
            match cmp_opt(self.a.peek(), self.b.peek(), Greater, Less) {
                Less    => return self.a.next(),
                Equal   => { self.b.next(); return self.a.next() }
                Greater => return self.b.next(),
            }
        }
    }
}


// Nodes keep track of their level in the tree, starting at 1 in the
// leaves and with a red child sharing the level of the parent.
#[deriving(Clone)]
struct TreeNode<K, V> {
    key: K,
    value: V,
    left: Option<~TreeNode<K, V>>,
    right: Option<~TreeNode<K, V>>,
    level: uint
}

impl<K: TotalOrd, V> TreeNode<K, V> {
    /// Creates a new tree node.
    #[inline]
    pub fn new(key: K, value: V) -> TreeNode<K, V> {
        TreeNode{key: key, value: value, left: None, right: None, level: 1}
    }
}

// Remove left horizontal link by rotating right
fn skew<K: TotalOrd, V>(node: &mut ~TreeNode<K, V>) {
    if node.left.as_ref().map_or(false, |x| x.level == node.level) {
        let mut save = node.left.take_unwrap();
        swap(&mut node.left, &mut save.right); // save.right now None
        swap(node, &mut save);
        node.right = Some(save);
    }
}

// Remove dual horizontal link by rotating left and increasing level of
// the parent
fn split<K: TotalOrd, V>(node: &mut ~TreeNode<K, V>) {
    if node.right.as_ref().map_or(false,
      |x| x.right.as_ref().map_or(false, |y| y.level == node.level)) {
        let mut save = node.right.take_unwrap();
        swap(&mut node.right, &mut save.left); // save.left now None
        save.level += 1;
        swap(node, &mut save);
        node.left = Some(save);
    }
}

fn find_mut<'r, K: TotalOrd, V>(node: &'r mut Option<~TreeNode<K, V>>,
                                key: &K)
                             -> Option<&'r mut V> {
    match *node {
      Some(ref mut x) => {
        match key.cmp(&x.key) {
          Less => find_mut(&mut x.left, key),
          Greater => find_mut(&mut x.right, key),
          Equal => Some(&mut x.value),
        }
      }
      None => None
    }
}

fn insert<K: TotalOrd, V>(node: &mut Option<~TreeNode<K, V>>,
                          key: K, value: V) -> Option<V> {
    match *node {
      Some(ref mut save) => {
        match key.cmp(&save.key) {
          Less => {
            let inserted = insert(&mut save.left, key, value);
            skew(save);
            split(save);
            inserted
          }
          Greater => {
            let inserted = insert(&mut save.right, key, value);
            skew(save);
            split(save);
            inserted
          }
          Equal => {
            save.key = key;
            Some(replace(&mut save.value, value))
          }
        }
      }
      None => {
       *node = Some(~TreeNode::new(key, value));
        None
      }
    }
}

fn remove<K: TotalOrd, V>(node: &mut Option<~TreeNode<K, V>>,
                          key: &K) -> Option<V> {
    fn heir_swap<K: TotalOrd, V>(node: &mut ~TreeNode<K, V>,
                                 child: &mut Option<~TreeNode<K, V>>) {
        // *could* be done without recursion, but it won't borrow check
        for x in child.mut_iter() {
            if x.right.is_some() {
                heir_swap(node, &mut x.right);
            } else {
                swap(&mut node.key, &mut x.key);
                swap(&mut node.value, &mut x.value);
            }
        }
    }

    match *node {
      None => {
        return None; // bottom of tree
      }
      Some(ref mut save) => {
        let (ret, rebalance) = match key.cmp(&save.key) {
          Less => (remove(&mut save.left, key), true),
          Greater => (remove(&mut save.right, key), true),
          Equal => {
            if save.left.is_some() {
                if save.right.is_some() {
                    let mut left = save.left.take_unwrap();
                    if left.right.is_some() {
                        heir_swap(save, &mut left.right);
                    } else {
                        swap(&mut save.key, &mut left.key);
                        swap(&mut save.value, &mut left.value);
                    }
                    save.left = Some(left);
                    (remove(&mut save.left, key), true)
                } else {
                    let new = save.left.take_unwrap();
                    let ~TreeNode{value, ..} = replace(save, new);
                    *save = save.left.take_unwrap();
                    (Some(value), true)
                }
            } else if save.right.is_some() {
                let new = save.right.take_unwrap();
                let ~TreeNode{value, ..} = replace(save, new);
                (Some(value), true)
            } else {
                (None, false)
            }
          }
        };

        if rebalance {
            let left_level = save.left.as_ref().map_or(0, |x| x.level);
            let right_level = save.right.as_ref().map_or(0, |x| x.level);

            // re-balance, if necessary
            if left_level < save.level - 1 || right_level < save.level - 1 {
                save.level -= 1;

                if right_level > save.level {
                    for x in save.right.mut_iter() { x.level = save.level }
                }

                skew(save);

                for right in save.right.mut_iter() {
                    skew(right);
                    for x in right.right.mut_iter() { skew(x) }
                }

                split(save);
                for x in save.right.mut_iter() { split(x) }
            }

            return ret;
        }
      }
    }
    return match node.take() {
        Some(~TreeNode{value, ..}) => Some(value), None => fail!()
    };
}

impl<K: TotalOrd, V> FromIterator<(K, V)> for TreeMap<K, V> {
    fn from_iterator<T: Iterator<(K, V)>>(iter: &mut T) -> TreeMap<K, V> {
        let mut map = TreeMap::new();
        map.extend(iter);
        map
    }
}

impl<K: TotalOrd, V> Extendable<(K, V)> for TreeMap<K, V> {
    #[inline]
    fn extend<T: Iterator<(K, V)>>(&mut self, iter: &mut T) {
        for (k, v) in *iter {
            self.insert(k, v);
        }
    }
}

impl<T: TotalOrd> FromIterator<T> for TreeSet<T> {
    fn from_iterator<Iter: Iterator<T>>(iter: &mut Iter) -> TreeSet<T> {
        let mut set = TreeSet::new();
        set.extend(iter);
        set
    }
}

impl<T: TotalOrd> Extendable<T> for TreeSet<T> {
    #[inline]
    fn extend<Iter: Iterator<T>>(&mut self, iter: &mut Iter) {
        for elem in *iter {
            self.insert(elem);
        }
    }
}

impl<
    E: Encoder,
    K: Encodable<E> + Eq + TotalOrd,
    V: Encodable<E> + Eq
> Encodable<E> for TreeMap<K, V> {
    fn encode(&self, e: &mut E) {
        e.emit_map(self.len(), |e| {
            let mut i = 0;
            for (key, val) in self.iter() {
                e.emit_map_elt_key(i, |e| key.encode(e));
                e.emit_map_elt_val(i, |e| val.encode(e));
                i += 1;
            }
        })
    }
}

impl<
    D: Decoder,
    K: Decodable<D> + Eq + TotalOrd,
    V: Decodable<D> + Eq
> Decodable<D> for TreeMap<K, V> {
    fn decode(d: &mut D) -> TreeMap<K, V> {
        d.read_map(|d, len| {
            let mut map = TreeMap::new();
            for i in range(0u, len) {
                let key = d.read_map_elt_key(i, |d| Decodable::decode(d));
                let val = d.read_map_elt_val(i, |d| Decodable::decode(d));
                map.insert(key, val);
            }
            map
        })
    }
}

impl<
    S: Encoder,
    T: Encodable<S> + Eq + TotalOrd
> Encodable<S> for TreeSet<T> {
    fn encode(&self, s: &mut S) {
        s.emit_seq(self.len(), |s| {
            let mut i = 0;
            for e in self.iter() {
                s.emit_seq_elt(i, |s| e.encode(s));
                i += 1;
            }
        })
    }
}

impl<
    D: Decoder,
    T: Decodable<D> + Eq + TotalOrd
> Decodable<D> for TreeSet<T> {
    fn decode(d: &mut D) -> TreeSet<T> {
        d.read_seq(|d, len| {
            let mut set = TreeSet::new();
            for i in range(0u, len) {
                set.insert(d.read_seq_elt(i, |d| Decodable::decode(d)));
            }
            set
        })
    }
}

#[cfg(test)]
mod test_treemap {

    use super::{TreeMap, TreeNode};

    use std::rand::Rng;
    use std::rand;

    #[test]
    fn find_empty() {
        let m: TreeMap<int,int> = TreeMap::new();
        assert!(m.find(&5) == None);
    }

    #[test]
    fn find_not_found() {
        let mut m = TreeMap::new();
        assert!(m.insert(1, 2));
        assert!(m.insert(5, 3));
        assert!(m.insert(9, 3));
        assert_eq!(m.find(&2), None);
    }

    #[test]
    fn test_find_mut() {
        let mut m = TreeMap::new();
        assert!(m.insert(1, 12));
        assert!(m.insert(2, 8));
        assert!(m.insert(5, 14));
        let new = 100;
        match m.find_mut(&5) {
          None => fail!(), Some(x) => *x = new
        }
        assert_eq!(m.find(&5), Some(&new));
    }

    #[test]
    fn insert_replace() {
        let mut m = TreeMap::new();
        assert!(m.insert(5, 2));
        assert!(m.insert(2, 9));
        assert!(!m.insert(2, 11));
        assert_eq!(m.find(&2).unwrap(), &11);
    }

    #[test]
    fn test_clear() {
        let mut m = TreeMap::new();
        m.clear();
        assert!(m.insert(5, 11));
        assert!(m.insert(12, -3));
        assert!(m.insert(19, 2));
        m.clear();
        assert!(m.find(&5).is_none());
        assert!(m.find(&12).is_none());
        assert!(m.find(&19).is_none());
        assert!(m.is_empty());
    }

    #[test]
    fn u8_map() {
        let mut m = TreeMap::new();

        let k1 = "foo".as_bytes();
        let k2 = "bar".as_bytes();
        let v1 = "baz".as_bytes();
        let v2 = "foobar".as_bytes();

        m.insert(k1.clone(), v1.clone());
        m.insert(k2.clone(), v2.clone());

        assert_eq!(m.find(&k2), Some(&v2));
        assert_eq!(m.find(&k1), Some(&v1));
    }

    fn check_equal<K: Eq + TotalOrd, V: Eq>(ctrl: &[(K, V)],
                                            map: &TreeMap<K, V>) {
        assert_eq!(ctrl.is_empty(), map.is_empty());
        for x in ctrl.iter() {
            let &(ref k, ref v) = x;
            assert!(map.find(k).unwrap() == v)
        }
        for (map_k, map_v) in map.iter() {
            let mut found = false;
            for x in ctrl.iter() {
                let &(ref ctrl_k, ref ctrl_v) = x;
                if *map_k == *ctrl_k {
                    assert!(*map_v == *ctrl_v);
                    found = true;
                    break;
                }
            }
            assert!(found);
        }
    }

    fn check_left<K: TotalOrd, V>(node: &Option<~TreeNode<K, V>>,
                                  parent: &~TreeNode<K, V>) {
        match *node {
          Some(ref r) => {
            assert_eq!(r.key.cmp(&parent.key), Less);
            assert!(r.level == parent.level - 1); // left is black
            check_left(&r.left, r);
            check_right(&r.right, r, false);
          }
          None => assert!(parent.level == 1) // parent is leaf
        }
    }

    fn check_right<K: TotalOrd, V>(node: &Option<~TreeNode<K, V>>,
                                   parent: &~TreeNode<K, V>,
                                   parent_red: bool) {
        match *node {
          Some(ref r) => {
            assert_eq!(r.key.cmp(&parent.key), Greater);
            let red = r.level == parent.level;
            if parent_red { assert!(!red) } // no dual horizontal links
            // Right red or black
            assert!(red || r.level == parent.level - 1);
            check_left(&r.left, r);
            check_right(&r.right, r, red);
          }
          None => assert!(parent.level == 1) // parent is leaf
        }
    }

    fn check_structure<K: TotalOrd, V>(map: &TreeMap<K, V>) {
        match map.root {
          Some(ref r) => {
            check_left(&r.left, r);
            check_right(&r.right, r, false);
          }
          None => ()
        }
    }

    #[test]
    fn test_rand_int() {
        let mut map: TreeMap<int,int> = TreeMap::new();
        let mut ctrl = ~[];

        check_equal(ctrl, &map);
        assert!(map.find(&5).is_none());

        let mut rng: rand::IsaacRng = rand::SeedableRng::from_seed(&[42]);

        for _ in range(0, 3) {
            for _ in range(0, 90) {
                let k = rng.gen();
                let v = rng.gen();
                if !ctrl.iter().any(|x| x == &(k, v)) {
                    assert!(map.insert(k, v));
                    ctrl.push((k, v));
                    check_structure(&map);
                    check_equal(ctrl, &map);
                }
            }

            for _ in range(0, 30) {
                let r = rng.gen_range(0, ctrl.len());
                let (key, _) = ctrl.remove(r).unwrap();
                assert!(map.remove(&key));
                check_structure(&map);
                check_equal(ctrl, &map);
            }
        }
    }

    #[test]
    fn test_len() {
        let mut m = TreeMap::new();
        assert!(m.insert(3, 6));
        assert_eq!(m.len(), 1);
        assert!(m.insert(0, 0));
        assert_eq!(m.len(), 2);
        assert!(m.insert(4, 8));
        assert_eq!(m.len(), 3);
        assert!(m.remove(&3));
        assert_eq!(m.len(), 2);
        assert!(!m.remove(&5));
        assert_eq!(m.len(), 2);
        assert!(m.insert(2, 4));
        assert_eq!(m.len(), 3);
        assert!(m.insert(1, 2));
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn test_iterator() {
        let mut m = TreeMap::new();

        assert!(m.insert(3, 6));
        assert!(m.insert(0, 0));
        assert!(m.insert(4, 8));
        assert!(m.insert(2, 4));
        assert!(m.insert(1, 2));

        let mut n = 0;
        for (k, v) in m.iter() {
            assert_eq!(*k, n);
            assert_eq!(*v, n * 2);
            n += 1;
        }
        assert_eq!(n, 5);
    }

    #[test]
    fn test_interval_iteration() {
        let mut m = TreeMap::new();
        for i in range(1, 100) {
            assert!(m.insert(i * 2, i * 4));
        }

        for i in range(1, 198) {
            let mut lb_it = m.lower_bound(&i);
            let (&k, &v) = lb_it.next().unwrap();
            let lb = i + i % 2;
            assert_eq!(lb, k);
            assert_eq!(lb * 2, v);

            let mut ub_it = m.upper_bound(&i);
            let (&k, &v) = ub_it.next().unwrap();
            let ub = i + 2 - i % 2;
            assert_eq!(ub, k);
            assert_eq!(ub * 2, v);
        }
        let mut end_it = m.lower_bound(&199);
        assert_eq!(end_it.next(), None);
    }

    #[test]
    fn test_rev_iter() {
        let mut m = TreeMap::new();

        assert!(m.insert(3, 6));
        assert!(m.insert(0, 0));
        assert!(m.insert(4, 8));
        assert!(m.insert(2, 4));
        assert!(m.insert(1, 2));

        let mut n = 4;
        for (k, v) in m.rev_iter() {
            assert_eq!(*k, n);
            assert_eq!(*v, n * 2);
            n -= 1;
        }
    }

    #[test]
    fn test_mut_iter() {
        let mut m = TreeMap::new();
        for i in range(0u, 10) {
            assert!(m.insert(i, 100 * i));
        }

        for (i, (&k, v)) in m.mut_iter().enumerate() {
            *v += k * 10 + i; // 000 + 00 + 0, 100 + 10 + 1, ...
        }

        for (&k, &v) in m.iter() {
            assert_eq!(v, 111 * k);
        }
    }
    #[test]
    fn test_mut_rev_iter() {
        let mut m = TreeMap::new();
        for i in range(0u, 10) {
            assert!(m.insert(i, 100 * i));
        }

        for (i, (&k, v)) in m.mut_rev_iter().enumerate() {
            *v += k * 10 + (9 - i); // 900 + 90 + (9 - 0), 800 + 80 + (9 - 1), ...
        }

        for (&k, &v) in m.iter() {
            assert_eq!(v, 111 * k);
        }
    }

    #[test]
    fn test_mut_interval_iter() {
        let mut m_lower = TreeMap::new();
        let mut m_upper = TreeMap::new();
        for i in range(1, 100) {
            assert!(m_lower.insert(i * 2, i * 4));
            assert!(m_upper.insert(i * 2, i * 4));
        }

        for i in range(1, 199) {
            let mut lb_it = m_lower.mut_lower_bound(&i);
            let (&k, v) = lb_it.next().unwrap();
            let lb = i + i % 2;
            assert_eq!(lb, k);
            *v -= k;
        }
        for i in range(0, 198) {
            let mut ub_it = m_upper.mut_upper_bound(&i);
            let (&k, v) = ub_it.next().unwrap();
            let ub = i + 2 - i % 2;
            assert_eq!(ub, k);
            *v -= k;
        }

        assert!(m_lower.mut_lower_bound(&199).next().is_none());

        assert!(m_upper.mut_upper_bound(&198).next().is_none());

        assert!(m_lower.iter().all(|(_, &x)| x == 0));
        assert!(m_upper.iter().all(|(_, &x)| x == 0));
    }

    #[test]
    fn test_eq() {
        let mut a = TreeMap::new();
        let mut b = TreeMap::new();

        assert!(a == b);
        assert!(a.insert(0, 5));
        assert!(a != b);
        assert!(b.insert(0, 4));
        assert!(a != b);
        assert!(a.insert(5, 19));
        assert!(a != b);
        assert!(!b.insert(0, 5));
        assert!(a != b);
        assert!(b.insert(5, 19));
        assert!(a == b);
    }

    #[test]
    fn test_lt() {
        let mut a = TreeMap::new();
        let mut b = TreeMap::new();

        assert!(!(a < b) && !(b < a));
        assert!(b.insert(0, 5));
        assert!(a < b);
        assert!(a.insert(0, 7));
        assert!(!(a < b) && b < a);
        assert!(b.insert(-2, 0));
        assert!(b < a);
        assert!(a.insert(-5, 2));
        assert!(a < b);
        assert!(a.insert(6, 2));
        assert!(a < b && !(b < a));
    }

    #[test]
    fn test_ord() {
        let mut a = TreeMap::new();
        let mut b = TreeMap::new();

        assert!(a <= b && a >= b);
        assert!(a.insert(1, 1));
        assert!(a > b && a >= b);
        assert!(b < a && b <= a);
        assert!(b.insert(2, 2));
        assert!(b > a && b >= a);
        assert!(a < b && a <= b);
    }

    #[test]
    fn test_lazy_iterator() {
        let mut m = TreeMap::new();
        let (x1, y1) = (2, 5);
        let (x2, y2) = (9, 12);
        let (x3, y3) = (20, -3);
        let (x4, y4) = (29, 5);
        let (x5, y5) = (103, 3);

        assert!(m.insert(x1, y1));
        assert!(m.insert(x2, y2));
        assert!(m.insert(x3, y3));
        assert!(m.insert(x4, y4));
        assert!(m.insert(x5, y5));

        let m = m;
        let mut a = m.iter();

        assert_eq!(a.next().unwrap(), (&x1, &y1));
        assert_eq!(a.next().unwrap(), (&x2, &y2));
        assert_eq!(a.next().unwrap(), (&x3, &y3));
        assert_eq!(a.next().unwrap(), (&x4, &y4));
        assert_eq!(a.next().unwrap(), (&x5, &y5));

        assert!(a.next().is_none());

        let mut b = m.iter();

        let expected = [(&x1, &y1), (&x2, &y2), (&x3, &y3), (&x4, &y4),
                        (&x5, &y5)];
        let mut i = 0;

        for x in b {
            assert_eq!(expected[i], x);
            i += 1;

            if i == 2 {
                break
            }
        }

        for x in b {
            assert_eq!(expected[i], x);
            i += 1;
        }
    }

    #[test]
    fn test_from_iter() {
        let xs = ~[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)];

        let map: TreeMap<int, int> = xs.iter().map(|&x| x).collect();

        for &(k, v) in xs.iter() {
            assert_eq!(map.find(&k), Some(&v));
        }
    }

}

#[cfg(test)]
mod bench {
    extern crate test;
    use self::test::BenchHarness;
    use super::TreeMap;
    use deque::bench::{insert_rand_n, insert_seq_n, find_rand_n, find_seq_n};

    // Find seq
    #[bench]
    pub fn insert_rand_100(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        insert_rand_n(100, &mut m, bh);
    }

    #[bench]
    pub fn insert_rand_10_000(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        insert_rand_n(10_000, &mut m, bh);
    }

    // Insert seq
    #[bench]
    pub fn insert_seq_100(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        insert_seq_n(100, &mut m, bh);
    }

    #[bench]
    pub fn insert_seq_10_000(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        insert_seq_n(10_000, &mut m, bh);
    }

    // Find rand
    #[bench]
    pub fn find_rand_100(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        find_rand_n(100, &mut m, bh);
    }

    #[bench]
    pub fn find_rand_10_000(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        find_rand_n(10_000, &mut m, bh);
    }

    // Find seq
    #[bench]
    pub fn find_seq_100(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        find_seq_n(100, &mut m, bh);
    }

    #[bench]
    pub fn find_seq_10_000(bh: &mut BenchHarness) {
        let mut m : TreeMap<uint,uint> = TreeMap::new();
        find_seq_n(10_000, &mut m, bh);
    }
}

#[cfg(test)]
mod test_set {

    use super::{TreeMap, TreeSet};

    #[test]
    fn test_clear() {
        let mut s = TreeSet::new();
        s.clear();
        assert!(s.insert(5));
        assert!(s.insert(12));
        assert!(s.insert(19));
        s.clear();
        assert!(!s.contains(&5));
        assert!(!s.contains(&12));
        assert!(!s.contains(&19));
        assert!(s.is_empty());
    }

    #[test]
    fn test_disjoint() {
        let mut xs = TreeSet::new();
        let mut ys = TreeSet::new();
        assert!(xs.is_disjoint(&ys));
        assert!(ys.is_disjoint(&xs));
        assert!(xs.insert(5));
        assert!(ys.insert(11));
        assert!(xs.is_disjoint(&ys));
        assert!(ys.is_disjoint(&xs));
        assert!(xs.insert(7));
        assert!(xs.insert(19));
        assert!(xs.insert(4));
        assert!(ys.insert(2));
        assert!(ys.insert(-11));
        assert!(xs.is_disjoint(&ys));
        assert!(ys.is_disjoint(&xs));
        assert!(ys.insert(7));
        assert!(!xs.is_disjoint(&ys));
        assert!(!ys.is_disjoint(&xs));
    }

    #[test]
    fn test_subset_and_superset() {
        let mut a = TreeSet::new();
        assert!(a.insert(0));
        assert!(a.insert(5));
        assert!(a.insert(11));
        assert!(a.insert(7));

        let mut b = TreeSet::new();
        assert!(b.insert(0));
        assert!(b.insert(7));
        assert!(b.insert(19));
        assert!(b.insert(250));
        assert!(b.insert(11));
        assert!(b.insert(200));

        assert!(!a.is_subset(&b));
        assert!(!a.is_superset(&b));
        assert!(!b.is_subset(&a));
        assert!(!b.is_superset(&a));

        assert!(b.insert(5));

        assert!(a.is_subset(&b));
        assert!(!a.is_superset(&b));
        assert!(!b.is_subset(&a));
        assert!(b.is_superset(&a));
    }

    #[test]
    fn test_iterator() {
        let mut m = TreeSet::new();

        assert!(m.insert(3));
        assert!(m.insert(0));
        assert!(m.insert(4));
        assert!(m.insert(2));
        assert!(m.insert(1));

        let mut n = 0;
        for x in m.iter() {
            assert_eq!(*x, n);
            n += 1
        }
    }

    #[test]
    fn test_rev_iter() {
        let mut m = TreeSet::new();

        assert!(m.insert(3));
        assert!(m.insert(0));
        assert!(m.insert(4));
        assert!(m.insert(2));
        assert!(m.insert(1));

        let mut n = 4;
        for x in m.rev_iter() {
            assert_eq!(*x, n);
            n -= 1;
        }
    }

    #[test]
    fn test_clone_eq() {
      let mut m = TreeSet::new();

      m.insert(1);
      m.insert(2);

      assert!(m.clone() == m);
    }

    fn check(a: &[int],
             b: &[int],
             expected: &[int],
             f: |&TreeSet<int>, &TreeSet<int>, f: |&int| -> bool| -> bool) {
        let mut set_a = TreeSet::new();
        let mut set_b = TreeSet::new();

        for x in a.iter() { assert!(set_a.insert(*x)) }
        for y in b.iter() { assert!(set_b.insert(*y)) }

        let mut i = 0;
        f(&set_a, &set_b, |x| {
            assert_eq!(*x, expected[i]);
            i += 1;
            true
        });
        assert_eq!(i, expected.len());
    }

    #[test]
    fn test_intersection() {
        fn check_intersection(a: &[int], b: &[int], expected: &[int]) {
            check(a, b, expected, |x, y, f| x.intersection(y).advance(f))
        }

        check_intersection([], [], []);
        check_intersection([1, 2, 3], [], []);
        check_intersection([], [1, 2, 3], []);
        check_intersection([2], [1, 2, 3], [2]);
        check_intersection([1, 2, 3], [2], [2]);
        check_intersection([11, 1, 3, 77, 103, 5, -5],
                           [2, 11, 77, -9, -42, 5, 3],
                           [3, 5, 11, 77]);
    }

    #[test]
    fn test_difference() {
        fn check_difference(a: &[int], b: &[int], expected: &[int]) {
            check(a, b, expected, |x, y, f| x.difference(y).advance(f))
        }

        check_difference([], [], []);
        check_difference([1, 12], [], [1, 12]);
        check_difference([], [1, 2, 3, 9], []);
        check_difference([1, 3, 5, 9, 11],
                         [3, 9],
                         [1, 5, 11]);
        check_difference([-5, 11, 22, 33, 40, 42],
                         [-12, -5, 14, 23, 34, 38, 39, 50],
                         [11, 22, 33, 40, 42]);
    }

    #[test]
    fn test_symmetric_difference() {
        fn check_symmetric_difference(a: &[int], b: &[int],
                                      expected: &[int]) {
            check(a, b, expected, |x, y, f| x.symmetric_difference(y).advance(f))
        }

        check_symmetric_difference([], [], []);
        check_symmetric_difference([1, 2, 3], [2], [1, 3]);
        check_symmetric_difference([2], [1, 2, 3], [1, 3]);
        check_symmetric_difference([1, 3, 5, 9, 11],
                                   [-2, 3, 9, 14, 22],
                                   [-2, 1, 5, 11, 14, 22]);
    }

    #[test]
    fn test_union() {
        fn check_union(a: &[int], b: &[int],
                                      expected: &[int]) {
            check(a, b, expected, |x, y, f| x.union(y).advance(f))
        }

        check_union([], [], []);
        check_union([1, 2, 3], [2], [1, 2, 3]);
        check_union([2], [1, 2, 3], [1, 2, 3]);
        check_union([1, 3, 5, 9, 11, 16, 19, 24],
                    [-2, 1, 5, 9, 13, 19],
                    [-2, 1, 3, 5, 9, 11, 13, 16, 19, 24]);
    }

    #[test]
    fn test_zip() {
        let mut x = TreeSet::new();
        x.insert(5u);
        x.insert(12u);
        x.insert(11u);

        let mut y = TreeSet::new();
        y.insert("foo");
        y.insert("bar");

        let x = x;
        let y = y;
        let mut z = x.iter().zip(y.iter());

        // FIXME: #5801: this needs a type hint to compile...
        let result: Option<(&uint, & &'static str)> = z.next();
        assert_eq!(result.unwrap(), (&5u, & &"bar"));

        let result: Option<(&uint, & &'static str)> = z.next();
        assert_eq!(result.unwrap(), (&11u, & &"foo"));

        let result: Option<(&uint, & &'static str)> = z.next();
        assert!(result.is_none());
    }

    #[test]
    fn test_swap() {
        let mut m = TreeMap::new();
        assert_eq!(m.swap(1, 2), None);
        assert_eq!(m.swap(1, 3), Some(2));
        assert_eq!(m.swap(1, 4), Some(3));
    }

    #[test]
    fn test_pop() {
        let mut m = TreeMap::new();
        m.insert(1, 2);
        assert_eq!(m.pop(&1), Some(2));
        assert_eq!(m.pop(&1), None);
    }

    #[test]
    fn test_from_iter() {
        let xs = ~[1, 2, 3, 4, 5, 6, 7, 8, 9];

        let set: TreeSet<int> = xs.iter().map(|&x| x).collect();

        for x in xs.iter() {
            assert!(set.contains(x));
        }
    }
}
