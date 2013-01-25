// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

A doubly-linked list. Supports O(1) head, tail, count, push, pop, etc.

# Safety note

Do not use ==, !=, <, etc on doubly-linked lists -- it may not terminate.

*/

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

use kinds::Copy;
use managed;
use option::{None, Option, Some};
use option;
use vec;

pub type DListLink<T> = Option<@DListNode<T>>;

pub struct DListNode<T> {
    data: T,
    mut linked: bool, // for assertions
    mut prev: DListLink<T>,
    mut next: DListLink<T>,
}

pub struct DList<T> {
    mut size: uint,
    mut hd: DListLink<T>,
    mut tl: DListLink<T>,
}

priv impl<T> DListNode<T> {
    pure fn assert_links(@self) {
        match self.next {
            Some(neighbour) => match neighbour.prev {
              Some(me) => if !managed::ptr_eq(self, me) {
                  fail ~"Asymmetric next-link in dlist node."
              },
              None => fail ~"One-way next-link in dlist node."
            },
            None => ()
        }
        match self.prev {
            Some(neighbour) => match neighbour.next {
              Some(me) => if !managed::ptr_eq(me, self) {
                  fail ~"Asymmetric prev-link in dlist node."
              },
              None => fail ~"One-way prev-link in dlist node."
            },
            None => ()
        }
    }
}

impl<T> DListNode<T> {
    /// Get the next node in the list, if there is one.
    pure fn next_link(@self) -> DListLink<T> {
        self.assert_links();
        self.next
    }
    /// Get the next node in the list, failing if there isn't one.
    pure fn next_node(@self) -> @DListNode<T> {
        match self.next_link() {
            Some(nobe) => nobe,
            None       => fail ~"This dlist node has no next neighbour."
        }
    }
    /// Get the previous node in the list, if there is one.
    pure fn prev_link(@self) -> DListLink<T> {
        self.assert_links();
        self.prev
    }
    /// Get the previous node in the list, failing if there isn't one.
    pure fn prev_node(@self) -> @DListNode<T> {
        match self.prev_link() {
            Some(nobe) => nobe,
            None       => fail ~"This dlist node has no previous neighbour."
        }
    }
}

/// Creates a new dlist node with the given data.
pub pure fn new_dlist_node<T>(data: T) -> @DListNode<T> {
    @DListNode { data: data, linked: false, prev: None, next: None }
}

/// Creates a new, empty dlist.
pub pure fn DList<T>() -> @DList<T> {
    @DList { size: 0, hd: None, tl: None }
}

/// Creates a new dlist with a single element
pub pure fn from_elem<T>(data: T) -> @DList<T> {
    let list = DList();
    unsafe { list.push(data); }
    list
}

pub fn from_vec<T: Copy>(vec: &[T]) -> @DList<T> {
    do vec::foldl(DList(), vec) |list,data| {
        list.push(*data); // Iterating left-to-right -- add newly to the tail.
        list
    }
}

/// Produce a list from a list of lists, leaving no elements behind in the
/// input. O(number of sub-lists).
pub fn concat<T>(lists: @DList<@DList<T>>) -> @DList<T> {
    let result = DList();
    while !lists.is_empty() {
        result.append(lists.pop().get());
    }
    result
}

