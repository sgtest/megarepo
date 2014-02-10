// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A standard, garbage-collected linked list.



#[deriving(Clone, Eq)]
#[allow(missing_doc)]
pub enum List<T> {
    Cons(T, @List<T>),
    Nil,
}

/// Create a list from a vector
pub fn from_vec<T:Clone + 'static>(v: &[T]) -> @List<T> {
    v.rev_iter().fold(@Nil::<T>, |t, h| @Cons((*h).clone(), t))
}

/**
 * Left fold
 *
 * Applies `f` to `u` and the first element in the list, then applies `f` to
 * the result of the previous call and the second element, and so on,
 * returning the accumulated result.
 *
 * # Arguments
 *
 * * ls - The list to fold
 * * z - The initial value
 * * f - The function to apply
 */
pub fn foldl<T:Clone,U>(z: T, ls: @List<U>, f: |&T, &U| -> T) -> T {
    let mut accum: T = z;
    iter(ls, |elt| accum = f(&accum, elt));
    accum
}

/**
 * Search for an element that matches a given predicate
 *
 * Apply function `f` to each element of `ls`, starting from the first.
 * When function `f` returns true then an option containing the element
 * is returned. If `f` matches no elements then none is returned.
 */
pub fn find<T:Clone>(ls: @List<T>, f: |&T| -> bool) -> Option<T> {
    let mut ls = ls;
    loop {
        ls = match *ls {
          Cons(ref hd, tl) => {
            if f(hd) { return Some((*hd).clone()); }
            tl
          }
          Nil => return None
        }
    };
}

/**
 * Returns true if a list contains an element that matches a given predicate
 *
 * Apply function `f` to each element of `ls`, starting from the first.
 * When function `f` returns true then it also returns true. If `f` matches no
 * elements then false is returned.
 */
pub fn any<T>(ls: @List<T>, f: |&T| -> bool) -> bool {
    let mut ls = ls;
    loop {
        ls = match *ls {
            Cons(ref hd, tl) => {
                if f(hd) { return true; }
                tl
            }
            Nil => return false
        }
    };
}

/// Returns true if a list contains an element with the given value
pub fn has<T:Eq>(ls: @List<T>, elt: T) -> bool {
    let mut found = false;
    each(ls, |e| {
        if *e == elt { found = true; false } else { true }
    });
    return found;
}

/// Returns true if the list is empty
pub fn is_empty<T>(ls: @List<T>) -> bool {
    match *ls {
        Nil => true,
        _ => false
    }
}

/// Returns the length of a list
pub fn len<T>(ls: @List<T>) -> uint {
    let mut count = 0u;
    iter(ls, |_e| count += 1u);
    count
}

/// Returns all but the first element of a list
pub fn tail<T>(ls: @List<T>) -> @List<T> {
    match *ls {
        Cons(_, tl) => return tl,
        Nil => fail!("list empty")
    }
}

/// Returns the first element of a list
pub fn head<T:Clone>(ls: @List<T>) -> T {
    match *ls {
      Cons(ref hd, _) => (*hd).clone(),
      // makes me sad
      _ => fail!("head invoked on empty list")
    }
}

/// Appends one list to another
pub fn append<T:Clone + 'static>(l: @List<T>, m: @List<T>) -> @List<T> {
    match *l {
      Nil => return m,
      Cons(ref x, xs) => {
        let rest = append(xs, m);
        return @Cons((*x).clone(), rest);
      }
    }
}

/*
/// Push one element into the front of a list, returning a new list
/// THIS VERSION DOESN'T ACTUALLY WORK
fn push<T:Clone>(ll: &mut @list<T>, vv: T) {
    ll = &mut @cons(vv, *ll)
}
*/

/// Iterate over a list
pub fn iter<T>(l: @List<T>, f: |&T|) {
    let mut cur = l;
    loop {
        cur = match *cur {
          Cons(ref hd, tl) => {
            f(hd);
            tl
          }
          Nil => break
        }
    }
}

/// Iterate over a list
pub fn each<T>(l: @List<T>, f: |&T| -> bool) -> bool {
    let mut cur = l;
    loop {
        cur = match *cur {
          Cons(ref hd, tl) => {
            if !f(hd) { return false; }
            tl
          }
          Nil => { return true; }
        }
    }
}

#[cfg(test)]
mod tests {
    use list::{List, Nil, from_vec, head, is_empty, tail};
    use list;

    use std::option;

    #[test]
    fn test_is_empty() {
        let empty : @list::List<int> = from_vec([]);
        let full1 = from_vec([1]);
        let full2 = from_vec(['r', 'u']);

        assert!(is_empty(empty));
        assert!(!is_empty(full1));
        assert!(!is_empty(full2));
    }

    #[test]
    fn test_from_vec() {
        let l = from_vec([0, 1, 2]);

        assert_eq!(head(l), 0);

        let tail_l = tail(l);
        assert_eq!(head(tail_l), 1);

        let tail_tail_l = tail(tail_l);
        assert_eq!(head(tail_tail_l), 2);
    }

    #[test]
    fn test_from_vec_empty() {
        let empty : @list::List<int> = from_vec([]);
        assert_eq!(empty, @list::Nil::<int>);
    }

    #[test]
    fn test_foldl() {
        fn add(a: &uint, b: &int) -> uint { return *a + (*b as uint); }
        let l = from_vec([0, 1, 2, 3, 4]);
        let empty = @list::Nil::<int>;
        assert_eq!(list::foldl(0u, l, add), 10u);
        assert_eq!(list::foldl(0u, empty, add), 0u);
    }

    #[test]
    fn test_foldl2() {
        fn sub(a: &int, b: &int) -> int {
            *a - *b
        }
        let l = from_vec([1, 2, 3, 4]);
        assert_eq!(list::foldl(0, l, sub), -10);
    }

    #[test]
    fn test_find_success() {
        fn match_(i: &int) -> bool { return *i == 2; }
        let l = from_vec([0, 1, 2]);
        assert_eq!(list::find(l, match_), option::Some(2));
    }

    #[test]
    fn test_find_fail() {
        fn match_(_i: &int) -> bool { return false; }
        let l = from_vec([0, 1, 2]);
        let empty = @list::Nil::<int>;
        assert_eq!(list::find(l, match_), option::None::<int>);
        assert_eq!(list::find(empty, match_), option::None::<int>);
    }

    #[test]
    fn test_any() {
        fn match_(i: &int) -> bool { return *i == 2; }
        let l = from_vec([0, 1, 2]);
        let empty = @list::Nil::<int>;
        assert_eq!(list::any(l, match_), true);
        assert_eq!(list::any(empty, match_), false);
    }

    #[test]
    fn test_has() {
        let l = from_vec([5, 8, 6]);
        let empty = @list::Nil::<int>;
        assert!((list::has(l, 5)));
        assert!((!list::has(l, 7)));
        assert!((list::has(l, 8)));
        assert!((!list::has(empty, 5)));
    }

    #[test]
    fn test_len() {
        let l = from_vec([0, 1, 2]);
        let empty = @list::Nil::<int>;
        assert_eq!(list::len(l), 3u);
        assert_eq!(list::len(empty), 0u);
    }

    #[test]
    fn test_append() {
        assert!(from_vec([1,2,3,4])
            == list::append(list::from_vec([1,2]), list::from_vec([3,4])));
    }
}
