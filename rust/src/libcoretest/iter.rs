// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::iter::*;
use core::iter::order::*;
use core::iter::MinMaxResult::*;
use core::usize;
use core::cmp;

use test::Bencher;

#[test]
fn test_lt() {
    let empty: [isize; 0] = [];
    let xs = [1,2,3];
    let ys = [1,2,0];

    assert!(!lt(xs.iter(), ys.iter()));
    assert!(!le(xs.iter(), ys.iter()));
    assert!( gt(xs.iter(), ys.iter()));
    assert!( ge(xs.iter(), ys.iter()));

    assert!( lt(ys.iter(), xs.iter()));
    assert!( le(ys.iter(), xs.iter()));
    assert!(!gt(ys.iter(), xs.iter()));
    assert!(!ge(ys.iter(), xs.iter()));

    assert!( lt(empty.iter(), xs.iter()));
    assert!( le(empty.iter(), xs.iter()));
    assert!(!gt(empty.iter(), xs.iter()));
    assert!(!ge(empty.iter(), xs.iter()));

    // Sequence with NaN
    let u = [1.0f64, 2.0];
    let v = [0.0f64/0.0, 3.0];

    assert!(!lt(u.iter(), v.iter()));
    assert!(!le(u.iter(), v.iter()));
    assert!(!gt(u.iter(), v.iter()));
    assert!(!ge(u.iter(), v.iter()));

    let a = [0.0f64/0.0];
    let b = [1.0f64];
    let c = [2.0f64];

    assert!(lt(a.iter(), b.iter()) == (a[0] <  b[0]));
    assert!(le(a.iter(), b.iter()) == (a[0] <= b[0]));
    assert!(gt(a.iter(), b.iter()) == (a[0] >  b[0]));
    assert!(ge(a.iter(), b.iter()) == (a[0] >= b[0]));

    assert!(lt(c.iter(), b.iter()) == (c[0] <  b[0]));
    assert!(le(c.iter(), b.iter()) == (c[0] <= b[0]));
    assert!(gt(c.iter(), b.iter()) == (c[0] >  b[0]));
    assert!(ge(c.iter(), b.iter()) == (c[0] >= b[0]));
}

#[test]
fn test_multi_iter() {
    let xs = [1,2,3,4];
    let ys = [4,3,2,1];
    assert!(eq(xs.iter(), ys.iter().rev()));
    assert!(lt(xs.iter(), xs.iter().skip(2)));
}

#[test]
fn test_counter_from_iter() {
    let it = (0..).step_by(5).take(10);
    let xs: Vec<isize> = FromIterator::from_iter(it);
    assert_eq!(xs, [0, 5, 10, 15, 20, 25, 30, 35, 40, 45]);
}

#[test]
fn test_iterator_chain() {
    let xs = [0, 1, 2, 3, 4, 5];
    let ys = [30, 40, 50, 60];
    let expected = [0, 1, 2, 3, 4, 5, 30, 40, 50, 60];
    let it = xs.iter().chain(ys.iter());
    let mut i = 0;
    for &x in it {
        assert_eq!(x, expected[i]);
        i += 1;
    }
    assert_eq!(i, expected.len());

    let ys = (30..).step_by(10).take(4);
    let it = xs.iter().cloned().chain(ys);
    let mut i = 0;
    for x in it {
        assert_eq!(x, expected[i]);
        i += 1;
    }
    assert_eq!(i, expected.len());
}

#[test]
fn test_filter_map() {
    let it = (0..).step_by(1).take(10)
        .filter_map(|x| if x % 2 == 0 { Some(x*x) } else { None });
    assert_eq!(it.collect::<Vec<usize>>(), [0*0, 2*2, 4*4, 6*6, 8*8]);
}

#[test]
fn test_iterator_enumerate() {
    let xs = [0, 1, 2, 3, 4, 5];
    let it = xs.iter().enumerate();
    for (i, &x) in it {
        assert_eq!(i, x);
    }
}

