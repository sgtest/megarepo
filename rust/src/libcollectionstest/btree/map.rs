// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::BTreeMap;
use std::collections::Bound::{Excluded, Included, Unbounded, self};
use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::iter::range_inclusive;

#[test]
fn test_basic_large() {
    let mut map = BTreeMap::new();
    let size = 10000;
    assert_eq!(map.len(), 0);

    for i in 0..size {
        assert_eq!(map.insert(i, 10*i), None);
        assert_eq!(map.len(), i + 1);
    }

    for i in 0..size {
        assert_eq!(map.get(&i).unwrap(), &(i*10));
    }

    for i in size..size*2 {
        assert_eq!(map.get(&i), None);
    }

    for i in 0..size {
        assert_eq!(map.insert(i, 100*i), Some(10*i));
        assert_eq!(map.len(), size);
    }

    for i in 0..size {
        assert_eq!(map.get(&i).unwrap(), &(i*100));
    }

    for i in 0..size/2 {
        assert_eq!(map.remove(&(i*2)), Some(i*200));
        assert_eq!(map.len(), size - i - 1);
    }

    for i in 0..size/2 {
        assert_eq!(map.get(&(2*i)), None);
        assert_eq!(map.get(&(2*i+1)).unwrap(), &(i*200 + 100));
    }

    for i in 0..size/2 {
        assert_eq!(map.remove(&(2*i)), None);
        assert_eq!(map.remove(&(2*i+1)), Some(i*200 + 100));
        assert_eq!(map.len(), size/2 - i - 1);
    }
}

#[test]
fn test_basic_small() {
    let mut map = BTreeMap::new();
    assert_eq!(map.remove(&1), None);
    assert_eq!(map.get(&1), None);
    assert_eq!(map.insert(1, 1), None);
    assert_eq!(map.get(&1), Some(&1));
    assert_eq!(map.insert(1, 2), Some(1));
    assert_eq!(map.get(&1), Some(&2));
    assert_eq!(map.insert(2, 4), None);
    assert_eq!(map.get(&2), Some(&4));
    assert_eq!(map.remove(&1), Some(2));
    assert_eq!(map.remove(&2), Some(4));
    assert_eq!(map.remove(&1), None);
}

#[test]
fn test_iter() {
    let size = 10000;

    // Forwards
    let mut map: BTreeMap<_, _> = (0..size).map(|i| (i, i)).collect();

    fn test<T>(size: usize, mut iter: T) where T: Iterator<Item=(usize, usize)> {
        for i in 0..size {
            assert_eq!(iter.size_hint(), (size - i, Some(size - i)));
            assert_eq!(iter.next().unwrap(), (i, i));
        }
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(iter.next(), None);
    }
    test(size, map.iter().map(|(&k, &v)| (k, v)));
    test(size, map.iter_mut().map(|(&k, &mut v)| (k, v)));
    test(size, map.into_iter());
}

#[test]
fn test_iter_rev() {
    let size = 10000;

    // Forwards
    let mut map: BTreeMap<_, _> = (0..size).map(|i| (i, i)).collect();

    fn test<T>(size: usize, mut iter: T) where T: Iterator<Item=(usize, usize)> {
        for i in 0..size {
            assert_eq!(iter.size_hint(), (size - i, Some(size - i)));
            assert_eq!(iter.next().unwrap(), (size - i - 1, size - i - 1));
        }
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(iter.next(), None);
    }
    test(size, map.iter().rev().map(|(&k, &v)| (k, v)));
    test(size, map.iter_mut().rev().map(|(&k, &mut v)| (k, v)));
    test(size, map.into_iter().rev());
}

#[test]
fn test_iter_mixed() {
    let size = 10000;

    // Forwards
    let mut map: BTreeMap<_, _> = (0..size).map(|i| (i, i)).collect();

    fn test<T>(size: usize, mut iter: T)
            where T: Iterator<Item=(usize, usize)> + DoubleEndedIterator {
        for i in 0..size / 4 {
            assert_eq!(iter.size_hint(), (size - i * 2, Some(size - i * 2)));
            assert_eq!(iter.next().unwrap(), (i, i));
            assert_eq!(iter.next_back().unwrap(), (size - i - 1, size - i - 1));
        }
        for i in size / 4..size * 3 / 4 {
            assert_eq!(iter.size_hint(), (size * 3 / 4 - i, Some(size * 3 / 4 - i)));
            assert_eq!(iter.next().unwrap(), (i, i));
        }
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(iter.next(), None);
    }
    test(size, map.iter().map(|(&k, &v)| (k, v)));
    test(size, map.iter_mut().map(|(&k, &mut v)| (k, v)));
    test(size, map.into_iter());
}

