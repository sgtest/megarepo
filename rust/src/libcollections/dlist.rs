// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A doubly-linked list with owned nodes.
//!
//! The `DList` allows pushing and popping elements at either end and is thus
//! efficiently usable as a double-ended queue.

// DList is constructed like a singly-linked list over the field `next`.
// including the last link being None; each Node owns its `next` field.
//
// Backlinks over DList::prev are raw pointers that form a full chain in
// the reverse direction.

use core::prelude::*;

use alloc::boxed::Box;
use core::cmp::Ordering;
use core::default::Default;
use core::fmt;
use core::hash::{Writer, Hash};
use core::iter::{mod, FromIterator};
use core::mem;
use core::ptr;

/// A doubly-linked list.
#[stable]
pub struct DList<T> {
    length: uint,
    list_head: Link<T>,
    list_tail: Rawlink<Node<T>>,
}

type Link<T> = Option<Box<Node<T>>>;

struct Rawlink<T> {
    p: *mut T,
}

impl<T> Copy for Rawlink<T> {}
unsafe impl<T:'static+Send> Send for Rawlink<T> {}
unsafe impl<T:Send+Sync> Sync for Rawlink<T> {}

struct Node<T> {
    next: Link<T>,
    prev: Rawlink<Node<T>>,
    value: T,
}

/// An iterator over references to the items of a `DList`.
#[stable]
pub struct Iter<'a, T:'a> {
    head: &'a Link<T>,
    tail: Rawlink<Node<T>>,
    nelem: uint,
}

// FIXME #19839: deriving is too aggressive on the bounds (T doesn't need to be Clone).
#[stable]
impl<'a, T> Clone for Iter<'a, T> {
    fn clone(&self) -> Iter<'a, T> {
        Iter {
            head: self.head.clone(),
            tail: self.tail,
            nelem: self.nelem,
        }
    }
}

/// An iterator over mutable references to the items of a `DList`.
#[stable]
pub struct IterMut<'a, T:'a> {
    list: &'a mut DList<T>,
    head: Rawlink<Node<T>>,
    tail: Rawlink<Node<T>>,
    nelem: uint,
}

/// An iterator over mutable references to the items of a `DList`.
#[deriving(Clone)]
#[stable]
pub struct IntoIter<T> {
    list: DList<T>
}

/// Rawlink is a type like Option<T> but for holding a raw pointer
impl<T> Rawlink<T> {
    /// Like Option::None for Rawlink
    fn none() -> Rawlink<T> {
        Rawlink{p: ptr::null_mut()}
    }

    /// Like Option::Some for Rawlink
    fn some(n: &mut T) -> Rawlink<T> {
        Rawlink{p: n}
    }

    /// Convert the `Rawlink` into an Option value
    fn resolve_immut<'a>(&self) -> Option<&'a T> {
        unsafe {
            mem::transmute(self.p.as_ref())
        }
    }

    /// Convert the `Rawlink` into an Option value
    fn resolve<'a>(&mut self) -> Option<&'a mut T> {
        if self.p.is_null() {
            None
        } else {
            Some(unsafe { mem::transmute(self.p) })
        }
    }

    /// Return the `Rawlink` and replace with `Rawlink::none()`
    fn take(&mut self) -> Rawlink<T> {
        mem::replace(self, Rawlink::none())
    }
}

impl<T> Clone for Rawlink<T> {
    #[inline]
    fn clone(&self) -> Rawlink<T> {
        Rawlink{p: self.p}
    }
}

impl<T> Node<T> {
    fn new(v: T) -> Node<T> {
        Node{value: v, next: None, prev: Rawlink::none()}
    }
}

/// Set the .prev field on `next`, then return `Some(next)`
fn link_with_prev<T>(mut next: Box<Node<T>>, prev: Rawlink<Node<T>>)
                  -> Link<T> {
    next.prev = prev;
    Some(next)
}

// private methods
impl<T> DList<T> {
    /// Add a Node first in the list
    #[inline]
    fn push_front_node(&mut self, mut new_head: Box<Node<T>>) {
        match self.list_head {
            None => {
                self.list_tail = Rawlink::some(&mut *new_head);
                self.list_head = link_with_prev(new_head, Rawlink::none());
            }
            Some(ref mut head) => {
                new_head.prev = Rawlink::none();
                head.prev = Rawlink::some(&mut *new_head);
                mem::swap(head, &mut new_head);
                head.next = Some(new_head);
            }
        }
        self.length += 1;
    }

    /// Remove the first Node and return it, or None if the list is empty
    #[inline]
    fn pop_front_node(&mut self) -> Option<Box<Node<T>>> {
        self.list_head.take().map(|mut front_node| {
            self.length -= 1;
            match front_node.next.take() {
                Some(node) => self.list_head = link_with_prev(node, Rawlink::none()),
                None => self.list_tail = Rawlink::none()
            }
            front_node
        })
    }