#[test]
fn test_iterator_peekable() {
    let xs = vec![0, 1, 2, 3, 4, 5];
    let mut it = xs.iter().cloned().peekable();

    assert_eq!(it.len(), 6);
    assert_eq!(it.peek().unwrap(), &0);
    assert_eq!(it.len(), 6);
    assert_eq!(it.next().unwrap(), 0);
    assert_eq!(it.len(), 5);
    assert_eq!(it.next().unwrap(), 1);
    assert_eq!(it.len(), 4);
    assert_eq!(it.next().unwrap(), 2);
    assert_eq!(it.len(), 3);
    assert_eq!(it.peek().unwrap(), &3);
    assert_eq!(it.len(), 3);
    assert_eq!(it.peek().unwrap(), &3);
    assert_eq!(it.len(), 3);
    assert_eq!(it.next().unwrap(), 3);
    assert_eq!(it.len(), 2);
    assert_eq!(it.next().unwrap(), 4);
    assert_eq!(it.len(), 1);
    assert_eq!(it.peek().unwrap(), &5);
    assert_eq!(it.len(), 1);
    assert_eq!(it.next().unwrap(), 5);
    assert_eq!(it.len(), 0);
    assert!(it.peek().is_none());
    assert_eq!(it.len(), 0);
    assert!(it.next().is_none());
    assert_eq!(it.len(), 0);
}

#[test]
fn test_iterator_take_while() {
    let xs = [0, 1, 2, 3, 5, 13, 15, 16, 17, 19];
    let ys = [0, 1, 2, 3, 5, 13];
    let it = xs.iter().take_while(|&x| *x < 15);
    let mut i = 0;
    for x in it {
        assert_eq!(*x, ys[i]);
        i += 1;
    }
    assert_eq!(i, ys.len());
}

#[test]
fn test_iterator_skip_while() {
    let xs = [0, 1, 2, 3, 5, 13, 15, 16, 17, 19];
    let ys = [15, 16, 17, 19];
    let it = xs.iter().skip_while(|&x| *x < 15);
    let mut i = 0;
    for x in it {
        assert_eq!(*x, ys[i]);
        i += 1;
    }
    assert_eq!(i, ys.len());
}

#[test]
fn test_iterator_skip() {
    let xs = [0, 1, 2, 3, 5, 13, 15, 16, 17, 19, 20, 30];
    let ys = [13, 15, 16, 17, 19, 20, 30];
    let mut it = xs.iter().skip(5);
    let mut i = 0;
    while let Some(&x) = it.next() {
        assert_eq!(x, ys[i]);
        i += 1;
        assert_eq!(it.len(), xs.len()-5-i);
    }
    assert_eq!(i, ys.len());
    assert_eq!(it.len(), 0);
}

#[test]
fn test_iterator_take() {
    let xs = [0, 1, 2, 3, 5, 13, 15, 16, 17, 19];
    let ys = [0, 1, 2, 3, 5];
    let mut it = xs.iter().take(5);
    let mut i = 0;
    assert_eq!(it.len(), 5);
    while let Some(&x) = it.next() {
        assert_eq!(x, ys[i]);
        i += 1;
        assert_eq!(it.len(), 5-i);
    }
    assert_eq!(i, ys.len());
    assert_eq!(it.len(), 0);
}

#[test]
fn test_iterator_take_short() {
    let xs = [0, 1, 2, 3];
    let ys = [0, 1, 2, 3];
    let mut it = xs.iter().take(5);
    let mut i = 0;
    assert_eq!(it.len(), 4);
    while let Some(&x) = it.next() {
        assert_eq!(x, ys[i]);
        i += 1;
        assert_eq!(it.len(), 4-i);
    }
    assert_eq!(i, ys.len());
    assert_eq!(it.len(), 0);
}

#[test]
fn test_iterator_scan() {
    // test the type inference
    fn add(old: &mut isize, new: &usize) -> Option<f64> {
        *old += *new as isize;
        Some(*old as f64)
    }
    let xs = [0, 1, 2, 3, 4];
    let ys = [0f64, 1.0, 3.0, 6.0, 10.0];

    let it = xs.iter().scan(0, add);
    let mut i = 0;
    for x in it {
        assert_eq!(x, ys[i]);
        i += 1;
    }
    assert_eq!(i, ys.len());
}