#[test]
fn test_range_small() {
    let size = 5;

    // Forwards
    let map: BTreeMap<_, _> = (0..size).map(|i| (i, i)).collect();

    let mut j = 0;
    for ((&k, &v), i) in map.range(Included(&2), Unbounded).zip(2..size) {
        assert_eq!(k, i);
        assert_eq!(v, i);
        j += 1;
    }
    assert_eq!(j, size - 2);
}

#[test]
fn test_range_1000() {
    let size = 1000;
    let map: BTreeMap<_, _> = (0..size).map(|i| (i, i)).collect();

    fn test(map: &BTreeMap<u32, u32>, size: u32, min: Bound<&u32>, max: Bound<&u32>) {
        let mut kvs = map.range(min, max).map(|(&k, &v)| (k, v));
        let mut pairs = (0..size).map(|i| (i, i));

        for (kv, pair) in kvs.by_ref().zip(pairs.by_ref()) {
            assert_eq!(kv, pair);
        }
        assert_eq!(kvs.next(), None);
        assert_eq!(pairs.next(), None);
    }
    test(&map, size, Included(&0), Excluded(&size));
    test(&map, size, Unbounded, Excluded(&size));
    test(&map, size, Included(&0), Included(&(size - 1)));
    test(&map, size, Unbounded, Included(&(size - 1)));
    test(&map, size, Included(&0), Unbounded);
    test(&map, size, Unbounded, Unbounded);
}

#[test]
fn test_range() {
    let size = 200;
    let map: BTreeMap<_, _> = (0..size).map(|i| (i, i)).collect();

    for i in 0..size {
        for j in i..size {
            let mut kvs = map.range(Included(&i), Included(&j)).map(|(&k, &v)| (k, v));
            let mut pairs = range_inclusive(i, j).map(|i| (i, i));

            for (kv, pair) in kvs.by_ref().zip(pairs.by_ref()) {
                assert_eq!(kv, pair);
            }
            assert_eq!(kvs.next(), None);
            assert_eq!(pairs.next(), None);
        }
    }
}

#[test]
fn test_entry(){
    let xs = [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (6, 60)];

    let mut map: BTreeMap<_, _> = xs.iter().cloned().collect();

    // Existing key (insert)
    match map.entry(1) {
        Vacant(_) => unreachable!(),
        Occupied(mut view) => {
            assert_eq!(view.get(), &10);
            assert_eq!(view.insert(100), 10);
        }
    }
    assert_eq!(map.get(&1).unwrap(), &100);
    assert_eq!(map.len(), 6);


    // Existing key (update)
    match map.entry(2) {
        Vacant(_) => unreachable!(),
        Occupied(mut view) => {
            let v = view.get_mut();
            *v *= 10;
        }
    }
    assert_eq!(map.get(&2).unwrap(), &200);
    assert_eq!(map.len(), 6);

    // Existing key (take)
    match map.entry(3) {
        Vacant(_) => unreachable!(),
        Occupied(view) => {
            assert_eq!(view.remove(), 30);
        }
    }
    assert_eq!(map.get(&3), None);
    assert_eq!(map.len(), 5);


    // Inexistent key (insert)
    match map.entry(10) {
        Occupied(_) => unreachable!(),
        Vacant(view) => {
            assert_eq!(*view.insert(1000), 1000);
        }
    }
    assert_eq!(map.get(&10).unwrap(), &1000);
    assert_eq!(map.len(), 6);
}

mod bench {
    use std::collections::BTreeMap;
    use std::__rand::{Rng, thread_rng};

    use test::{Bencher, black_box};

    map_insert_rand_bench!{insert_rand_100,    100,    BTreeMap}
    map_insert_rand_bench!{insert_rand_10_000, 10_000, BTreeMap}

    map_insert_seq_bench!{insert_seq_100,    100,    BTreeMap}
    map_insert_seq_bench!{insert_seq_10_000, 10_000, BTreeMap}

    map_find_rand_bench!{find_rand_100,    100,    BTreeMap}
    map_find_rand_bench!{find_rand_10_000, 10_000, BTreeMap}

    map_find_seq_bench!{find_seq_100,    100,    BTreeMap}
    map_find_seq_bench!{find_seq_10_000, 10_000, BTreeMap}

    fn bench_iter(b: &mut Bencher, size: i32) {
        let mut map = BTreeMap::<i32, i32>::new();
        let mut rng = thread_rng();

        for _ in 0..size {
            map.insert(rng.gen(), rng.gen());
        }

        b.iter(|| {
            for entry in &map {
                black_box(entry);
            }
        });
    }

    #[bench]
    pub fn iter_20(b: &mut Bencher) {
        bench_iter(b, 20);
    }

    #[bench]
    pub fn iter_1000(b: &mut Bencher) {
        bench_iter(b, 1000);
    }

    #[bench]
    pub fn iter_100000(b: &mut Bencher) {
        bench_iter(b, 100000);
    }
}