    /// Add a Node last in the list
    #[inline]
    fn push_back_node(&mut self, mut new_tail: Box<Node<T>>) {
        match self.list_tail.resolve() {
            None => return self.push_front_node(new_tail),
            Some(tail) => {
                self.list_tail = Rawlink::some(&mut *new_tail);
                tail.next = link_with_prev(new_tail, Rawlink::some(tail));
            }
        }
        self.length += 1;
    }

    /// Remove the last Node and return it, or None if the list is empty
    #[inline]
    fn pop_back_node(&mut self) -> Option<Box<Node<T>>> {
        self.list_tail.resolve().map_or(None, |tail| {
            self.length -= 1;
            self.list_tail = tail.prev;
            match tail.prev.resolve() {
                None => self.list_head.take(),
                Some(tail_prev) => tail_prev.next.take()
            }
        })
    }
}

#[stable]
impl<T> Default for DList<T> {
    #[inline]
    #[stable]
    fn default() -> DList<T> { DList::new() }
}

impl<T> DList<T> {
    /// Creates an empty `DList`.
    #[inline]
    #[stable]
    pub fn new() -> DList<T> {
        DList{list_head: None, list_tail: Rawlink::none(), length: 0}
    }

    /// Deprecated: Not clearly useful enough; use split and append when available.
    #[deprecated = "Not clearly useful enough; use split and append when available"]
    pub fn rotate_forward(&mut self) {
        self.pop_back_node().map(|tail| {
            self.push_front_node(tail)
        });
    }

    /// Deprecated: Not clearly useful enough; use split and append when available.
    #[deprecated = "Not clearly useful enough; use split and append when available"]
    pub fn rotate_backward(&mut self) {
        self.pop_front_node().map(|head| {
            self.push_back_node(head)
        });
    }

    /// Adds all elements from `other` to the end of the list.
    ///
    /// This operation should compute in O(1) time.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::DList;
    ///
    /// let mut a = DList::new();
    /// let mut b = DList::new();
    /// a.push_back(1i);
    /// a.push_back(2);
    /// b.push_back(3i);
    /// b.push_back(4);
    ///
    /// a.append(b);
    ///
    /// for e in a.iter() {
    ///     println!("{}", e); // prints 1, then 2, then 3, then 4
    /// }
    /// ```
    #[unstable = "append should be by-mutable-reference"]
    pub fn append(&mut self, mut other: DList<T>) {
        match self.list_tail.resolve() {
            None => *self = other,
            Some(tail) => {
                // Carefully empty `other`.
                let o_tail = other.list_tail.take();
                let o_length = other.length;
                match other.list_head.take() {
                    None => return,
                    Some(node) => {
                        tail.next = link_with_prev(node, self.list_tail);
                        self.list_tail = o_tail;
                        self.length += o_length;
                    }
                }
            }
        }
    }

    /// Deprecated: Use append and a swap instead.
    #[deprecated = "Use append and a swap instead"]
    pub fn prepend(&mut self, mut other: DList<T>) {
        mem::swap(self, &mut other);
        self.append(other);
    }

    /// Deprecated: Use custom methods on IterMut.
    #[deprecated = "Use custom methods on IterMut"]
    pub fn insert_when<F>(&mut self, elt: T, mut f: F) where F: FnMut(&T, &T) -> bool {
        let mut it = self.iter_mut();
        loop {
            match it.peek_next() {
                None => break,
                Some(x) => if f(x, &elt) { break }
            }
            it.next();
        }
        it.insert_next(elt);
    }

    /// Deprecated: Use custom methods on IterMut.
    #[deprecated = "Use custom methods on IterMut"]
    pub fn merge<F>(&mut self, mut other: DList<T>, mut f: F) where F: FnMut(&T, &T) -> bool {
        {
            let mut it = self.iter_mut();
            loop {
                let take_a = match (it.peek_next(), other.front()) {
                    (_   , None) => return,
                    (None, _   ) => break,
                    (Some(ref mut x), Some(y)) => f(*x, y),
                };
                if take_a {
                    it.next();
                } else {
                    it.insert_next_node(other.pop_front_node().unwrap());
                }
            }
        }
        self.append(other);
    }


    /// Provides a forward iterator.
    #[inline]
    #[stable]
    pub fn iter(&self) -> Iter<T> {
        Iter{nelem: self.len(), head: &self.list_head, tail: self.list_tail}
    }