priv impl<T> DList<T> {
    static pure fn new_link(data: T) -> DListLink<T> {
        Some(@DListNode { data: data, linked: true, prev: None, next: None })
    }
    pure fn assert_mine(@self, nobe: @DListNode<T>) {
        // These asserts could be stronger if we had node-root back-pointers,
        // but those wouldn't allow for O(1) append.
        if self.size == 0 {
            fail ~"This dlist is empty; that node can't be on it."
        }
        if !nobe.linked { fail ~"That node isn't linked to any dlist." }
        if !((nobe.prev.is_some()
              || managed::ptr_eq(self.hd.expect(~"headless dlist?"),
                                 nobe)) &&
             (nobe.next.is_some()
              || managed::ptr_eq(self.tl.expect(~"tailless dlist?"),
                                 nobe))) {
            fail ~"That node isn't on this dlist."
        }
    }
    fn make_mine(nobe: @DListNode<T>) {
        if nobe.prev.is_some() || nobe.next.is_some() || nobe.linked {
            fail ~"Cannot insert node that's already on a dlist!"
        }
        nobe.linked = true;
    }
    // Link two nodes together. If either of them are 'none', also sets
    // the head and/or tail pointers appropriately.
    #[inline(always)]
    fn link(before: DListLink<T>, after: DListLink<T>) {
        match before {
            Some(neighbour) => neighbour.next = after,
            None            => self.hd        = after
        }
        match after {
            Some(neighbour) => neighbour.prev = before,
            None            => self.tl        = before
        }
    }
    // Remove a node from the list.
    fn unlink(@self, nobe: @DListNode<T>) {
        self.assert_mine(nobe);
        assert self.size > 0;
        self.link(nobe.prev, nobe.next);
        nobe.prev = None; // Release extraneous references.
        nobe.next = None;
        nobe.linked = false;
        self.size -= 1;
    }

    fn add_head(@self, nobe: DListLink<T>) {
        self.link(nobe, self.hd); // Might set tail too.
        self.hd = nobe;
        self.size += 1;
    }
    fn add_tail(@self, nobe: DListLink<T>) {
        self.link(self.tl, nobe); // Might set head too.
        self.tl = nobe;
        self.size += 1;
    }
    fn insert_left(@self, nobe: DListLink<T>, neighbour: @DListNode<T>) {
        self.assert_mine(neighbour);
        assert self.size > 0;
        self.link(neighbour.prev, nobe);
        self.link(nobe, Some(neighbour));
        self.size += 1;
    }
    fn insert_right(@self, neighbour: @DListNode<T>, nobe: DListLink<T>) {
        self.assert_mine(neighbour);
        assert self.size > 0;
        self.link(nobe, neighbour.next);
        self.link(Some(neighbour), nobe);
        self.size += 1;
    }
}

impl<T> DList<T> {
    /// Get the size of the list. O(1).
    pure fn len(@self) -> uint { self.size }
    /// Returns true if the list is empty. O(1).
    pure fn is_empty(@self) -> bool { self.len() == 0 }
    /// Returns true if the list is not empty. O(1).
    pure fn is_not_empty(@self) -> bool { self.len() != 0 }

    /// Add data to the head of the list. O(1).
    fn push_head(@self, data: T) {
        self.add_head(DList::new_link(data));
    }
    /**
     * Add data to the head of the list, and get the new containing
     * node. O(1).
     */
    fn push_head_n(@self, data: T) -> @DListNode<T> {
        let mut nobe = DList::new_link(data);
        self.add_head(nobe);
        nobe.get()
    }
    /// Add data to the tail of the list. O(1).
    fn push(@self, data: T) {
        self.add_tail(DList::new_link(data));
    }
    /**
     * Add data to the tail of the list, and get the new containing
     * node. O(1).
     */
    fn push_n(@self, data: T) -> @DListNode<T> {
        let mut nobe = DList::new_link(data);
        self.add_tail(nobe);
        nobe.get()
    }
    /**
     * Insert data into the middle of the list, left of the given node.
     * O(1).
     */
    fn insert_before(@self, data: T, neighbour: @DListNode<T>) {
        self.insert_left(DList::new_link(data), neighbour);
    }
    /**
     * Insert an existing node in the middle of the list, left of the
     * given node. O(1).
     */
    fn insert_n_before(@self, nobe: @DListNode<T>, neighbour: @DListNode<T>) {
        self.make_mine(nobe);
        self.insert_left(Some(nobe), neighbour);
    }
    /**
     * Insert data in the middle of the list, left of the given node,
     * and get its containing node. O(1).
     */
    fn insert_before_n(
        @self,
        data: T,
        neighbour: @DListNode<T>
    ) -> @DListNode<T> {
        let mut nobe = DList::new_link(data);
        self.insert_left(nobe, neighbour);
        nobe.get()
    }
    /**
     * Insert data into the middle of the list, right of the given node.
     * O(1).
     */
    fn insert_after(@self, data: T, neighbour: @DListNode<T>) {
        self.insert_right(neighbour, DList::new_link(data));
    }
    /**
     * Insert an existing node in the middle of the list, right of the
     * given node. O(1).
     */
    fn insert_n_after(@self, nobe: @DListNode<T>, neighbour: @DListNode<T>) {
        self.make_mine(nobe);
        self.insert_right(neighbour, Some(nobe));
    }
    /**
     * Insert data in the middle of the list, right of the given node,
     * and get its containing node. O(1).
     */
    fn insert_after_n(
        @self,
        data: T,
        neighbour: @DListNode<T>
    ) -> @DListNode<T> {
        let mut nobe = DList::new_link(data);
        self.insert_right(neighbour, nobe);
        nobe.get()
    }