#[test]
fn test_iterator_flat_map() {
    let xs = [0, 3, 6];
    let ys = [0, 1, 2, 3, 4, 5, 6, 7, 8];
    let it = xs.iter().flat_map(|&x| (x..).step_by(1).take(3));
    let mut i = 0;
    for x in it {
        assert_eq!(x, ys[i]);
        i += 1;
    }
    assert_eq!(i, ys.len());
}

#[test]
fn test_inspect() {
    let xs = [1, 2, 3, 4];
    let mut n = 0;

    let ys = xs.iter()
               .cloned()
               .inspect(|_| n += 1)
               .collect::<Vec<usize>>();

    assert_eq!(n, xs.len());
    assert_eq!(&xs[..], &ys[..]);
}

#[test]
fn test_unfoldr() {
    fn count(st: &mut usize) -> Option<usize> {
        if *st < 10 {
            let ret = Some(*st);
            *st += 1;
            ret
        } else {
            None
        }
    }

    let it = Unfold::new(0, count);
    let mut i = 0;
    for counted in it {
        assert_eq!(counted, i);
        i += 1;
    }
    assert_eq!(i, 10);
}

#[test]
fn test_cycle() {
    let cycle_len = 3;
    let it = (0..).step_by(1).take(cycle_len).cycle();
    assert_eq!(it.size_hint(), (usize::MAX, None));
    for (i, x) in it.take(100).enumerate() {
        assert_eq!(i % cycle_len, x);
    }

    let mut it = (0..).step_by(1).take(0).cycle();
    assert_eq!(it.size_hint(), (0, Some(0)));
    assert_eq!(it.next(), None);
}

#[test]
fn test_iterator_nth() {
    let v: &[_] = &[0, 1, 2, 3, 4];
    for i in 0..v.len() {
        assert_eq!(v.iter().nth(i).unwrap(), &v[i]);
    }
    assert_eq!(v.iter().nth(v.len()), None);
}

#[test]
fn test_iterator_last() {
    let v: &[_] = &[0, 1, 2, 3, 4];
    assert_eq!(v.iter().last().unwrap(), &4);
    assert_eq!(v[..1].iter().last().unwrap(), &0);
}

#[test]
fn test_iterator_len() {
    let v: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(v[..4].iter().count(), 4);
    assert_eq!(v[..10].iter().count(), 10);
    assert_eq!(v[..0].iter().count(), 0);
}

#[test]
fn test_iterator_sum() {
    let v: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(v[..4].iter().cloned().sum::<i32>(), 6);
    assert_eq!(v.iter().cloned().sum::<i32>(), 55);
    assert_eq!(v[..0].iter().cloned().sum::<i32>(), 0);
}

#[test]
fn test_iterator_product() {
    let v: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(v[..4].iter().cloned().product::<i32>(), 0);
    assert_eq!(v[1..5].iter().cloned().product::<i32>(), 24);
    assert_eq!(v[..0].iter().cloned().product::<i32>(), 1);
}

#[test]
fn test_iterator_max() {
    let v: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(v[..4].iter().cloned().max(), Some(3));
    assert_eq!(v.iter().cloned().max(), Some(10));
    assert_eq!(v[..0].iter().cloned().max(), None);
}

#[test]
fn test_iterator_min() {
    let v: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(v[..4].iter().cloned().min(), Some(0));
    assert_eq!(v.iter().cloned().min(), Some(0));
    assert_eq!(v[..0].iter().cloned().min(), None);
}

