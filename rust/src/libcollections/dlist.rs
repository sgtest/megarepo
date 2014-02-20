// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
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
//! The DList allows pushing and popping elements at either end.
//!
//! DList implements the trait Deque. It should be imported with `use
//! extra::container::Deque`.


// DList is constructed like a singly-linked list over the field `next`.
// including the last link being None; each Node owns its `next` field.
//
// Backlinks over DList::prev are raw pointers that form a full chain in
// the reverse direction.

use std::cast;
use std::mem::{replace, swap};
use std::ptr;
use std::iter::Rev;
use std::iter;

use deque::Deque;

use serialize::{Encodable, Decodable, Encoder, Decoder};

/// A doubly-linked list.
pub struct DList<T> {
    priv length: uint,
    priv list_head: Link<T>,
    priv list_tail: Rawlink<Node<T>>,
}

type Link<T> = Option<~Node<T>>;
struct Rawlink<T> { p: *mut T }

struct Node<T> {
    next: Link<T>,
    prev: Rawlink<Node<T>>,
    value: T,
}

/// Double-ended DList iterator
pub struct Items<'a, T> {
    priv head: &'a Link<T>,
    priv tail: Rawlink<Node<T>>,
    priv nelem: uint,
}

// FIXME #11820: the &'a Option<> of the Link stops clone working.
impl<'a, T> Clone for Items<'a, T> {
    fn clone(&self) -> Items<'a, T> { *self }
}

/// Double-ended mutable DList iterator
pub struct MutItems<'a, T> {
    priv list: &'a mut DList<T>,
    priv head: Rawlink<Node<T>>,
    priv tail: Rawlink<Node<T>>,
    priv nelem: uint,
}

/// DList consuming iterator
#[deriving(Clone)]
pub struct MoveItems<T> {
    priv list: DList<T>
}

/// Rawlink is a type like Option<T> but for holding a raw pointer
impl<T> Rawlink<T> {
    /// Like Option::None for Rawlink
    fn none() -> Rawlink<T> {
        Rawlink{p: ptr::mut_null()}
    }

    /// Like Option::Some for Rawlink
    fn some(n: &mut T) -> Rawlink<T> {
        Rawlink{p: n}
    }

    /// Convert the `Rawlink` into an Option value
    fn resolve_immut(&self) -> Option<&T> {
        unsafe { self.p.to_option() }
    }

    /// Convert the `Rawlink` into an Option value
    fn resolve(&mut self) -> Option<&mut T> {
        if self.p.is_null() {
            None
        } else {
            Some(unsafe { cast::transmute(self.p) })
        }
    }