    /// Provides a forward iterator with mutable references.
    #[inline]
    #[stable]
    pub fn iter_mut(&mut self) -> IterMut<T> {
        let head_raw = match self.list_head {
            Some(ref mut h) => Rawlink::some(&mut **h),
            None => Rawlink::none(),
        };
        IterMut{
            nelem: self.len(),
            head: head_raw,
            tail: self.list_tail,
            list: self
        }
    }

    /// Consumes the list into an iterator yielding elements by value.
    #[inline]
    #[stable]
    pub fn into_iter(self) -> IntoIter<T> {
        IntoIter{list: self}
    }

    /// Returns `true` if the `DList` is empty.
    ///
    /// This operation should compute in O(1) time.
    #[inline]
    #[stable]
    pub fn is_empty(&self) -> bool {
        self.list_head.is_none()
    }

    /// Returns the length of the `DList`.
    ///
    /// This operation should compute in O(1) time.
    #[inline]
    #[stable]
    pub fn len(&self) -> uint {
        self.length
    }

    /// Removes all elements from the `DList`.
    ///
    /// This operation should compute in O(n) time.
    #[inline]
    #[stable]
    pub fn clear(&mut self) {
        *self = DList::new()
    }

    /// Provides a reference to the front element, or `None` if the list is
    /// empty.
    #[inline]
    #[stable]
    pub fn front(&self) -> Option<&T> {
        self.list_head.as_ref().map(|head| &head.value)
    }

    /// Provides a mutable reference to the front element, or `None` if the list
    /// is empty.
    #[inline]
    #[stable]
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.list_head.as_mut().map(|head| &mut head.value)
    }

    /// Provides a reference to the back element, or `None` if the list is
    /// empty.
    #[inline]
    #[stable]
    pub fn back(&self) -> Option<&T> {
        self.list_tail.resolve_immut().as_ref().map(|tail| &tail.value)
    }

    /// Provides a mutable reference to the back element, or `None` if the list
    /// is empty.
    #[inline]
    #[stable]
    pub fn back_mut(&mut self) -> Option<&mut T> {
        self.list_tail.resolve().map(|tail| &mut tail.value)
    }

    /// Adds an element first in the list.
    ///
    /// This operation should compute in O(1) time.
    #[stable]
    pub fn push_front(&mut self, elt: T) {
        self.push_front_node(box Node::new(elt))
    }

    /// Removes the first element and returns it, or `None` if the list is
    /// empty.
    ///
    /// This operation should compute in O(1) time.
    #[stable]
    pub fn pop_front(&mut self) -> Option<T> {
        self.pop_front_node().map(|box Node{value, ..}| value)
    }

    /// Deprecated: Renamed to `push_back`.
    #[deprecated = "Renamed to `push_back`"]
    pub fn push(&mut self, elt: T) {
        self.push_back(elt)
    }

    /// Appends an element to the back of a list
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::DList;
    ///
    /// let mut d = DList::new();
    /// d.push_back(1i);
    /// d.push_back(3);
    /// assert_eq!(3, *d.back().unwrap());
    /// ```
    #[stable]
    pub fn push_back(&mut self, elt: T) {
        self.push_back_node(box Node::new(elt))
    }

    /// Deprecated: Renamed to `pop_back`.
    #[deprecated = "Renamed to `pop_back`"]
    pub fn pop(&mut self) -> Option<T> {
        self.pop_back()
    }

    /// Removes the last element from a list and returns it, or `None` if
    /// it is empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::DList;
    ///
    /// let mut d = DList::new();
    /// assert_eq!(d.pop_back(), None);
    /// d.push_back(1i);
    /// d.push_back(3);
    /// assert_eq!(d.pop_back(), Some(3));
    /// ```
    #[stable]
    pub fn pop_back(&mut self) -> Option<T> {
        self.pop_back_node().map(|box Node{value, ..}| value)
    }
}

impl<T: Ord> DList<T> {
    /// Deprecated: Why are you maintaining a sorted DList?
    #[deprecated = "Why are you maintaining a sorted DList?"]
    #[allow(deprecated)]
    pub fn insert_ordered(&mut self, elt: T) {
        self.insert_when(elt, |a, b| a >= b)
    }
}

#[unsafe_destructor]
#[stable]
impl<T> Drop for DList<T> {
    fn drop(&mut self) {
        // Dissolve the dlist in backwards direction
        // Just dropping the list_head can lead to stack exhaustion
        // when length is >> 1_000_000
        let mut tail = self.list_tail;
        loop {
            match tail.resolve() {
                None => break,
                Some(prev) => {
                    prev.next.take(); // release Box<Node<T>>
                    tail = prev.prev;
                }
            }
        }
        self.length = 0;
        self.list_head = None;
        self.list_tail = Rawlink::none();
    }
}