#[test]
fn test_iterator_size_hint() {
    let c = (0..).step_by(1);
    let v: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let v2 = &[10, 11, 12];
    let vi = v.iter();

    assert_eq!(c.size_hint(), (usize::MAX, None));
    assert_eq!(vi.clone().size_hint(), (10, Some(10)));

    assert_eq!(c.clone().take(5).size_hint(), (5, Some(5)));
    assert_eq!(c.clone().skip(5).size_hint().1, None);
    assert_eq!(c.clone().take_while(|_| false).size_hint(), (0, None));
    assert_eq!(c.clone().skip_while(|_| false).size_hint(), (0, None));
    assert_eq!(c.clone().enumerate().size_hint(), (usize::MAX, None));
    assert_eq!(c.clone().chain(vi.clone().cloned()).size_hint(), (usize::MAX, None));
    assert_eq!(c.clone().zip(vi.clone()).size_hint(), (10, Some(10)));
    assert_eq!(c.clone().scan(0, |_,_| Some(0)).size_hint(), (0, None));
    assert_eq!(c.clone().filter(|_| false).size_hint(), (0, None));
    assert_eq!(c.clone().map(|_| 0).size_hint(), (usize::MAX, None));
    assert_eq!(c.filter_map(|_| Some(0)).size_hint(), (0, None));

    assert_eq!(vi.clone().take(5).size_hint(), (5, Some(5)));
    assert_eq!(vi.clone().take(12).size_hint(), (10, Some(10)));
    assert_eq!(vi.clone().skip(3).size_hint(), (7, Some(7)));
    assert_eq!(vi.clone().skip(12).size_hint(), (0, Some(0)));
    assert_eq!(vi.clone().take_while(|_| false).size_hint(), (0, Some(10)));
    assert_eq!(vi.clone().skip_while(|_| false).size_hint(), (0, Some(10)));
    assert_eq!(vi.clone().enumerate().size_hint(), (10, Some(10)));
    assert_eq!(vi.clone().chain(v2.iter()).size_hint(), (13, Some(13)));
    assert_eq!(vi.clone().zip(v2.iter()).size_hint(), (3, Some(3)));
    assert_eq!(vi.clone().scan(0, |_,_| Some(0)).size_hint(), (0, Some(10)));
    assert_eq!(vi.clone().filter(|_| false).size_hint(), (0, Some(10)));
    assert_eq!(vi.clone().map(|&i| i+1).size_hint(), (10, Some(10)));
    assert_eq!(vi.filter_map(|_| Some(0)).size_hint(), (0, Some(10)));
}

#[test]
fn test_collect() {
    let a = vec![1, 2, 3, 4, 5];
    let b: Vec<isize> = a.iter().cloned().collect();
    assert!(a == b);
}

#[test]
fn test_all() {
    // FIXME (#22405): Replace `Box::new` with `box` here when/if possible.
    let v: Box<[isize]> = Box::new([1, 2, 3, 4, 5]);
    assert!(v.iter().all(|&x| x < 10));
    assert!(!v.iter().all(|&x| x % 2 == 0));
    assert!(!v.iter().all(|&x| x > 100));
    assert!(v[..0].iter().all(|_| panic!()));
}

#[test]
fn test_any() {
    // FIXME (#22405): Replace `Box::new` with `box` here when/if possible.
    let v: Box<[isize]> = Box::new([1, 2, 3, 4, 5]);
    assert!(v.iter().any(|&x| x < 10));
    assert!(v.iter().any(|&x| x % 2 == 0));
    assert!(!v.iter().any(|&x| x > 100));
    assert!(!v[..0].iter().any(|_| panic!()));
}

#[test]
fn test_find() {
    let v: &[isize] = &[1, 3, 9, 27, 103, 14, 11];
    assert_eq!(*v.iter().find(|&&x| x & 1 == 0).unwrap(), 14);
    assert_eq!(*v.iter().find(|&&x| x % 3 == 0).unwrap(), 3);
    assert!(v.iter().find(|&&x| x % 12 == 0).is_none());
}

#[test]
fn test_position() {
    let v = &[1, 3, 9, 27, 103, 14, 11];
    assert_eq!(v.iter().position(|x| *x & 1 == 0).unwrap(), 5);
    assert_eq!(v.iter().position(|x| *x % 3 == 0).unwrap(), 1);
    assert!(v.iter().position(|x| *x % 12 == 0).is_none());
}

#[test]
fn test_count() {
    let xs = &[1, 2, 2, 1, 5, 9, 0, 2];
    assert_eq!(xs.iter().filter(|x| **x == 2).count(), 3);
    assert_eq!(xs.iter().filter(|x| **x == 5).count(), 1);
    assert_eq!(xs.iter().filter(|x| **x == 95).count(), 0);
}

#[test]
fn test_max_by() {
    let xs: &[isize] = &[-3, 0, 1, 5, -10];
    assert_eq!(*xs.iter().max_by(|x| x.abs()).unwrap(), -10);
}