    /// Remove a node from the head of the list. O(1).
    fn pop_n(@self) -> DListLink<T> {
        let hd = self.peek_n();
        hd.map(|nobe| self.unlink(*nobe));
        hd
    }
    /// Remove a node from the tail of the list. O(1).
    fn pop_tail_n(@self) -> DListLink<T> {
        let tl = self.peek_tail_n();
        tl.map(|nobe| self.unlink(*nobe));
        tl
    }
    /// Get the node at the list's head. O(1).
    pure fn peek_n(@self) -> DListLink<T> { self.hd }
    /// Get the node at the list's tail. O(1).
    pure fn peek_tail_n(@self) -> DListLink<T> { self.tl }

    /// Get the node at the list's head, failing if empty. O(1).
    pure fn head_n(@self) -> @DListNode<T> {
        match self.hd {
            Some(nobe) => nobe,
            None       => fail ~"Attempted to get the head of an empty dlist."
        }
    }
    /// Get the node at the list's tail, failing if empty. O(1).
    pure fn tail_n(@self) -> @DListNode<T> {
        match self.tl {
            Some(nobe) => nobe,
            None       => fail ~"Attempted to get the tail of an empty dlist."
        }
    }

    /// Remove a node from anywhere in the list. O(1).
    fn remove(@self, nobe: @DListNode<T>) { self.unlink(nobe); }

    /**
     * Empty another list onto the end of this list, joining this list's tail
     * to the other list's head. O(1).
     */
    fn append(@self, them: @DList<T>) {
        if managed::ptr_eq(self, them) {
            fail ~"Cannot append a dlist to itself!"
        }
        if them.len() > 0 {
            self.link(self.tl, them.hd);
            self.tl    = them.tl;
            self.size += them.size;
            them.size  = 0;
            them.hd    = None;
            them.tl    = None;
        }
    }
    /**
     * Empty another list onto the start of this list, joining the other
     * list's tail to this list's head. O(1).
     */
    fn prepend(@self, them: @DList<T>) {
        if managed::ptr_eq(self, them) {
            fail ~"Cannot prepend a dlist to itself!"
        }
        if them.len() > 0 {
            self.link(them.tl, self.hd);
            self.hd    = them.hd;
            self.size += them.size;
            them.size  = 0;
            them.hd    = None;
            them.tl    = None;
        }
    }

    /// Reverse the list's elements in place. O(n).
    fn reverse(@self) {
        do option::while_some(self.hd) |nobe| {
            let next_nobe = nobe.next;
            self.remove(nobe);
            self.make_mine(nobe);
            self.add_head(Some(nobe));
            next_nobe
        }
    }

    /**
     * Remove everything from the list. This is important because the cyclic
     * links won't otherwise be automatically refcounted-collected. O(n).
     */
    fn clear(@self) {
        // Cute as it would be to simply detach the list and proclaim "O(1)!",
        // the GC would still be a hidden O(n). Better to be honest about it.
        while !self.is_empty() {
            let _ = self.pop_n();
        }
    }

    /// Iterate over nodes.
    pure fn each_node(@self, f: fn(@DListNode<T>) -> bool) {
        let mut link = self.peek_n();
        while link.is_some() {
            let nobe = link.get();
            if !f(nobe) { break; }
            link = nobe.next_link();
        }
    }