    /// Return the `Rawlink` and replace with `Rawlink::none()`
    fn take(&mut self) -> Rawlink<T> {
        replace(self, Rawlink::none())
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
fn link_with_prev<T>(mut next: ~Node<T>, prev: Rawlink<Node<T>>) -> Link<T> {
    next.prev = prev;
    Some(next)
}

impl<T> Container for DList<T> {
    /// O(1)
    #[inline]
    fn is_empty(&self) -> bool {
        self.list_head.is_none()
    }
    /// O(1)
    #[inline]
    fn len(&self) -> uint {
        self.length
    }
}

impl<T> Mutable for DList<T> {
    /// Remove all elements from the DList
    ///
    /// O(N)
    #[inline]
    fn clear(&mut self) {
        *self = DList::new()
    }
}

// private methods
impl<T> DList<T> {
    /// Add a Node first in the list
    #[inline]
    fn push_front_node(&mut self, mut new_head: ~Node<T>) {
        match self.list_head {
            None => {
                self.list_tail = Rawlink::some(new_head);
                self.list_head = link_with_prev(new_head, Rawlink::none());
            }
            Some(ref mut head) => {
                new_head.prev = Rawlink::none();
                head.prev = Rawlink::some(new_head);
                swap(head, &mut new_head);
                head.next = Some(new_head);
            }
        }
        self.length += 1;
    }

    /// Remove the first Node and return it, or None if the list is empty
    #[inline]
    fn pop_front_node(&mut self) -> Option<~Node<T>> {
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
    fn push_back_node(&mut self, mut new_tail: ~Node<T>) {
        match self.list_tail.resolve() {
            None => return self.push_front_node(new_tail),
            Some(tail) => {
                self.list_tail = Rawlink::some(new_tail);
                tail.next = link_with_prev(new_tail, Rawlink::some(tail));
            }
        }
        self.length += 1;
    }

    /// Remove the last Node and return it, or None if the list is empty
    #[inline]
    fn pop_back_node(&mut self) -> Option<~Node<T>> {
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

impl<T> Deque<T> for DList<T> {
    /// Provide a reference to the front element, or None if the list is empty
    #[inline]
    fn front<'a>(&'a self) -> Option<&'a T> {
        self.list_head.as_ref().map(|head| &head.value)
    }

    /// Provide a mutable reference to the front element, or None if the list is empty
    #[inline]
    fn front_mut<'a>(&'a mut self) -> Option<&'a mut T> {
        self.list_head.as_mut().map(|head| &mut head.value)
    }

    /// Provide a reference to the back element, or None if the list is empty
    #[inline]
    fn back<'a>(&'a self) -> Option<&'a T> {
        let tmp = self.list_tail.resolve_immut(); // FIXME: #3511: shouldn't need variable
        tmp.as_ref().map(|tail| &tail.value)
    }

    /// Provide a mutable reference to the back element, or None if the list is empty
    #[inline]
    fn back_mut<'a>(&'a mut self) -> Option<&'a mut T> {
        let tmp: Option<&'a mut Node<T>> =
            self.list_tail.resolve(); // FIXME: #3511: shouldn't need variable
        tmp.map(|tail| &mut tail.value)
    }

    /// Add an element first in the list
    ///
    /// O(1)
    fn push_front(&mut self, elt: T) {
        self.push_front_node(~Node::new(elt))
    }

    /// Remove the first element and return it, or None if the list is empty
    ///
    /// O(1)
    fn pop_front(&mut self) -> Option<T> {
        self.pop_front_node().map(|~Node{value, ..}| value)
    }

    /// Add an element last in the list
    ///
    /// O(1)
    fn push_back(&mut self, elt: T) {
        self.push_back_node(~Node::new(elt))
    }

    /// Remove the last element and return it, or None if the list is empty
    ///
    /// O(1)
    fn pop_back(&mut self) -> Option<T> {
        self.pop_back_node().map(|~Node{value, ..}| value)
    }
}

impl<T> DList<T> {
    /// Create an empty DList
    #[inline]
    pub fn new() -> DList<T> {
        DList{list_head: None, list_tail: Rawlink::none(), length: 0}
    }

    /// Move the last element to the front of the list.
    ///
    /// If the list is empty, do nothing.
    #[inline]
    pub fn rotate_forward(&mut self) {
        self.pop_back_node().map(|tail| {
            self.push_front_node(tail)
        });
    }

    /// Move the first element to the back of the list.
    ///
    /// If the list is empty, do nothing.
    #[inline]
    pub fn rotate_backward(&mut self) {
        self.pop_front_node().map(|head| {
            self.push_back_node(head)
        });
    }

    /// Add all elements from `other` to the end of the list
    ///
    /// O(1)
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

    /// Add all elements from `other` to the beginning of the list
    ///
    /// O(1)
    #[inline]
    pub fn prepend(&mut self, mut other: DList<T>) {
        swap(self, &mut other);
        self.append(other);
    }