#[test]
fn test_min_by() {
    let xs: &[isize] = &[-3, 0, 1, 5, -10];
    assert_eq!(*xs.iter().min_by(|x| x.abs()).unwrap(), 0);
}

#[test]
fn test_by_ref() {
    let mut xs = 0..10;
    // sum the first five values
    let partial_sum = xs.by_ref().take(5).fold(0, |a, b| a + b);
    assert_eq!(partial_sum, 10);
    assert_eq!(xs.next(), Some(5));
}

#[test]
fn test_rev() {
    let xs = [2, 4, 6, 8, 10, 12, 14, 16];
    let mut it = xs.iter();
    it.next();
    it.next();
    assert!(it.rev().cloned().collect::<Vec<isize>>() ==
            vec![16, 14, 12, 10, 8, 6]);
}

#[test]
fn test_cloned() {
    let xs = [2u8, 4, 6, 8];

    let mut it = xs.iter().cloned();
    assert_eq!(it.len(), 4);
    assert_eq!(it.next(), Some(2));
    assert_eq!(it.len(), 3);
    assert_eq!(it.next(), Some(4));
    assert_eq!(it.len(), 2);
    assert_eq!(it.next_back(), Some(8));
    assert_eq!(it.len(), 1);
    assert_eq!(it.next_back(), Some(6));
    assert_eq!(it.len(), 0);
    assert_eq!(it.next_back(), None);
}

#[test]
fn test_double_ended_map() {
    let xs = [1, 2, 3, 4, 5, 6];
    let mut it = xs.iter().map(|&x| x * -1);
    assert_eq!(it.next(), Some(-1));
    assert_eq!(it.next(), Some(-2));
    assert_eq!(it.next_back(), Some(-6));
    assert_eq!(it.next_back(), Some(-5));
    assert_eq!(it.next(), Some(-3));
    assert_eq!(it.next_back(), Some(-4));
    assert_eq!(it.next(), None);
}

#[test]
fn test_double_ended_enumerate() {
    let xs = [1, 2, 3, 4, 5, 6];
    let mut it = xs.iter().cloned().enumerate();
    assert_eq!(it.next(), Some((0, 1)));
    assert_eq!(it.next(), Some((1, 2)));
    assert_eq!(it.next_back(), Some((5, 6)));
    assert_eq!(it.next_back(), Some((4, 5)));
    assert_eq!(it.next_back(), Some((3, 4)));
    assert_eq!(it.next_back(), Some((2, 3)));
    assert_eq!(it.next(), None);
}

#[test]
fn test_double_ended_zip() {
    let xs = [1, 2, 3, 4, 5, 6];
    let ys = [1, 2, 3, 7];
    let a = xs.iter().cloned();
    let b = ys.iter().cloned();
    let mut it = a.zip(b);
    assert_eq!(it.next(), Some((1, 1)));
    assert_eq!(it.next(), Some((2, 2)));
    assert_eq!(it.next_back(), Some((4, 7)));
    assert_eq!(it.next_back(), Some((3, 3)));
    assert_eq!(it.next(), None);
}

#[test]
fn test_double_ended_filter() {
    let xs = [1, 2, 3, 4, 5, 6];
    let mut it = xs.iter().filter(|&x| *x & 1 == 0);
    assert_eq!(it.next_back().unwrap(), &6);
    assert_eq!(it.next_back().unwrap(), &4);
    assert_eq!(it.next().unwrap(), &2);
    assert_eq!(it.next_back(), None);
}

#[test]
fn test_double_ended_filter_map() {
    let xs = [1, 2, 3, 4, 5, 6];
    let mut it = xs.iter().filter_map(|&x| if x & 1 == 0 { Some(x * 2) } else { None });
    assert_eq!(it.next_back().unwrap(), 12);
    assert_eq!(it.next_back().unwrap(), 8);
    assert_eq!(it.next().unwrap(), 4);
    assert_eq!(it.next_back(), None);
}

