// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use core::default::Default;
use core::fmt;
use core::fmt::Show;
use std::hash::Hash;

use trie_map::{TrieMap, Entries};

/// A set implemented as a radix trie.
///
/// # Example
///
/// ```
/// use std::collections::TrieSet;
///
/// let mut set = TrieSet::new();
/// set.insert(6);
/// set.insert(28);
/// set.insert(6);
///
/// assert_eq!(set.len(), 2);
///
/// if !set.contains(&3) {
///     println!("3 is not in the set");
/// }
///
/// // Print contents in order
/// for x in set.iter() {
///     println!("{}", x);
/// }
///
/// set.remove(&6);
/// assert_eq!(set.len(), 1);
///
/// set.clear();
/// assert!(set.is_empty());
/// ```
#[deriving(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrieSet {
    map: TrieMap<()>
}

impl Show for TrieSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "{{"));

        for (i, x) in self.iter().enumerate() {
            if i != 0 { try!(write!(f, ", ")); }
            try!(write!(f, "{}", x));
        }

        write!(f, "}}")
    }
}

impl Default for TrieSet {
    #[inline]
    fn default() -> TrieSet { TrieSet::new() }
}

impl TrieSet {
    /// Creates an empty TrieSet.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    /// let mut set = TrieSet::new();
    /// ```
    #[inline]
    pub fn new() -> TrieSet {
        TrieSet{map: TrieMap::new()}
    }

    /// Visits all values in reverse order. Aborts traversal when `f` returns `false`.
    /// Returns `true` if `f` returns `true` for all elements.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let set: TrieSet = [1, 2, 3, 4, 5].iter().map(|&x| x).collect();
    ///
    /// let mut vec = Vec::new();
    /// assert_eq!(true, set.each_reverse(|&x| { vec.push(x); true }));
    /// assert_eq!(vec, vec![5, 4, 3, 2, 1]);
    ///
    /// // Stop when we reach 3
    /// let mut vec = Vec::new();
    /// assert_eq!(false, set.each_reverse(|&x| { vec.push(x); x != 3 }));
    /// assert_eq!(vec, vec![5, 4, 3]);
    /// ```
    #[inline]
    pub fn each_reverse(&self, f: |&uint| -> bool) -> bool {
        self.map.each_reverse(|k, _| f(k))
    }

    /// Gets an iterator over the values in the set, in sorted order.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let mut set = TrieSet::new();
    /// set.insert(3);
    /// set.insert(2);
    /// set.insert(1);
    /// set.insert(2);
    ///
    /// // Print 1, 2, 3
    /// for x in set.iter() {
    ///     println!("{}", x);
    /// }
    /// ```
    #[inline]
    pub fn iter<'a>(&'a self) -> SetItems<'a> {
        SetItems{iter: self.map.iter()}
    }

    /// Gets an iterator pointing to the first value that is not less than `val`.
    /// If all values in the set are less than `val` an empty iterator is returned.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let set: TrieSet = [2, 4, 6, 8].iter().map(|&x| x).collect();
    /// assert_eq!(set.lower_bound(4).next(), Some(4));
    /// assert_eq!(set.lower_bound(5).next(), Some(6));
    /// assert_eq!(set.lower_bound(10).next(), None);
    /// ```
    pub fn lower_bound<'a>(&'a self, val: uint) -> SetItems<'a> {
        SetItems{iter: self.map.lower_bound(val)}
    }

    /// Gets an iterator pointing to the first value that key is greater than `val`.
    /// If all values in the set are less than or equal to `val` an empty iterator is returned.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let set: TrieSet = [2, 4, 6, 8].iter().map(|&x| x).collect();
    /// assert_eq!(set.upper_bound(4).next(), Some(6));
    /// assert_eq!(set.upper_bound(5).next(), Some(6));
    /// assert_eq!(set.upper_bound(10).next(), None);
    /// ```
    pub fn upper_bound<'a>(&'a self, val: uint) -> SetItems<'a> {
        SetItems{iter: self.map.upper_bound(val)}
    }

    /// Return the number of elements in the set
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let mut v = TrieSet::new();
    /// assert_eq!(v.len(), 0);
    /// v.insert(1);
    /// assert_eq!(v.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> uint { self.map.len() }

    /// Returns true if the set contains no elements
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let mut v = TrieSet::new();
    /// assert!(v.is_empty());
    /// v.insert(1);
    /// assert!(!v.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Clears the set, removing all values.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let mut v = TrieSet::new();
    /// v.insert(1);
    /// v.clear();
    /// assert!(v.is_empty());
    /// ```
    #[inline]
    pub fn clear(&mut self) { self.map.clear() }

    /// Returns `true` if the set contains a value.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let set: TrieSet = [1, 2, 3].iter().map(|&x| x).collect();
    /// assert_eq!(set.contains(&1), true);
    /// assert_eq!(set.contains(&4), false);
    /// ```
    #[inline]
    pub fn contains(&self, value: &uint) -> bool {
        self.map.contains_key(value)
    }

    /// Returns `true` if the set has no elements in common with `other`.
    /// This is equivalent to checking for an empty intersection.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let a: TrieSet = [1, 2, 3].iter().map(|&x| x).collect();
    /// let mut b: TrieSet = TrieSet::new();
    ///
    /// assert_eq!(a.is_disjoint(&b), true);
    /// b.insert(4);
    /// assert_eq!(a.is_disjoint(&b), true);
    /// b.insert(1);
    /// assert_eq!(a.is_disjoint(&b), false);
    /// ```
    #[inline]
    pub fn is_disjoint(&self, other: &TrieSet) -> bool {
        self.iter().all(|v| !other.contains(&v))
    }

    /// Returns `true` if the set is a subset of another.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let sup: TrieSet = [1, 2, 3].iter().map(|&x| x).collect();
    /// let mut set: TrieSet = TrieSet::new();
    ///
    /// assert_eq!(set.is_subset(&sup), true);
    /// set.insert(2);
    /// assert_eq!(set.is_subset(&sup), true);
    /// set.insert(4);
    /// assert_eq!(set.is_subset(&sup), false);
    /// ```
    #[inline]
    pub fn is_subset(&self, other: &TrieSet) -> bool {
        self.iter().all(|v| other.contains(&v))
    }

    /// Returns `true` if the set is a superset of another.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let sub: TrieSet = [1, 2].iter().map(|&x| x).collect();
    /// let mut set: TrieSet = TrieSet::new();
    ///
    /// assert_eq!(set.is_superset(&sub), false);
    ///
    /// set.insert(0);
    /// set.insert(1);
    /// assert_eq!(set.is_superset(&sub), false);
    ///
    /// set.insert(2);
    /// assert_eq!(set.is_superset(&sub), true);
    /// ```
    #[inline]
    pub fn is_superset(&self, other: &TrieSet) -> bool {
        other.is_subset(self)
    }

    /// Adds a value to the set. Returns `true` if the value was not already
    /// present in the set.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let mut set = TrieSet::new();
    ///
    /// assert_eq!(set.insert(2), true);
    /// assert_eq!(set.insert(2), false);
    /// assert_eq!(set.len(), 1);
    /// ```
    #[inline]
    pub fn insert(&mut self, value: uint) -> bool {
        self.map.insert(value, ())
    }

    /// Removes a value from the set. Returns `true` if the value was
    /// present in the set.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::TrieSet;
    ///
    /// let mut set = TrieSet::new();
    ///
    /// set.insert(2);
    /// assert_eq!(set.remove(&2), true);
    /// assert_eq!(set.remove(&2), false);
    /// ```
    #[inline]
    pub fn remove(&mut self, value: &uint) -> bool {
        self.map.remove(value)
    }
}