#[stable]
impl<'a, A> Iterator<&'a A> for Iter<'a, A> {
    #[inline]
    fn next(&mut self) -> Option<&'a A> {
        if self.nelem == 0 {
            return None;
        }
        self.head.as_ref().map(|head| {
            self.nelem -= 1;
            self.head = &head.next;
            &head.value
        })
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (self.nelem, Some(self.nelem))
    }
}

#[stable]
impl<'a, A> DoubleEndedIterator<&'a A> for Iter<'a, A> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a A> {
        if self.nelem == 0 {
            return None;
        }
        self.tail.resolve_immut().as_ref().map(|prev| {
            self.nelem -= 1;
            self.tail = prev.prev;
            &prev.value
        })
    }
}

#[stable]
impl<'a, A> ExactSizeIterator<&'a A> for Iter<'a, A> {}

#[stable]
impl<'a, A> Iterator<&'a mut A> for IterMut<'a, A> {
    #[inline]
    fn next(&mut self) -> Option<&'a mut A> {
        if self.nelem == 0 {
            return None;
        }
        self.head.resolve().map(|next| {
            self.nelem -= 1;
            self.head = match next.next {
                Some(ref mut node) => Rawlink::some(&mut **node),
                None => Rawlink::none(),
            };
            &mut next.value
        })
    }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (self.nelem, Some(self.nelem))
    }
}

#[stable]
impl<'a, A> DoubleEndedIterator<&'a mut A> for IterMut<'a, A> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a mut A> {
        if self.nelem == 0 {
            return None;
        }
        self.tail.resolve().map(|prev| {
            self.nelem -= 1;
            self.tail = prev.prev;
            &mut prev.value
        })
    }
}

#[stable]
impl<'a, A> ExactSizeIterator<&'a mut A> for IterMut<'a, A> {}

/// Allows mutating a `DList` while iterating.
#[deprecated = "Trait is deprecated, use inherent methods on the iterator instead"]
pub trait ListInsertion<A> {
    /// Inserts `elt` just after to the element most recently returned by
    /// `.next()`
    ///
    /// The inserted element does not appear in the iteration.
    fn insert_next(&mut self, elt: A);

    /// Provides a reference to the next element, without changing the iterator
    fn peek_next<'a>(&'a mut self) -> Option<&'a mut A>;
}

// private methods for IterMut
impl<'a, A> IterMut<'a, A> {
    fn insert_next_node(&mut self, mut ins_node: Box<Node<A>>) {
        // Insert before `self.head` so that it is between the
        // previously yielded element and self.head.
        //
        // The inserted node will not appear in further iteration.
        match self.head.resolve() {
            None => { self.list.push_back_node(ins_node); }
            Some(node) => {
                let prev_node = match node.prev.resolve() {
                    None => return self.list.push_front_node(ins_node),
                    Some(prev) => prev,
                };
                let node_own = prev_node.next.take().unwrap();
                ins_node.next = link_with_prev(node_own, Rawlink::some(&mut *ins_node));
                prev_node.next = link_with_prev(ins_node, Rawlink::some(prev_node));
                self.list.length += 1;
            }
        }
    }
}

impl<'a, A> IterMut<'a, A> {
    /// Inserts `elt` just after the element most recently returned by `.next()`.
    /// The inserted element does not appear in the iteration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::DList;
    ///
    /// let mut list: DList<int> = vec![1, 3, 4].into_iter().collect();
    ///
    /// {
    ///     let mut it = list.iter_mut();
    ///     assert_eq!(it.next().unwrap(), &1);
    ///     // insert `2` after `1`
    ///     it.insert_next(2);
    /// }
    /// {
    ///     let vec: Vec<int> = list.into_iter().collect();
    ///     assert_eq!(vec, vec![1i, 2, 3, 4]);
    /// }
    /// ```
    #[inline]
    #[unstable = "this is probably better handled by a cursor type -- we'll see"]
    pub fn insert_next(&mut self, elt: A) {
        self.insert_next_node(box Node::new(elt))
    }

    /// Provides a reference to the next element, without changing the iterator.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::DList;
    ///
    /// let mut list: DList<int> = vec![1, 2, 3].into_iter().collect();
    ///
    /// let mut it = list.iter_mut();
    /// assert_eq!(it.next().unwrap(), &1);
    /// assert_eq!(it.peek_next().unwrap(), &2);
    /// // We just peeked at 2, so it was not consumed from the iterator.
    /// assert_eq!(it.next().unwrap(), &2);
    /// ```
    #[inline]
    #[unstable = "this is probably better handled by a cursor type -- we'll see"]
    pub fn peek_next(&mut self) -> Option<&mut A> {
        if self.nelem == 0 {
            return None
        }
        self.head.resolve().map(|head| &mut head.value)
    }
}