#[test]
fn test_double_ended_chain() {
    let xs = [1, 2, 3, 4, 5];
    let ys = [7, 9, 11];
    let mut it = xs.iter().chain(ys.iter()).rev();
    assert_eq!(it.next().unwrap(), &11);
    assert_eq!(it.next().unwrap(), &9);
    assert_eq!(it.next_back().unwrap(), &1);
    assert_eq!(it.next_back().unwrap(), &2);
    assert_eq!(it.next_back().unwrap(), &3);
    assert_eq!(it.next_back().unwrap(), &4);
    assert_eq!(it.next_back().unwrap(), &5);
    assert_eq!(it.next_back().unwrap(), &7);
    assert_eq!(it.next_back(), None);
}

#[test]
fn test_rposition() {
    fn f(xy: &(isize, char)) -> bool { let (_x, y) = *xy; y == 'b' }
    fn g(xy: &(isize, char)) -> bool { let (_x, y) = *xy; y == 'd' }
    let v = [(0, 'a'), (1, 'b'), (2, 'c'), (3, 'b')];

    assert_eq!(v.iter().rposition(f), Some(3));
    assert!(v.iter().rposition(g).is_none());
}

#[test]
#[should_panic]
fn test_rposition_panic() {
    let v: [(Box<_>, Box<_>); 4] =
        [(box 0, box 0), (box 0, box 0),
         (box 0, box 0), (box 0, box 0)];
    let mut i = 0;
    v.iter().rposition(|_elt| {
        if i == 2 {
            panic!()
        }
        i += 1;
        false
    });
}


#[cfg(test)]
fn check_randacc_iter<A, T>(a: T, len: usize) where
    A: PartialEq,
    T: Clone + RandomAccessIterator + Iterator<Item=A>,
{
    let mut b = a.clone();
    assert_eq!(len, b.indexable());
    let mut n = 0;
    for (i, elt) in a.enumerate() {
        assert!(Some(elt) == b.idx(i));
        n += 1;
    }
    assert_eq!(n, len);
    assert!(None == b.idx(n));
    // call recursively to check after picking off an element
    if len > 0 {
        b.next();
        check_randacc_iter(b, len-1);
    }
}


#[test]
fn test_double_ended_flat_map() {
    let u = [0,1];
    let v = [5,6,7,8];
    let mut it = u.iter().flat_map(|x| v[*x..v.len()].iter());
    assert_eq!(it.next_back().unwrap(), &8);
    assert_eq!(it.next().unwrap(),      &5);
    assert_eq!(it.next_back().unwrap(), &7);
    assert_eq!(it.next_back().unwrap(), &6);
    assert_eq!(it.next_back().unwrap(), &8);
    assert_eq!(it.next().unwrap(),      &6);
    assert_eq!(it.next_back().unwrap(), &7);
    assert_eq!(it.next_back(), None);
    assert_eq!(it.next(),      None);
    assert_eq!(it.next_back(), None);
}

#[test]
fn test_random_access_chain() {
    let xs = [1, 2, 3, 4, 5];
    let ys = [7, 9, 11];
    let mut it = xs.iter().chain(ys.iter());
    assert_eq!(it.idx(0).unwrap(), &1);
    assert_eq!(it.idx(5).unwrap(), &7);
    assert_eq!(it.idx(7).unwrap(), &11);
    assert!(it.idx(8).is_none());

    it.next();
    it.next();
    it.next_back();

    assert_eq!(it.idx(0).unwrap(), &3);
    assert_eq!(it.idx(4).unwrap(), &9);
    assert!(it.idx(6).is_none());

    check_randacc_iter(it, xs.len() + ys.len() - 3);
}

#[test]
fn test_random_access_enumerate() {
    let xs = [1, 2, 3, 4, 5];
    check_randacc_iter(xs.iter().enumerate(), xs.len());
}

#[test]
fn test_random_access_rev() {
    let xs = [1, 2, 3, 4, 5];
    check_randacc_iter(xs.iter().rev(), xs.len());
    let mut it = xs.iter().rev();
    it.next();
    it.next_back();
    it.next();
    check_randacc_iter(it, xs.len() - 3);
}

#[test]
fn test_random_access_zip() {
    let xs = [1, 2, 3, 4, 5];
    let ys = [7, 9, 11];
    check_randacc_iter(xs.iter().zip(ys.iter()), cmp::min(xs.len(), ys.len()));
}

