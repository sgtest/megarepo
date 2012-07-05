/*!
 * A functional key,value store that works on anything.
 *
 * This works using a binary search tree. In the first version, it's a
 * very naive algorithm, but it will probably be updated to be a
 * red-black tree or something else.
 *
 * This is copied and modified from treemap right now. It's missing a lot
 * of features.
 */

import option::{some, none};
import option = option;

export treemap;
export init;
export insert;
export find;
export traverse;

type treemap<K, V> = @tree_node<K, V>;

enum tree_node<K, V> {
    empty,
    node(@K, @V, @tree_node<K, V>, @tree_node<K, V>)
}

/// Create a treemap
fn init<K, V>() -> treemap<K, V> { @empty }

/// Insert a value into the map
fn insert<K: copy, V: copy>(m: treemap<K, V>, k: K, v: V) -> treemap<K, V> {
    @alt m {
       @empty { node(@k, @v, @empty, @empty) }
       @node(@kk, vv, left, right) {
         if k < kk {
             node(@kk, vv, insert(left, k, v), right)
         } else if k == kk {
             node(@kk, @v, left, right)
         } else { node(@kk, vv, left, insert(right, k, v)) }
       }
     }
}

/// Find a value based on the key
fn find<K, V: copy>(m: treemap<K, V>, k: K) -> option<V> {
    alt *m {
      empty { none }
      node(@kk, @v, left, right) {
        if k == kk {
            some(v)
        } else if k < kk { find(left, k) } else { find(right, k) }
      }
    }
}

/// Visit all pairs in the map in order.
fn traverse<K, V: copy>(m: treemap<K, V>, f: fn(K, V)) {
    alt *m {
      empty { }
      /*
        Previously, this had what looked like redundant
        matches to me, so I changed it. but that may be a
        de-optimization -- tjc
       */
      node(@k, @v, left, right) {
        // copy v to make aliases work out
        let v1 = v;
        traverse(left, f);
        f(k, v1);
        traverse(right, f);
      }
    }
}