#[stable]
impl<A> Iterator<A> for IntoIter<A> {
    #[inline]
    fn next(&mut self) -> Option<A> { self.list.pop_front() }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (self.list.length, Some(self.list.length))
    }
}

#[stable]
impl<A> DoubleEndedIterator<A> for IntoIter<A> {
    #[inline]
    fn next_back(&mut self) -> Option<A> { self.list.pop_back() }
}

#[stable]
impl<A> FromIterator<A> for DList<A> {
    fn from_iter<T: Iterator<A>>(iterator: T) -> DList<A> {
        let mut ret = DList::new();
        ret.extend(iterator);
        ret
    }
}

#[stable]
impl<A> Extend<A> for DList<A> {
    fn extend<T: Iterator<A>>(&mut self, mut iterator: T) {
        for elt in iterator { self.push_back(elt); }
    }
}

#[stable]
impl<A: PartialEq> PartialEq for DList<A> {
    fn eq(&self, other: &DList<A>) -> bool {
        self.len() == other.len() &&
            iter::order::eq(self.iter(), other.iter())
    }

    fn ne(&self, other: &DList<A>) -> bool {
        self.len() != other.len() ||
            iter::order::ne(self.iter(), other.iter())
    }
}

#[stable]
impl<A: Eq> Eq for DList<A> {}

#[stable]
impl<A: PartialOrd> PartialOrd for DList<A> {
    fn partial_cmp(&self, other: &DList<A>) -> Option<Ordering> {
        iter::order::partial_cmp(self.iter(), other.iter())
    }
}

#[stable]
impl<A: Ord> Ord for DList<A> {
    #[inline]
    fn cmp(&self, other: &DList<A>) -> Ordering {
        iter::order::cmp(self.iter(), other.iter())
    }
}

#[stable]
impl<A: Clone> Clone for DList<A> {
    fn clone(&self) -> DList<A> {
        self.iter().map(|x| x.clone()).collect()
    }
}

#[stable]
impl<A: fmt::Show> fmt::Show for DList<A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "["));

        for (i, e) in self.iter().enumerate() {
            if i != 0 { try!(write!(f, ", ")); }
            try!(write!(f, "{}", *e));
        }

        write!(f, "]")
    }
}