#[test]
fn test_random_access_take() {
    let xs = [1, 2, 3, 4, 5];
    let empty: &[isize] = &[];
    check_randacc_iter(xs.iter().take(3), 3);
    check_randacc_iter(xs.iter().take(20), xs.len());
    check_randacc_iter(xs.iter().take(0), 0);
    check_randacc_iter(empty.iter().take(2), 0);
}

#[test]
fn test_random_access_skip() {
    let xs = [1, 2, 3, 4, 5];
    let empty: &[isize] = &[];
    check_randacc_iter(xs.iter().skip(2), xs.len() - 2);
    check_randacc_iter(empty.iter().skip(2), 0);
}

#[test]
fn test_random_access_inspect() {
    let xs = [1, 2, 3, 4, 5];

    // test .map and .inspect that don't implement Clone
    let mut it = xs.iter().inspect(|_| {});
    assert_eq!(xs.len(), it.indexable());
    for (i, elt) in xs.iter().enumerate() {
        assert_eq!(Some(elt), it.idx(i));
    }

}

#[test]
fn test_random_access_map() {
    let xs = [1, 2, 3, 4, 5];

    let mut it = xs.iter().cloned();
    assert_eq!(xs.len(), it.indexable());
    for (i, elt) in xs.iter().enumerate() {
        assert_eq!(Some(*elt), it.idx(i));
    }
}

#[test]
fn test_random_access_cycle() {
    let xs = [1, 2, 3, 4, 5];
    let empty: &[isize] = &[];
    check_randacc_iter(xs.iter().cycle().take(27), 27);
    check_randacc_iter(empty.iter().cycle(), 0);
}

#[test]
fn test_double_ended_range() {
    assert_eq!((11..14).rev().collect::<Vec<_>>(), [13, 12, 11]);
    for _ in (10..0).rev() {
        panic!("unreachable");
    }

    assert_eq!((11..14).rev().collect::<Vec<_>>(), [13, 12, 11]);
    for _ in (10..0).rev() {
        panic!("unreachable");
    }
}

