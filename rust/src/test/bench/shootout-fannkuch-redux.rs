use core::from_str::FromStr;
use core::i32::range;
use core::vec::MutableVector;

fn max(a: i32, b: i32) -> i32 {
    if a > b {
        a
    } else {
        b
    }
}

#[inline(never)]
fn fannkuch_redux(n: i32) -> i32 {
    let mut perm = vec::from_elem(n as uint, 0i32);
    let mut perm1 = vec::from_fn(n as uint, |i| i as i32);
    let mut count = vec::from_elem(n as uint, 0i32);
    let mut max_flips_count = 0i32, perm_count = 0i32, checksum = 0i32;

    let mut r = n;
    loop {
        unsafe {
            while r != 1 {
                count.unsafe_set((r-1) as uint, r);
                r -= 1;
            }

            // XXX: Need each2_mut.
            for vec::eachi_mut(perm) |i, perm_i| {
                *perm_i = perm1.unsafe_get(i);
            }

            let mut flips_count: i32 = 0;
            let mut k: i32;
            loop {
                k = perm.unsafe_get(0);
                if k == 0 {
                    break;
                }

                let k2 = (k+1) >> 1;
                for range(0, k2) |i| {
                    let (perm_i, perm_k_i) = {
                        (perm.unsafe_get(i as uint),
                            perm.unsafe_get((k-i) as uint))
                    };
                    perm.unsafe_set(i as uint, perm_k_i);
                    perm.unsafe_set((k-i) as uint, perm_i);
                }
                flips_count += 1;
            }

            max_flips_count = max(max_flips_count, flips_count);
            checksum += if perm_count % 2 == 0 {
                flips_count
            } else {
                -flips_count
            };

            // Use incremental change to generate another permutation.
            loop {
                if r == n {
                    println(checksum.to_str());
                    return max_flips_count;
                }

                let perm0 = perm1[0];
                let mut i: i32 = 0;
                while i < r {
                    let j = i + 1;
                    let perm1_j = { perm1.unsafe_get(j as uint) };
                    perm1.unsafe_set(i as uint, perm1_j);
                    i = j;
                }
                perm1.unsafe_set(r as uint, perm0);

                let count_r = { count.unsafe_get(r as uint) };
                count.unsafe_set(r as uint, count_r - 1);
                if count.unsafe_get(r as uint) > 0 {
                    break;
                }
                r += 1;
            }

            perm_count += 1;
        }
    }
}

#[fixed_stack_segment]
fn main() {
    let n: i32 = FromStr::from_str(os::args()[1]).get();
    println(fmt!("Pfannkuchen(%d) = %d", n as int, fannkuch_redux(n) as int));
}