#[stable]
impl<S: Writer, A: Hash<S>> Hash<S> for DList<A> {
    fn hash(&self, state: &mut S) {
        self.len().hash(state);
        for elt in self.iter() {
            elt.hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use prelude::*;
    use std::rand;
    use std::hash;
    use std::task::spawn;
    use test::Bencher;
    use test;

    use super::{DList, Node};

    pub fn check_links<T>(list: &DList<T>) {
        let mut len = 0u;
        let mut last_ptr: Option<&Node<T>> = None;
        let mut node_ptr: &Node<T>;
        match list.list_head {
            None => { assert_eq!(0u, list.length); return }
            Some(ref node) => node_ptr = &**node,
        }
        loop {
            match (last_ptr, node_ptr.prev.resolve_immut()) {
                (None   , None      ) => {}
                (None   , _         ) => panic!("prev link for list_head"),
                (Some(p), Some(pptr)) => {
                    assert_eq!(p as *const Node<T>, pptr as *const Node<T>);
                }
                _ => panic!("prev link is none, not good"),
            }
            match node_ptr.next {
                Some(ref next) => {
                    last_ptr = Some(node_ptr);
                    node_ptr = &**next;
                    len += 1;
                }
                None => {
                    len += 1;
                    break;
                }
            }
        }
        assert_eq!(len, list.length);
    }

    #[test]
    fn test_basic() {
        let mut m: DList<Box<int>> = DList::new();
        assert_eq!(m.pop_front(), None);
        assert_eq!(m.pop_back(), None);
        assert_eq!(m.pop_front(), None);
        m.push_front(box 1);
        assert_eq!(m.pop_front(), Some(box 1));
        m.push_back(box 2);
        m.push_back(box 3);
        assert_eq!(m.len(), 2);
        assert_eq!(m.pop_front(), Some(box 2));
        assert_eq!(m.pop_front(), Some(box 3));
        assert_eq!(m.len(), 0);
        assert_eq!(m.pop_front(), None);
        m.push_back(box 1);
        m.push_back(box 3);
        m.push_back(box 5);
        m.push_back(box 7);
        assert_eq!(m.pop_front(), Some(box 1));

        let mut n = DList::new();
        n.push_front(2i);
        n.push_front(3);
        {
            assert_eq!(n.front().unwrap(), &3);
            let x = n.front_mut().unwrap();
            assert_eq!(*x, 3);
            *x = 0;
        }
        {
            assert_eq!(n.back().unwrap(), &2);
            let y = n.back_mut().unwrap();
            assert_eq!(*y, 2);
            *y = 1;
        }
        assert_eq!(n.pop_front(), Some(0));
        assert_eq!(n.pop_front(), Some(1));
    }

    #[cfg(test)]
    fn generate_test() -> DList<int> {
        list_from(&[0i,1,2,3,4,5,6])
    }

    #[cfg(test)]
    fn list_from<T: Clone>(v: &[T]) -> DList<T> {
        v.iter().map(|x| (*x).clone()).collect()
    }

    #[test]
    #[allow(deprecated)]
    fn test_append() {
        {
            let mut m = DList::new();
            let mut n = DList::new();
            n.push_back(2i);
            m.append(n);
            assert_eq!(m.len(), 1);
            assert_eq!(m.pop_back(), Some(2));
            check_links(&m);
        }
        {
            let mut m = DList::new();
            let n = DList::new();
            m.push_back(2i);
            m.append(n);
            assert_eq!(m.len(), 1);
            assert_eq!(m.pop_back(), Some(2));
            check_links(&m);
        }

        let v = vec![1i,2,3,4,5];
        let u = vec![9i,8,1,2,3,4,5];
        let mut m = list_from(v.as_slice());
        m.append(list_from(u.as_slice()));
        check_links(&m);
        let mut sum = v;
        sum.push_all(u.as_slice());
        assert_eq!(sum.len(), m.len());
        for elt in sum.into_iter() {
            assert_eq!(m.pop_front(), Some(elt))
        }
    }

    #[test]
    fn test_prepend() {
        {
            let mut m = DList::new();
            let mut n = DList::new();
            n.push_back(2i);
            m.prepend(n);
            assert_eq!(m.len(), 1);
            assert_eq!(m.pop_back(), Some(2));
            check_links(&m);
        }

        let v = vec![1i,2,3,4,5];
        let mut u = vec![9i,8,1,2,3,4,5];
        let mut m = list_from(v.as_slice());
        m.prepend(list_from(u.as_slice()));
        check_links(&m);
        u.extend(v.iter().map(|&b| b));
        assert_eq!(u.len(), m.len());
        for elt in u.into_iter() {
            assert_eq!(m.pop_front(), Some(elt))
        }
    }

    #[test]
    fn test_rotate() {
        let mut n: DList<int> = DList::new();
        n.rotate_backward(); check_links(&n);
        assert_eq!(n.len(), 0);
        n.rotate_forward(); check_links(&n);
        assert_eq!(n.len(), 0);

        let v = vec![1i,2,3,4,5];
        let mut m = list_from(v.as_slice());
        m.rotate_backward(); check_links(&m);
        m.rotate_forward(); check_links(&m);
        assert_eq!(v.iter().collect::<Vec<&int>>(), m.iter().collect::<Vec<_>>());
        m.rotate_forward(); check_links(&m);
        m.rotate_forward(); check_links(&m);
        m.pop_front(); check_links(&m);
        m.rotate_forward(); check_links(&m);
        m.rotate_backward(); check_links(&m);
        m.push_front(9); check_links(&m);
        m.rotate_forward(); check_links(&m);
        assert_eq!(vec![3i,9,5,1,2], m.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn test_iterator() {
        let m = generate_test();
        for (i, elt) in m.iter().enumerate() {
            assert_eq!(i as int, *elt);
        }
        let mut n = DList::new();
        assert_eq!(n.iter().next(), None);
        n.push_front(4i);
        let mut it = n.iter();
        assert_eq!(it.size_hint(), (1, Some(1)));
        assert_eq!(it.next().unwrap(), &4);
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_iterator_clone() {
        let mut n = DList::new();
        n.push_back(2i);
        n.push_back(3);
        n.push_back(4);
        let mut it = n.iter();
        it.next();
        let mut jt = it.clone();
        assert_eq!(it.next(), jt.next());
        assert_eq!(it.next_back(), jt.next_back());
        assert_eq!(it.next(), jt.next());
    }

    #[test]
    fn test_iterator_double_end() {
        let mut n = DList::new();
        assert_eq!(n.iter().next(), None);
        n.push_front(4i);
        n.push_front(5);
        n.push_front(6);
        let mut it = n.iter();
        assert_eq!(it.size_hint(), (3, Some(3)));
        assert_eq!(it.next().unwrap(), &6);
        assert_eq!(it.size_hint(), (2, Some(2)));
        assert_eq!(it.next_back().unwrap(), &4);
        assert_eq!(it.size_hint(), (1, Some(1)));
        assert_eq!(it.next_back().unwrap(), &5);
        assert_eq!(it.next_back(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_rev_iter() {
        let m = generate_test();
        for (i, elt) in m.iter().rev().enumerate() {
            assert_eq!((6 - i) as int, *elt);
        }
        let mut n = DList::new();
        assert_eq!(n.iter().rev().next(), None);
        n.push_front(4i);
        let mut it = n.iter().rev();
        assert_eq!(it.size_hint(), (1, Some(1)));
        assert_eq!(it.next().unwrap(), &4);
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_mut_iter() {
        let mut m = generate_test();
        let mut len = m.len();
        for (i, elt) in m.iter_mut().enumerate() {
            assert_eq!(i as int, *elt);
            len -= 1;
        }
        assert_eq!(len, 0);
        let mut n = DList::new();
        assert!(n.iter_mut().next().is_none());
        n.push_front(4i);
        n.push_back(5);
        let mut it = n.iter_mut();
        assert_eq!(it.size_hint(), (2, Some(2)));
        assert!(it.next().is_some());
        assert!(it.next().is_some());
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert!(it.next().is_none());
    }

    #[test]
    fn test_iterator_mut_double_end() {
        let mut n = DList::new();
        assert!(n.iter_mut().next_back().is_none());
        n.push_front(4i);
        n.push_front(5);
        n.push_front(6);
        let mut it = n.iter_mut();
        assert_eq!(it.size_hint(), (3, Some(3)));
        assert_eq!(*it.next().unwrap(), 6);
        assert_eq!(it.size_hint(), (2, Some(2)));
        assert_eq!(*it.next_back().unwrap(), 4);
        assert_eq!(it.size_hint(), (1, Some(1)));
        assert_eq!(*it.next_back().unwrap(), 5);
        assert!(it.next_back().is_none());
        assert!(it.next().is_none());
    }

    #[test]
    fn test_insert_prev() {
        let mut m = list_from(&[0i,2,4,6,8]);
        let len = m.len();
        {
            let mut it = m.iter_mut();
            it.insert_next(-2);
            loop {
                match it.next() {
                    None => break,
                    Some(elt) => {
                        it.insert_next(*elt + 1);
                        match it.peek_next() {
                            Some(x) => assert_eq!(*x, *elt + 2),
                            None => assert_eq!(8, *elt),
                        }
                    }
                }
            }
            it.insert_next(0);
            it.insert_next(1);
        }
        check_links(&m);
        assert_eq!(m.len(), 3 + len * 2);
        assert_eq!(m.into_iter().collect::<Vec<int>>(), vec![-2,0,1,2,3,4,5,6,7,8,9,0,1]);
    }

    #[test]
    fn test_merge() {
        let mut m = list_from(&[0i, 1, 3, 5, 6, 7, 2]);
        let n = list_from(&[-1i, 0, 0, 7, 7, 9]);
        let len = m.len() + n.len();
        m.merge(n, |a, b| a <= b);
        assert_eq!(m.len(), len);
        check_links(&m);
        let res = m.into_iter().collect::<Vec<int>>();
        assert_eq!(res, vec![-1, 0, 0, 0, 1, 3, 5, 6, 7, 2, 7, 7, 9]);
    }

    #[test]
    fn test_insert_ordered() {
        let mut n = DList::new();
        n.insert_ordered(1i);
        assert_eq!(n.len(), 1);
        assert_eq!(n.pop_front(), Some(1));

        let mut m = DList::new();
        m.push_back(2i);
        m.push_back(4);
        m.insert_ordered(3);
        check_links(&m);
        assert_eq!(vec![2,3,4], m.into_iter().collect::<Vec<int>>());
    }

    #[test]
    fn test_mut_rev_iter() {
        let mut m = generate_test();
        for (i, elt) in m.iter_mut().rev().enumerate() {
            assert_eq!((6-i) as int, *elt);
        }
        let mut n = DList::new();
        assert!(n.iter_mut().rev().next().is_none());
        n.push_front(4i);
        let mut it = n.iter_mut().rev();
        assert!(it.next().is_some());
        assert!(it.next().is_none());
    }

    #[test]
    fn test_send() {
        let n = list_from(&[1i,2,3]);
        spawn(move || {
            check_links(&n);
            let a: &[_] = &[&1,&2,&3];
            assert_eq!(a, n.iter().collect::<Vec<&int>>());
        });
    }

    #[test]
    fn test_eq() {
        let mut n: DList<u8> = list_from(&[]);
        let mut m = list_from(&[]);
        assert!(n == m);
        n.push_front(1);
        assert!(n != m);
        m.push_back(1);
        assert!(n == m);

        let n = list_from(&[2i,3,4]);
        let m = list_from(&[1i,2,3]);
        assert!(n != m);
    }

    #[test]
    fn test_hash() {
      let mut x = DList::new();
      let mut y = DList::new();

      assert!(hash::hash(&x) == hash::hash(&y));

      x.push_back(1i);
      x.push_back(2);
      x.push_back(3);

      y.push_front(3i);
      y.push_front(2);
      y.push_front(1);

      assert!(hash::hash(&x) == hash::hash(&y));
    }

    #[test]
    fn test_ord() {
        let n: DList<int> = list_from(&[]);
        let m = list_from(&[1i,2,3]);
        assert!(n < m);
        assert!(m > n);
        assert!(n <= n);
        assert!(n >= n);
    }

    #[test]
    fn test_ord_nan() {
        let nan = 0.0f64/0.0;
        let n = list_from(&[nan]);
        let m = list_from(&[nan]);
        assert!(!(n < m));
        assert!(!(n > m));
        assert!(!(n <= m));
        assert!(!(n >= m));

        let n = list_from(&[nan]);
        let one = list_from(&[1.0f64]);
        assert!(!(n < one));
        assert!(!(n > one));
        assert!(!(n <= one));
        assert!(!(n >= one));

        let u = list_from(&[1.0f64,2.0,nan]);
        let v = list_from(&[1.0f64,2.0,3.0]);
        assert!(!(u < v));
        assert!(!(u > v));
        assert!(!(u <= v));
        assert!(!(u >= v));

        let s = list_from(&[1.0f64,2.0,4.0,2.0]);
        let t = list_from(&[1.0f64,2.0,3.0,2.0]);
        assert!(!(s < t));
        assert!(s > one);
        assert!(!(s <= one));
        assert!(s >= one);
    }

    #[test]
    fn test_fuzz() {
        for _ in range(0u, 25) {
            fuzz_test(3);
            fuzz_test(16);
            fuzz_test(189);
        }
    }

    #[test]
    fn test_show() {
        let list: DList<int> = range(0i, 10).collect();
        assert!(list.to_string() == "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]");

        let list: DList<&str> = vec!["just", "one", "test", "more"].iter()
                                                                   .map(|&s| s)
                                                                   .collect();
        assert!(list.to_string() == "[just, one, test, more]");
    }

    #[cfg(test)]
    fn fuzz_test(sz: int) {
        let mut m: DList<int> = DList::new();
        let mut v = vec![];
        for i in range(0, sz) {
            check_links(&m);
            let r: u8 = rand::random();
            match r % 6 {
                0 => {
                    m.pop_back();
                    v.pop();
                }
                1 => {
                    if !v.is_empty() {
                        m.pop_front();
                        v.remove(0);
                    }
                }
                2 | 4 =>  {
                    m.push_front(-i);
                    v.insert(0, -i);
                }
                3 | 5 | _ => {
                    m.push_back(i);
                    v.push(i);
                }
            }
        }

        check_links(&m);

        let mut i = 0u;
        for (a, &b) in m.into_iter().zip(v.iter()) {
            i += 1;
            assert_eq!(a, b);
        }
        assert_eq!(i, v.len());
    }

    #[bench]
    fn bench_collect_into(b: &mut test::Bencher) {
        let v = &[0i; 64];
        b.iter(|| {
            let _: DList<int> = v.iter().map(|x| *x).collect();
        })
    }

    #[bench]
    fn bench_push_front(b: &mut test::Bencher) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_front(0);
        })
    }

    #[bench]
    fn bench_push_back(b: &mut test::Bencher) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_back(0);
        })
    }

    #[bench]
    fn bench_push_back_pop_back(b: &mut test::Bencher) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_back(0);
            m.pop_back();
        })
    }