    /// Insert `elt` before the first `x` in the list where `f(x, elt)` is true,
    /// or at the end.
    ///
    /// O(N)
    pub fn insert_when(&mut self, elt: T, f: |&T, &T| -> bool) {
        {
            let mut it = self.mut_iter();
            loop {
                match it.peek_next() {
                    None => break,
                    Some(x) => if f(x, &elt) { break }
                }
                it.next();
            }
            it.insert_next(elt);
        }
    }

    /// Merge DList `other` into this DList, using the function `f`.
    /// Iterate the both DList with `a` from self and `b` from `other`, and
    /// put `a` in the result if `f(a, b)` is true, else `b`.
    ///
    /// O(max(N, M))
    pub fn merge(&mut self, mut other: DList<T>, f: |&T, &T| -> bool) {
        {
            let mut it = self.mut_iter();
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


    /// Provide a forward iterator
    #[inline]
    pub fn iter<'a>(&'a self) -> Items<'a, T> {
        Items{nelem: self.len(), head: &self.list_head, tail: self.list_tail}
    }

    /// Provide a reverse iterator
    #[inline]
    pub fn rev_iter<'a>(&'a self) -> Rev<Items<'a, T>> {
        self.iter().rev()
    }

    /// Provide a forward iterator with mutable references
    #[inline]
    pub fn mut_iter<'a>(&'a mut self) -> MutItems<'a, T> {
        let head_raw = match self.list_head {
            Some(ref mut h) => Rawlink::some(*h),
            None => Rawlink::none(),
        };
        MutItems{
            nelem: self.len(),
            head: head_raw,
            tail: self.list_tail,
            list: self
        }
    }
    /// Provide a reverse iterator with mutable references
    #[inline]
    pub fn mut_rev_iter<'a>(&'a mut self) -> Rev<MutItems<'a, T>> {
        self.mut_iter().rev()
    }


    /// Consume the list into an iterator yielding elements by value
    #[inline]
    pub fn move_iter(self) -> MoveItems<T> {
        MoveItems{list: self}
    }

    /// Consume the list into an iterator yielding elements by value, in reverse
    #[inline]
    pub fn move_rev_iter(self) -> Rev<MoveItems<T>> {
        self.move_iter().rev()
    }
}

impl<T: Ord> DList<T> {
    /// Insert `elt` sorted in ascending order
    ///
    /// O(N)
    #[inline]
    pub fn insert_ordered(&mut self, elt: T) {
        self.insert_when(elt, |a, b| a >= b)
    }
}

#[unsafe_destructor]
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
                    prev.next.take(); // release ~Node<T>
                    tail = prev.prev;
                }
            }
        }
        self.length = 0;
        self.list_head = None;
        self.list_tail = Rawlink::none();
    }
}


impl<'a, A> Iterator<&'a A> for Items<'a, A> {
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

impl<'a, A> DoubleEndedIterator<&'a A> for Items<'a, A> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a A> {
        if self.nelem == 0 {
            return None;
        }
        let tmp = self.tail.resolve_immut(); // FIXME: #3511: shouldn't need variable
        tmp.as_ref().map(|prev| {
            self.nelem -= 1;
            self.tail = prev.prev;
            &prev.value
        })
    }
}

impl<'a, A> ExactSize<&'a A> for Items<'a, A> {}

impl<'a, A> Iterator<&'a mut A> for MutItems<'a, A> {
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

impl<'a, A> DoubleEndedIterator<&'a mut A> for MutItems<'a, A> {
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

impl<'a, A> ExactSize<&'a mut A> for MutItems<'a, A> {}

/// Allow mutating the DList while iterating
pub trait ListInsertion<A> {
    /// Insert `elt` just after to the element most recently returned by `.next()`
    ///
    /// The inserted element does not appear in the iteration.
    fn insert_next(&mut self, elt: A);

    /// Provide a reference to the next element, without changing the iterator
    fn peek_next<'a>(&'a mut self) -> Option<&'a mut A>;
}

