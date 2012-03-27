#[doc = "
A key,value store that works on anything.

This works using a binary search tree. In the first version, it's a
very naive algorithm, but it will probably be updated to be a
red-black tree or something else.
"];

import core::option::{some, none};
import option = core::option;

export treemap;
export treemap;
export insert;
export find;
export traverse;

type treemap<K, V> = @mut tree_node<K, V>;

enum tree_node<K, V> { empty, node(@K, @V, treemap<K, V>, treemap<K, V>) }

#[doc = "Create a treemap"]
fn treemap<K, V>() -> treemap<K, V> { @mut empty }

#[doc = "Insert a value into the map"]
fn insert<K: copy, V: copy>(m: treemap<K, V>, k: K, v: V) {
    alt m {
      @empty { *m = node(@k, @v, @mut empty, @mut empty); }
      @node(@kk, _, _, _) {

        // We have to name left and right individually, because
        // otherwise the alias checker complains.
        if k < kk {
            alt check m { @node(_, _, left, _) { insert(left, k, v); } }
        } else {
            alt check m {
              @node(_, _, _, right) { insert(right, k, v); }
            }
        }
      }
    }
}

#[doc = "Find a value based on the key"]
fn find<K: copy, V: copy>(m: treemap<K, V>, k: K) -> option<V> {
    alt *m {
      empty { none }
      // TODO: was that an optimization?
      node(@kk, @v, left, right) {
        if k == kk {
            some(v)
        } else if k < kk {
            find(left, k)
        } else { find(right, k) }
      }
    }
}

#[doc = "Visit all pairs in the map in order."]
fn traverse<K, V>(m: treemap<K, V>, f: fn(K, V)) {
    alt *m {
      empty { }
      /*
        Previously, this had what looked like redundant
        matches to me, so I changed it. but that may be a
        de-optimization -- tjc
       */
      node(k, v, left, right) {
        let k1 = k, v1 = v;
        traverse(left, f);
        f(*k1, *v1);
        traverse(right, f);
      }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn init_treemap() { let _m = treemap::<int, int>(); }

    #[test]
    fn insert_one() { let m = treemap(); insert(m, 1, 2); }

    #[test]
    fn insert_two() { let m = treemap(); insert(m, 1, 2); insert(m, 3, 4); }

    #[test]
    fn insert_find() {
        let m = treemap();
        insert(m, 1, 2);
        assert (find(m, 1) == some(2));
    }

    #[test]
    fn find_empty() {
        let m = treemap::<int, int>(); assert (find(m, 1) == none);
    }

    #[test]
    fn find_not_found() {
        let m = treemap();
        insert(m, 1, 2);
        assert (find(m, 2) == none);
    }

    #[test]
    fn traverse_in_order() {
        let m = treemap();
        insert(m, 3, ());
        insert(m, 0, ());
        insert(m, 4, ());
        insert(m, 2, ());
        insert(m, 1, ());

        let n = @mut 0;
        fn t(n: @mut int, &&k: int, &&_v: ()) {
            assert (*n == k); *n += 1;
        }
        traverse(m, bind t(n, _, _));
    }

    #[test]
    fn u8_map() {
        let m = treemap();

        let k1 = str::bytes("foo");
        let k2 = str::bytes("bar");

        insert(m, k1, "foo");
        insert(m, k2, "bar");

        assert (find(m, k2) == some("bar"));
        assert (find(m, k1) == some("foo"));
    }
}