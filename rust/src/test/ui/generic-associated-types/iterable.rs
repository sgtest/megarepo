// FIXME: missing sysroot spans (#53081)
// ignore-i586-unknown-linux-gnu
// ignore-i586-unknown-linux-musl
// ignore-i686-unknown-linux-musl

#![allow(incomplete_features)]
#![feature(generic_associated_types)]

// FIXME(#30472) normalize enough to handle this.

trait Iterable {
    type Item<'a> where Self: 'a;
    type Iter<'a>: Iterator<Item = Self::Item<'a>> where Self: 'a;

    fn iter<'a>(&'a self) -> Self::Iter<'a>;
}

// Impl for struct type
impl<T> Iterable for Vec<T> {
    type Item<'a> where T: 'a = <std::slice::Iter<'a, T> as Iterator>::Item;
    //~^ ERROR type mismatch resolving
    type Iter<'a> where T: 'a = std::slice::Iter<'a, T>;

    fn iter<'a>(&'a self) -> Self::Iter<'a> {
    //~^ ERROR type mismatch resolving
        self.iter()
    }
}

// Impl for a primitive type
impl<T> Iterable for [T] {
    type Item<'a> where T: 'a = <std::slice::Iter<'a, T> as Iterator>::Item;
    //~^ ERROR type mismatch resolving
    type Iter<'a> where T: 'a = std::slice::Iter<'a, T>;

    fn iter<'a>(&'a self) -> Self::Iter<'a> {
    //~^ ERROR type mismatch resolving
        self.iter()
    }
}

fn make_iter<'a, I: Iterable>(it: &'a I) -> I::Iter<'a> {
    it.iter()
}

fn get_first<'a, I: Iterable>(it: &'a I) -> Option<I::Item<'a>> {
    it.iter().next()
}

fn main() {
    let v = vec![1, 2, 3];
    assert_eq!(v, make_iter(&v).copied().collect());
    assert_eq!(v, make_iter(&*v).copied().collect());
    assert_eq!(1, get_first(&v));
    assert_eq!(1, get_first(&*v));
}