// private methods for MutItems
impl<'a, A> MutItems<'a, A> {
    fn insert_next_node(&mut self, mut ins_node: ~Node<A>) {
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
                let node_own = prev_node.next.take_unwrap();
                ins_node.next = link_with_prev(node_own, Rawlink::some(ins_node));
                prev_node.next = link_with_prev(ins_node, Rawlink::some(prev_node));
                self.list.length += 1;
            }
        }
    }
}

impl<'a, A> ListInsertion<A> for MutItems<'a, A> {
    #[inline]
    fn insert_next(&mut self, elt: A) {
        self.insert_next_node(~Node::new(elt))
    }

    #[inline]
    fn peek_next<'a>(&'a mut self) -> Option<&'a mut A> {
        if self.nelem == 0 {
            return None
        }
        self.head.resolve().map(|head| &mut head.value)
    }
}

impl<A> Iterator<A> for MoveItems<A> {
    #[inline]
    fn next(&mut self) -> Option<A> { self.list.pop_front() }

    #[inline]
    fn size_hint(&self) -> (uint, Option<uint>) {
        (self.list.length, Some(self.list.length))
    }
}

impl<A> DoubleEndedIterator<A> for MoveItems<A> {
    #[inline]
    fn next_back(&mut self) -> Option<A> { self.list.pop_back() }
}

impl<A> FromIterator<A> for DList<A> {
    fn from_iterator<T: Iterator<A>>(iterator: &mut T) -> DList<A> {
        let mut ret = DList::new();
        ret.extend(iterator);
        ret
    }
}

impl<A> Extendable<A> for DList<A> {
    fn extend<T: Iterator<A>>(&mut self, iterator: &mut T) {
        for elt in *iterator { self.push_back(elt); }
    }
}

impl<A: Eq> Eq for DList<A> {
    fn eq(&self, other: &DList<A>) -> bool {
        self.len() == other.len() &&
            iter::order::eq(self.iter(), other.iter())
    }

    fn ne(&self, other: &DList<A>) -> bool {
        self.len() != other.len() ||
            iter::order::ne(self.iter(), other.iter())
    }
}

impl<A: Eq + Ord> Ord for DList<A> {
    fn lt(&self, other: &DList<A>) -> bool {
        iter::order::lt(self.iter(), other.iter())
    }
    fn le(&self, other: &DList<A>) -> bool {
        iter::order::le(self.iter(), other.iter())
    }
    fn gt(&self, other: &DList<A>) -> bool {
        iter::order::gt(self.iter(), other.iter())
    }
    fn ge(&self, other: &DList<A>) -> bool {
        iter::order::ge(self.iter(), other.iter())
    }
}

impl<A: Clone> Clone for DList<A> {
    fn clone(&self) -> DList<A> {
        self.iter().map(|x| x.clone()).collect()
    }
}

impl<
    S: Encoder,
    T: Encodable<S>
> Encodable<S> for DList<T> {
    fn encode(&self, s: &mut S) {
        s.emit_seq(self.len(), |s| {
            for (i, e) in self.iter().enumerate() {
                s.emit_seq_elt(i, |s| e.encode(s));
            }
        })
    }
}

impl<D:Decoder,T:Decodable<D>> Decodable<D> for DList<T> {
    fn decode(d: &mut D) -> DList<T> {
        let mut list = DList::new();
        d.read_seq(|d, len| {
            for i in range(0u, len) {
                list.push_back(d.read_seq_elt(i, |d| Decodable::decode(d)));
            }
        });
        list
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use self::test::BenchHarness;
    use deque::Deque;
    use std::rand;
    use super::{DList, Node, ListInsertion};

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
                (None   , _         ) => fail!("prev link for list_head"),
                (Some(p), Some(pptr)) => {
                    assert_eq!(p as *Node<T>, pptr as *Node<T>);
                }
                _ => fail!("prev link is none, not good"),
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
        let mut m: DList<~int> = DList::new();
        assert_eq!(m.pop_front(), None);
        assert_eq!(m.pop_back(), None);
        assert_eq!(m.pop_front(), None);
        m.push_front(~1);
        assert_eq!(m.pop_front(), Some(~1));
        m.push_back(~2);
        m.push_back(~3);
        assert_eq!(m.len(), 2);
        assert_eq!(m.pop_front(), Some(~2));
        assert_eq!(m.pop_front(), Some(~3));
        assert_eq!(m.len(), 0);
        assert_eq!(m.pop_front(), None);
        m.push_back(~1);
        m.push_back(~3);
        m.push_back(~5);
        m.push_back(~7);
        assert_eq!(m.pop_front(), Some(~1));

        let mut n = DList::new();
        n.push_front(2);
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
        list_from(&[0,1,2,3,4,5,6])
    }