impl FromIterator<uint> for TrieSet {
    fn from_iter<Iter: Iterator<uint>>(iter: Iter) -> TrieSet {
        let mut set = TrieSet::new();
        set.extend(iter);
        set
    }
}

impl Extendable<uint> for TrieSet {
    fn extend<Iter: Iterator<uint>>(&mut self, mut iter: Iter) {
        for elem in iter {
            self.insert(elem);
        }
    }
}

/// A forward iterator over a set.
pub struct SetItems<'a> {
    iter: Entries<'a, ()>
}

impl<'a> Iterator<uint> for SetItems<'a> {
    fn next(&mut self) -> Option<uint> {
        self.iter.next().map(|(key, _)| key)
    }

    fn size_hint(&self) -> (uint, Option<uint>) {
        self.iter.size_hint()
    }
}

#[cfg(test)]
mod test {
    use std::prelude::*;
    use std::uint;

    use super::TrieSet;

    #[test]
    fn test_sane_chunk() {
        let x = 1;
        let y = 1 << (uint::BITS - 1);

        let mut trie = TrieSet::new();

        assert!(trie.insert(x));
        assert!(trie.insert(y));

        assert_eq!(trie.len(), 2);

        let expected = [x, y];

        for (i, x) in trie.iter().enumerate() {
            assert_eq!(expected[i], x);
        }
    }

    #[test]
    fn test_from_iter() {
        let xs = vec![9u, 8, 7, 6, 5, 4, 3, 2, 1];

        let set: TrieSet = xs.iter().map(|&x| x).collect();

        for x in xs.iter() {
            assert!(set.contains(x));
        }
    }

    #[test]
    fn test_show() {
        let mut set = TrieSet::new();
        let empty = TrieSet::new();

        set.insert(1);
        set.insert(2);

        let set_str = format!("{}", set);

        assert!(set_str == "{1, 2}".to_string());
        assert_eq!(format!("{}", empty), "{}".to_string());
    }

    #[test]
    fn test_clone() {
        let mut a = TrieSet::new();

        a.insert(1);
        a.insert(2);
        a.insert(3);

        assert!(a.clone() == a);
    }

    #[test]
    fn test_lt() {
        let mut a = TrieSet::new();
        let mut b = TrieSet::new();

        assert!(!(a < b) && !(b < a));
        assert!(b.insert(2u));
        assert!(a < b);
        assert!(a.insert(3u));
        assert!(!(a < b) && b < a);
        assert!(b.insert(1));
        assert!(b < a);
        assert!(a.insert(0));
        assert!(a < b);
        assert!(a.insert(6));
        assert!(a < b && !(b < a));
    }

    #[test]
    fn test_ord() {
        let mut a = TrieSet::new();
        let mut b = TrieSet::new();

        assert!(a <= b && a >= b);
        assert!(a.insert(1u));
        assert!(a > b && a >= b);
        assert!(b < a && b <= a);
        assert!(b.insert(2u));
        assert!(b > a && b >= a);
        assert!(a < b && a <= b);
    }
}