    /// Check data structure integrity. O(n).
    fn assert_consistent(@self) {
        if self.hd.is_none() || self.tl.is_none() {
            assert self.hd.is_none() && self.tl.is_none();
        }
        // iterate forwards
        let mut count = 0;
        let mut link = self.peek_n();
        let mut rabbit = link;
        while link.is_some() {
            let nobe = link.get();
            assert nobe.linked;
            // check cycle
            if rabbit.is_some() {
                rabbit = rabbit.get().next;
            }
            if rabbit.is_some() {
                rabbit = rabbit.get().next;
            }
            if rabbit.is_some() {
                assert !managed::ptr_eq(rabbit.get(), nobe);
            }
            // advance
            link = nobe.next_link();
            count += 1;
        }
        assert count == self.len();
        // iterate backwards - some of this is probably redundant.
        link = self.peek_tail_n();
        rabbit = link;
        while link.is_some() {
            let nobe = link.get();
            assert nobe.linked;
            // check cycle
            if rabbit.is_some() {
                rabbit = rabbit.get().prev;
            }
            if rabbit.is_some() {
                rabbit = rabbit.get().prev;
            }
            if rabbit.is_some() {
                assert !managed::ptr_eq(rabbit.get(), nobe);
            }
            // advance
            link = nobe.prev_link();
            count -= 1;
        }
        assert count == 0;
    }
}

impl<T: Copy> DList<T> {
    /// Remove data from the head of the list. O(1).
    fn pop(@self) -> Option<T> {
        self.pop_n().map(|nobe| nobe.data)
    }

    /// Remove data from the tail of the list. O(1).
    fn pop_tail(@self) -> Option<T> {
        self.pop_tail_n().map(|nobe| nobe.data)
    }

    /// Get data at the list's head. O(1).
    pure fn peek(@self) -> Option<T> {
        self.peek_n().map(|nobe| nobe.data)
    }

    /// Get data at the list's tail. O(1).
    pure fn peek_tail(@self) -> Option<T> {
        self.peek_tail_n().map (|nobe| nobe.data)
    }

    /// Get data at the list's head, failing if empty. O(1).
    pure fn head(@self) -> T { self.head_n().data }

    /// Get data at the list's tail, failing if empty. O(1).
    pure fn tail(@self) -> T { self.tail_n().data }

    /// Get the elements of the list as a vector. O(n).
    pure fn to_vec(@self) -> ~[T] {
        let mut v = vec::with_capacity(self.size);
        unsafe {
            // Take this out of the unchecked when iter's functions are pure
            for self.eachi |index,data| {
                v[index] = *data;
            }
        }
        move v
    }
}

#[cfg(test)]
mod tests {
    #[legacy_exports];

    use dlist::{DList, concat, from_vec, new_dlist_node};
    use iter;
    use option::{None, Some};
    use vec;