    #[bench]
    fn bench_push_front_pop_front(b: &mut test::Bencher) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_front(0);
            m.pop_front();
        })
    }

    #[bench]
    fn bench_rotate_forward(b: &mut test::Bencher) {
        let mut m: DList<int> = DList::new();
        m.push_front(0i);
        m.push_front(1);
        b.iter(|| {
            m.rotate_forward();
        })
    }

    #[bench]
    fn bench_rotate_backward(b: &mut test::Bencher) {
        let mut m: DList<int> = DList::new();
        m.push_front(0i);
        m.push_front(1);
        b.iter(|| {
            m.rotate_backward();
        })
    }

    #[bench]
    fn bench_iter(b: &mut test::Bencher) {
        let v = &[0i; 128];
        let m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.iter().count() == 128);
        })
    }
    #[bench]
    fn bench_iter_mut(b: &mut test::Bencher) {
        let v = &[0i; 128];
        let mut m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.iter_mut().count() == 128);
        })
    }
    #[bench]
    fn bench_iter_rev(b: &mut test::Bencher) {
        let v = &[0i; 128];
        let m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.iter().rev().count() == 128);
        })
    }
    #[bench]
    fn bench_iter_mut_rev(b: &mut test::Bencher) {
        let v = &[0i; 128];
        let mut m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.iter_mut().rev().count() == 128);
        })
    }
}