#[test]
fn test_range() {
    assert_eq!((0..5).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
    assert_eq!((-10..-1).collect::<Vec<_>>(), [-10, -9, -8, -7, -6, -5, -4, -3, -2]);
    assert_eq!((0..5).rev().collect::<Vec<_>>(), [4, 3, 2, 1, 0]);
    assert_eq!((200..-5).count(), 0);
    assert_eq!((200..-5).rev().count(), 0);
    assert_eq!((200..200).count(), 0);
    assert_eq!((200..200).rev().count(), 0);

    assert_eq!((0..100).size_hint(), (100, Some(100)));
    // this test is only meaningful when sizeof usize < sizeof u64
    assert_eq!((usize::MAX - 1..usize::MAX).size_hint(), (1, Some(1)));
    assert_eq!((-10..-1).size_hint(), (9, Some(9)));
    assert_eq!((-1..-10).size_hint(), (0, Some(0)));
}

#[test]
fn test_range_inclusive() {
    assert!(range_inclusive(0, 5).collect::<Vec<isize>>() ==
            vec![0, 1, 2, 3, 4, 5]);
    assert!(range_inclusive(0, 5).rev().collect::<Vec<isize>>() ==
            vec![5, 4, 3, 2, 1, 0]);
    assert_eq!(range_inclusive(200, -5).count(), 0);
    assert_eq!(range_inclusive(200, -5).rev().count(), 0);
    assert_eq!(range_inclusive(200, 200).collect::<Vec<isize>>(), [200]);
    assert_eq!(range_inclusive(200, 200).rev().collect::<Vec<isize>>(), [200]);
}

#[test]
fn test_range_step() {
    assert_eq!((0..20).step_by(5).collect::<Vec<isize>>(), [0, 5, 10, 15]);
    assert_eq!((20..0).step_by(-5).collect::<Vec<isize>>(), [20, 15, 10, 5]);
    assert_eq!((20..0).step_by(-6).collect::<Vec<isize>>(), [20, 14, 8, 2]);
    assert_eq!((200..255).step_by(50).collect::<Vec<u8>>(), [200, 250]);
    assert_eq!((200..-5).step_by(1).collect::<Vec<isize>>(), []);
    assert_eq!((200..200).step_by(1).collect::<Vec<isize>>(), []);
}

#[test]
fn test_reverse() {
    let mut ys = [1, 2, 3, 4, 5];
    ys.iter_mut().reverse_in_place();
    assert!(ys == [5, 4, 3, 2, 1]);
}

#[test]
fn test_peekable_is_empty() {
    let a = [1];
    let mut it = a.iter().peekable();
    assert!( !it.is_empty() );
    it.next();
    assert!( it.is_empty() );
}

#[test]
fn test_min_max() {
    let v: [isize; 0] = [];
    assert_eq!(v.iter().min_max(), NoElements);

    let v = [1];
    assert!(v.iter().min_max() == OneElement(&1));

    let v = [1, 2, 3, 4, 5];
    assert!(v.iter().min_max() == MinMax(&1, &5));

    let v = [1, 2, 3, 4, 5, 6];
    assert!(v.iter().min_max() == MinMax(&1, &6));

    let v = [1, 1, 1, 1];
    assert!(v.iter().min_max() == MinMax(&1, &1));
}

#[test]
fn test_min_max_result() {
    let r: MinMaxResult<isize> = NoElements;
    assert_eq!(r.into_option(), None);

    let r = OneElement(1);
    assert_eq!(r.into_option(), Some((1,1)));

    let r = MinMax(1,2);
    assert_eq!(r.into_option(), Some((1,2)));
}

#[test]
fn test_iterate() {
    let mut it = iterate(1, |x| x * 2);
    assert_eq!(it.next(), Some(1));
    assert_eq!(it.next(), Some(2));
    assert_eq!(it.next(), Some(4));
    assert_eq!(it.next(), Some(8));
}

#[test]
fn test_repeat() {
    let mut it = repeat(42);
    assert_eq!(it.next(), Some(42));
    assert_eq!(it.next(), Some(42));
    assert_eq!(it.next(), Some(42));
}

#[test]
fn test_fuse() {
    let mut it = 0..3;
    assert_eq!(it.len(), 3);
    assert_eq!(it.next(), Some(0));
    assert_eq!(it.len(), 2);
    assert_eq!(it.next(), Some(1));
    assert_eq!(it.len(), 1);
    assert_eq!(it.next(), Some(2));
    assert_eq!(it.len(), 0);
    assert_eq!(it.next(), None);
    assert_eq!(it.len(), 0);
    assert_eq!(it.next(), None);
    assert_eq!(it.len(), 0);
    assert_eq!(it.next(), None);
    assert_eq!(it.len(), 0);
}

#[bench]
fn bench_rposition(b: &mut Bencher) {
    let it: Vec<usize> = (0..300).collect();
    b.iter(|| {
        it.iter().rposition(|&x| x <= 150);
    });
}

#[bench]
fn bench_skip_while(b: &mut Bencher) {
    b.iter(|| {
        let it = 0..100;
        let mut sum = 0;
        it.skip_while(|&x| { sum += x; sum < 4000 }).all(|_| true);
    });
}

#[bench]
fn bench_multiple_take(b: &mut Bencher) {
    let mut it = (0..42).cycle();
    b.iter(|| {
        let n = it.next().unwrap();
        for _ in 0..n {
            it.clone().take(it.next().unwrap()).all(|_| true);
        }
    });
}

fn scatter(x: i32) -> i32 { (x * 31) % 127 }

#[bench]
fn bench_max_by(b: &mut Bencher) {
    b.iter(|| {
        let it = 0..100;
        it.max_by(|&x| scatter(x))
    })
}

// http://www.reddit.com/r/rust/comments/31syce/using_iterators_to_find_the_index_of_the_min_or/
#[bench]
fn bench_max_by2(b: &mut Bencher) {
    fn max_index_iter(array: &[i32]) -> usize {
        array.iter().enumerate().max_by(|&(_, item)| item).unwrap().0
    }

    let mut data = vec![0i32; 1638];
    data[514] = 9999;

    b.iter(|| max_index_iter(&data));
}

#[bench]
fn bench_max(b: &mut Bencher) {
    b.iter(|| {
        let it = 0..100;
        it.map(scatter).max()
    })
}