    #[test]
    fn test_dlist_concat() {
        let a = from_vec(~[1,2]);
        let b = from_vec(~[3,4]);
        let c = from_vec(~[5,6]);
        let d = from_vec(~[7,8]);
        let ab = from_vec(~[a,b]);
        let cd = from_vec(~[c,d]);
        let abcd = concat(concat(from_vec(~[ab,cd])));
        abcd.assert_consistent(); assert abcd.len() == 8;
        abcd.assert_consistent(); assert abcd.pop().get() == 1;
        abcd.assert_consistent(); assert abcd.pop().get() == 2;
        abcd.assert_consistent(); assert abcd.pop().get() == 3;
        abcd.assert_consistent(); assert abcd.pop().get() == 4;
        abcd.assert_consistent(); assert abcd.pop().get() == 5;
        abcd.assert_consistent(); assert abcd.pop().get() == 6;
        abcd.assert_consistent(); assert abcd.pop().get() == 7;
        abcd.assert_consistent(); assert abcd.pop().get() == 8;
        abcd.assert_consistent(); assert abcd.is_empty();
    }
    #[test]
    fn test_dlist_append() {
        let a = from_vec(~[1,2,3]);
        let b = from_vec(~[4,5,6]);
        a.append(b);
        assert a.len() == 6;
        assert b.len() == 0;
        b.assert_consistent();
        a.assert_consistent(); assert a.pop().get() == 1;
        a.assert_consistent(); assert a.pop().get() == 2;
        a.assert_consistent(); assert a.pop().get() == 3;
        a.assert_consistent(); assert a.pop().get() == 4;
        a.assert_consistent(); assert a.pop().get() == 5;
        a.assert_consistent(); assert a.pop().get() == 6;
        a.assert_consistent(); assert a.is_empty();
    }
    #[test]
    fn test_dlist_append_empty() {
        let a = from_vec(~[1,2,3]);
        let b = DList::<int>();
        a.append(b);
        assert a.len() == 3;
        assert b.len() == 0;
        b.assert_consistent();
        a.assert_consistent(); assert a.pop().get() == 1;
        a.assert_consistent(); assert a.pop().get() == 2;
        a.assert_consistent(); assert a.pop().get() == 3;
        a.assert_consistent(); assert a.is_empty();
    }
    #[test]
    fn test_dlist_append_to_empty() {
        let a = DList::<int>();
        let b = from_vec(~[4,5,6]);
        a.append(b);
        assert a.len() == 3;
        assert b.len() == 0;
        b.assert_consistent();
        a.assert_consistent(); assert a.pop().get() == 4;
        a.assert_consistent(); assert a.pop().get() == 5;
        a.assert_consistent(); assert a.pop().get() == 6;
        a.assert_consistent(); assert a.is_empty();
    }
    #[test]
    fn test_dlist_append_two_empty() {
        let a = DList::<int>();
        let b = DList::<int>();
        a.append(b);
        assert a.len() == 0;
        assert b.len() == 0;
        b.assert_consistent();
        a.assert_consistent();
    }
    #[test]
    #[ignore(cfg(windows))]
    #[should_fail]
    fn test_dlist_append_self() {
        let a = DList::<int>();
        a.append(a);
    }
    #[test]
    #[ignore(cfg(windows))]
    #[should_fail]
    fn test_dlist_prepend_self() {
        let a = DList::<int>();
        a.prepend(a);
    }
    #[test]
    fn test_dlist_prepend() {
        let a = from_vec(~[1,2,3]);
        let b = from_vec(~[4,5,6]);
        b.prepend(a);
        assert a.len() == 0;
        assert b.len() == 6;
        a.assert_consistent();
        b.assert_consistent(); assert b.pop().get() == 1;
        b.assert_consistent(); assert b.pop().get() == 2;
        b.assert_consistent(); assert b.pop().get() == 3;
        b.assert_consistent(); assert b.pop().get() == 4;
        b.assert_consistent(); assert b.pop().get() == 5;
        b.assert_consistent(); assert b.pop().get() == 6;
        b.assert_consistent(); assert b.is_empty();
    }
    #[test]
    fn test_dlist_reverse() {
        let a = from_vec(~[5,4,3,2,1]);
        a.reverse();
        assert a.len() == 5;
        a.assert_consistent(); assert a.pop().get() == 1;
        a.assert_consistent(); assert a.pop().get() == 2;
        a.assert_consistent(); assert a.pop().get() == 3;
        a.assert_consistent(); assert a.pop().get() == 4;
        a.assert_consistent(); assert a.pop().get() == 5;
        a.assert_consistent(); assert a.is_empty();
    }
    #[test]
    fn test_dlist_reverse_empty() {
        let a = DList::<int>();
        a.reverse();
        assert a.len() == 0;
        a.assert_consistent();
    }
    #[test]
    fn test_dlist_each_node() {
        let a = from_vec(~[1,2,4,5]);
        for a.each_node |nobe| {
            if nobe.data > 3 {
                a.insert_before(3, nobe);
            }
        }
        assert a.len() == 6;
        a.assert_consistent(); assert a.pop().get() == 1;
        a.assert_consistent(); assert a.pop().get() == 2;
        a.assert_consistent(); assert a.pop().get() == 3;
        a.assert_consistent(); assert a.pop().get() == 4;
        a.assert_consistent(); assert a.pop().get() == 3;
        a.assert_consistent(); assert a.pop().get() == 5;
        a.assert_consistent(); assert a.is_empty();
    }
    #[test]
    fn test_dlist_clear() {
        let a = from_vec(~[5,4,3,2,1]);
        a.clear();
        assert a.len() == 0;
        a.assert_consistent();
    }
    #[test]
    fn test_dlist_is_empty() {
        let empty = DList::<int>();
        let full1 = from_vec(~[1,2,3]);
        assert empty.is_empty();
        assert !full1.is_empty();
        assert !empty.is_not_empty();
        assert full1.is_not_empty();
    }
    #[test]
    fn test_dlist_head_tail() {
        let l = from_vec(~[1,2,3]);
        assert l.head() == 1;
        assert l.tail() == 3;
        assert l.len() == 3;
    }
    #[test]
    fn test_dlist_pop() {
        let l = from_vec(~[1,2,3]);
        assert l.pop().get() == 1;
        assert l.tail() == 3;
        assert l.head() == 2;
        assert l.pop().get() == 2;
        assert l.tail() == 3;
        assert l.head() == 3;
        assert l.pop().get() == 3;
        assert l.is_empty();
        assert l.pop().is_none();
    }
    #[test]
    fn test_dlist_pop_tail() {
        let l = from_vec(~[1,2,3]);
        assert l.pop_tail().get() == 3;
        assert l.tail() == 2;
        assert l.head() == 1;
        assert l.pop_tail().get() == 2;
        assert l.tail() == 1;
        assert l.head() == 1;
        assert l.pop_tail().get() == 1;
        assert l.is_empty();
        assert l.pop_tail().is_none();
    }
    #[test]
    fn test_dlist_push() {
        let l = DList::<int>();
        l.push(1);
        assert l.head() == 1;
        assert l.tail() == 1;
        l.push(2);
        assert l.head() == 1;
        assert l.tail() == 2;
        l.push(3);
        assert l.head() == 1;
        assert l.tail() == 3;
        assert l.len() == 3;
    }
    #[test]
    fn test_dlist_push_head() {
        let l = DList::<int>();
        l.push_head(3);
        assert l.head() == 3;
        assert l.tail() == 3;
        l.push_head(2);
        assert l.head() == 2;
        assert l.tail() == 3;
        l.push_head(1);
        assert l.head() == 1;
        assert l.tail() == 3;
        assert l.len() == 3;
    }
    #[test]
    fn test_dlist_foldl() {
        let l = from_vec(vec::from_fn(101, |x|x));
        assert iter::foldl(&l, 0, |accum,elem| *accum+*elem) == 5050;
    }
    #[test]
    fn test_dlist_break_early() {
        let l = from_vec(~[1,2,3,4,5]);
        let mut x = 0;
        for l.each |i| {
            x += 1;
            if (*i == 3) { break; }
        }
        assert x == 3;
    }
    #[test]
    fn test_dlist_remove_head() {
        let l = DList::<int>();
        l.assert_consistent(); let one = l.push_n(1);
        l.assert_consistent(); let _two = l.push_n(2);
        l.assert_consistent(); let _three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(one);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); assert l.head() == 2;
        l.assert_consistent(); assert l.tail() == 3;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_remove_mid() {
        let l = DList::<int>();
        l.assert_consistent(); let _one = l.push_n(1);
        l.assert_consistent(); let two = l.push_n(2);
        l.assert_consistent(); let _three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(two);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); assert l.head() == 1;
        l.assert_consistent(); assert l.tail() == 3;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_remove_tail() {
        let l = DList::<int>();
        l.assert_consistent(); let _one = l.push_n(1);
        l.assert_consistent(); let _two = l.push_n(2);
        l.assert_consistent(); let three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(three);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); assert l.head() == 1;
        l.assert_consistent(); assert l.tail() == 2;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_remove_one_two() {
        let l = DList::<int>();
        l.assert_consistent(); let one = l.push_n(1);
        l.assert_consistent(); let two = l.push_n(2);
        l.assert_consistent(); let _three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(one);
        l.assert_consistent(); l.remove(two);
        // and through and through, the vorpal blade went snicker-snack
        l.assert_consistent(); assert l.len() == 1;
        l.assert_consistent(); assert l.head() == 3;
        l.assert_consistent(); assert l.tail() == 3;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_remove_one_three() {
        let l = DList::<int>();
        l.assert_consistent(); let one = l.push_n(1);
        l.assert_consistent(); let _two = l.push_n(2);
        l.assert_consistent(); let three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(one);
        l.assert_consistent(); l.remove(three);
        l.assert_consistent(); assert l.len() == 1;
        l.assert_consistent(); assert l.head() == 2;
        l.assert_consistent(); assert l.tail() == 2;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_remove_two_three() {
        let l = DList::<int>();
        l.assert_consistent(); let _one = l.push_n(1);
        l.assert_consistent(); let two = l.push_n(2);
        l.assert_consistent(); let three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(two);
        l.assert_consistent(); l.remove(three);
        l.assert_consistent(); assert l.len() == 1;
        l.assert_consistent(); assert l.head() == 1;
        l.assert_consistent(); assert l.tail() == 1;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_remove_all() {
        let l = DList::<int>();
        l.assert_consistent(); let one = l.push_n(1);
        l.assert_consistent(); let two = l.push_n(2);
        l.assert_consistent(); let three = l.push_n(3);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); l.remove(two);
        l.assert_consistent(); l.remove(three);
        l.assert_consistent(); l.remove(one); // Twenty-three is number one!
        l.assert_consistent(); assert l.peek().is_none();
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_insert_n_before() {
        let l = DList::<int>();
        l.assert_consistent(); let _one = l.push_n(1);
        l.assert_consistent(); let two = l.push_n(2);
        l.assert_consistent(); let three = new_dlist_node(3);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); l.insert_n_before(three, two);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); assert l.head() == 1;
        l.assert_consistent(); assert l.tail() == 2;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_insert_n_after() {
        let l = DList::<int>();
        l.assert_consistent(); let one = l.push_n(1);
        l.assert_consistent(); let _two = l.push_n(2);
        l.assert_consistent(); let three = new_dlist_node(3);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); l.insert_n_after(three, one);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); assert l.head() == 1;
        l.assert_consistent(); assert l.tail() == 2;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_insert_before_head() {
        let l = DList::<int>();
        l.assert_consistent(); let one = l.push_n(1);
        l.assert_consistent(); let _two = l.push_n(2);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); l.insert_before(3, one);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); assert l.head() == 3;
        l.assert_consistent(); assert l.tail() == 2;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test]
    fn test_dlist_insert_after_tail() {
        let l = DList::<int>();
        l.assert_consistent(); let _one = l.push_n(1);
        l.assert_consistent(); let two = l.push_n(2);
        l.assert_consistent(); assert l.len() == 2;
        l.assert_consistent(); l.insert_after(3, two);
        l.assert_consistent(); assert l.len() == 3;
        l.assert_consistent(); assert l.head() == 1;
        l.assert_consistent(); assert l.tail() == 3;
        l.assert_consistent(); assert l.pop().get() == 1;
        l.assert_consistent(); assert l.pop().get() == 2;
        l.assert_consistent(); assert l.pop().get() == 3;
        l.assert_consistent(); assert l.is_empty();
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_asymmetric_link() {
        let l = DList::<int>();
        let _one = l.push_n(1);
        let two = l.push_n(2);
        two.prev = None;
        l.assert_consistent();
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_cyclic_list() {
        let l = DList::<int>();
        let one = l.push_n(1);
        let _two = l.push_n(2);
        let three = l.push_n(3);
        three.next = Some(one);
        one.prev = Some(three);
        l.assert_consistent();
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_headless() {
        DList::<int>().head();
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_insert_already_present_before() {
        let l = DList::<int>();
        let one = l.push_n(1);
        let two = l.push_n(2);
        l.insert_n_before(two, one);
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_insert_already_present_after() {
        let l = DList::<int>();
        let one = l.push_n(1);
        let two = l.push_n(2);
        l.insert_n_after(one, two);
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_insert_before_orphan() {
        let l = DList::<int>();
        let one = new_dlist_node(1);
        let two = new_dlist_node(2);
        l.insert_n_before(one, two);
    }
    #[test] #[should_fail] #[ignore(cfg(windows))]
    fn test_dlist_insert_after_orphan() {
        let l = DList::<int>();
        let one = new_dlist_node(1);
        let two = new_dlist_node(2);
        l.insert_n_after(two, one);
    }
}