    #[cfg(test)]
    fn list_from<T: Clone>(v: &[T]) -> DList<T> {
        v.iter().map(|x| (*x).clone()).collect()
    }

    #[test]
    fn test_append() {
        {
            let mut m = DList::new();
            let mut n = DList::new();
            n.push_back(2);
            m.append(n);
            assert_eq!(m.len(), 1);
            assert_eq!(m.pop_back(), Some(2));
            check_links(&m);
        }
        {
            let mut m = DList::new();
            let n = DList::new();
            m.push_back(2);
            m.append(n);
            assert_eq!(m.len(), 1);
            assert_eq!(m.pop_back(), Some(2));
            check_links(&m);
        }

        let v = ~[1,2,3,4,5];
        let u = ~[9,8,1,2,3,4,5];
        let mut m = list_from(v);
        m.append(list_from(u));
        check_links(&m);
        let sum = v + u;
        assert_eq!(sum.len(), m.len());
        for elt in sum.move_iter() {
            assert_eq!(m.pop_front(), Some(elt))
        }
    }

    #[test]
    fn test_prepend() {
        {
            let mut m = DList::new();
            let mut n = DList::new();
            n.push_back(2);
            m.prepend(n);
            assert_eq!(m.len(), 1);
            assert_eq!(m.pop_back(), Some(2));
            check_links(&m);
        }

        let v = ~[1,2,3,4,5];
        let u = ~[9,8,1,2,3,4,5];
        let mut m = list_from(v);
        m.prepend(list_from(u));
        check_links(&m);
        let sum = u + v;
        assert_eq!(sum.len(), m.len());
        for elt in sum.move_iter() {
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

        let v = ~[1,2,3,4,5];
        let mut m = list_from(v);
        m.rotate_backward(); check_links(&m);
        m.rotate_forward(); check_links(&m);
        assert_eq!(v.iter().collect::<~[&int]>(), m.iter().collect());
        m.rotate_forward(); check_links(&m);
        m.rotate_forward(); check_links(&m);
        m.pop_front(); check_links(&m);
        m.rotate_forward(); check_links(&m);
        m.rotate_backward(); check_links(&m);
        m.push_front(9); check_links(&m);
        m.rotate_forward(); check_links(&m);
        assert_eq!(~[3,9,5,1,2], m.move_iter().collect());
    }

    #[test]
    fn test_iterator() {
        let m = generate_test();
        for (i, elt) in m.iter().enumerate() {
            assert_eq!(i as int, *elt);
        }
        let mut n = DList::new();
        assert_eq!(n.iter().next(), None);
        n.push_front(4);
        let mut it = n.iter();
        assert_eq!(it.size_hint(), (1, Some(1)));
        assert_eq!(it.next().unwrap(), &4);
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_iterator_clone() {
        let mut n = DList::new();
        n.push_back(2);
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
        n.push_front(4);
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
        for (i, elt) in m.rev_iter().enumerate() {
            assert_eq!((6 - i) as int, *elt);
        }
        let mut n = DList::new();
        assert_eq!(n.rev_iter().next(), None);
        n.push_front(4);
        let mut it = n.rev_iter();
        assert_eq!(it.size_hint(), (1, Some(1)));
        assert_eq!(it.next().unwrap(), &4);
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_mut_iter() {
        let mut m = generate_test();
        let mut len = m.len();
        for (i, elt) in m.mut_iter().enumerate() {
            assert_eq!(i as int, *elt);
            len -= 1;
        }
        assert_eq!(len, 0);
        let mut n = DList::new();
        assert!(n.mut_iter().next().is_none());
        n.push_front(4);
        n.push_back(5);
        let mut it = n.mut_iter();
        assert_eq!(it.size_hint(), (2, Some(2)));
        assert!(it.next().is_some());
        assert!(it.next().is_some());
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert!(it.next().is_none());
    }

    #[test]
    fn test_iterator_mut_double_end() {
        let mut n = DList::new();
        assert!(n.mut_iter().next_back().is_none());
        n.push_front(4);
        n.push_front(5);
        n.push_front(6);
        let mut it = n.mut_iter();
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
        let mut m = list_from(&[0,2,4,6,8]);
        let len = m.len();
        {
            let mut it = m.mut_iter();
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
        assert_eq!(m.move_iter().collect::<~[int]>(), ~[-2,0,1,2,3,4,5,6,7,8,9,0,1]);
    }

    #[test]
    fn test_merge() {
        let mut m = list_from([0, 1, 3, 5, 6, 7, 2]);
        let n = list_from([-1, 0, 0, 7, 7, 9]);
        let len = m.len() + n.len();
        m.merge(n, |a, b| a <= b);
        assert_eq!(m.len(), len);
        check_links(&m);
        let res = m.move_iter().collect::<~[int]>();
        assert_eq!(res, ~[-1, 0, 0, 0, 1, 3, 5, 6, 7, 2, 7, 7, 9]);
    }

    #[test]
    fn test_insert_ordered() {
        let mut n = DList::new();
        n.insert_ordered(1);
        assert_eq!(n.len(), 1);
        assert_eq!(n.pop_front(), Some(1));

        let mut m = DList::new();
        m.push_back(2);
        m.push_back(4);
        m.insert_ordered(3);
        check_links(&m);
        assert_eq!(~[2,3,4], m.move_iter().collect::<~[int]>());
    }

    #[test]
    fn test_mut_rev_iter() {
        let mut m = generate_test();
        for (i, elt) in m.mut_rev_iter().enumerate() {
            assert_eq!((6-i) as int, *elt);
        }
        let mut n = DList::new();
        assert!(n.mut_rev_iter().next().is_none());
        n.push_front(4);
        let mut it = n.mut_rev_iter();
        assert!(it.next().is_some());
        assert!(it.next().is_none());
    }

    #[test]
    fn test_send() {
        let n = list_from([1,2,3]);
        spawn(proc() {
            check_links(&n);
            assert_eq!(~[&1,&2,&3], n.iter().collect::<~[&int]>());
        });
    }

    #[test]
    fn test_eq() {
        let mut n: DList<u8> = list_from([]);
        let mut m = list_from([]);
        assert_eq!(&n, &m);
        n.push_front(1);
        assert!(n != m);
        m.push_back(1);
        assert_eq!(&n, &m);

        let n = list_from([2,3,4]);
        let m = list_from([1,2,3]);
        assert!(n != m);
    }

    #[test]
    fn test_ord() {
        let n: DList<int> = list_from([]);
        let m = list_from([1,2,3]);
        assert!(n < m);
        assert!(m > n);
        assert!(n <= n);
        assert!(n >= n);
    }

    #[test]
    fn test_ord_nan() {
        let nan = 0.0/0.0;
        let n = list_from([nan]);
        let m = list_from([nan]);
        assert!(!(n < m));
        assert!(!(n > m));
        assert!(!(n <= m));
        assert!(!(n >= m));

        let n = list_from([nan]);
        let one = list_from([1.0]);
        assert!(!(n < one));
        assert!(!(n > one));
        assert!(!(n <= one));
        assert!(!(n >= one));

        let u = list_from([1.0,2.0,nan]);
        let v = list_from([1.0,2.0,3.0]);
        assert!(!(u < v));
        assert!(!(u > v));
        assert!(!(u <= v));
        assert!(!(u >= v));

        let s = list_from([1.0,2.0,4.0,2.0]);
        let t = list_from([1.0,2.0,3.0,2.0]);
        assert!(!(s < t));
        assert!(s > one);
        assert!(!(s <= one));
        assert!(s >= one);
    }

    #[test]
    fn test_fuzz() {
        for _ in range(0, 25) {
            fuzz_test(3);
            fuzz_test(16);
            fuzz_test(189);
        }
    }

    #[cfg(test)]
    fn fuzz_test(sz: int) {
        let mut m: DList<int> = DList::new();
        let mut v = ~[];
        for i in range(0, sz) {
            check_links(&m);
            let r: u8 = rand::random();
            match r % 6 {
                0 => {
                    m.pop_back();
                    v.pop();
                }
                1 => {
                    m.pop_front();
                    v.shift();
                }
                2 | 4 =>  {
                    m.push_front(-i);
                    v.unshift(-i);
                }
                3 | 5 | _ => {
                    m.push_back(i);
                    v.push(i);
                }
            }
        }

        check_links(&m);

        let mut i = 0u;
        for (a, &b) in m.move_iter().zip(v.iter()) {
            i += 1;
            assert_eq!(a, b);
        }
        assert_eq!(i, v.len());
    }

    #[bench]
    fn bench_collect_into(b: &mut test::BenchHarness) {
        let v = &[0, ..64];
        b.iter(|| {
            let _: DList<int> = v.iter().map(|x| *x).collect();
        })
    }

    #[bench]
    fn bench_push_front(b: &mut test::BenchHarness) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_front(0);
        })
    }

    #[bench]
    fn bench_push_back(b: &mut test::BenchHarness) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_back(0);
        })
    }

    #[bench]
    fn bench_push_back_pop_back(b: &mut test::BenchHarness) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_back(0);
            m.pop_back();
        })
    }

    #[bench]
    fn bench_push_front_pop_front(b: &mut test::BenchHarness) {
        let mut m: DList<int> = DList::new();
        b.iter(|| {
            m.push_front(0);
            m.pop_front();
        })
    }

    #[bench]
    fn bench_rotate_forward(b: &mut test::BenchHarness) {
        let mut m: DList<int> = DList::new();
        m.push_front(0);
        m.push_front(1);
        b.iter(|| {
            m.rotate_forward();
        })
    }

    #[bench]
    fn bench_rotate_backward(b: &mut test::BenchHarness) {
        let mut m: DList<int> = DList::new();
        m.push_front(0);
        m.push_front(1);
        b.iter(|| {
            m.rotate_backward();
        })
    }

    #[bench]
    fn bench_iter(b: &mut test::BenchHarness) {
        let v = &[0, ..128];
        let m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.iter().len() == 128);
        })
    }
    #[bench]
    fn bench_iter_mut(b: &mut test::BenchHarness) {
        let v = &[0, ..128];
        let mut m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.mut_iter().len() == 128);
        })
    }
    #[bench]
    fn bench_iter_rev(b: &mut test::BenchHarness) {
        let v = &[0, ..128];
        let m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.rev_iter().len() == 128);
        })
    }
    #[bench]
    fn bench_iter_mut_rev(b: &mut test::BenchHarness) {
        let v = &[0, ..128];
        let mut m: DList<int> = v.iter().map(|&x|x).collect();
        b.iter(|| {
            assert!(m.mut_rev_iter().len() == 128);
        })
    }
}
