/*!

Sendable hash maps.  Very much a work in progress.

*/


/**
 * A function that returns a hash of a value
 *
 * The hash should concentrate entropy in the lower bits.
 */
type hashfn<K> = pure fn~(K) -> uint;
type eqfn<K> = pure fn~(K, K) -> bool;

/// Open addressing with linear probing.
mod linear {
    export linear_map, linear_map_with_capacity;

    const initial_capacity: uint = 32u; // 2^5
    type bucket<K,V> = {hash: uint, key: K, value: V};
    enum linear_map<K,V> {
        linear_map_({
            hashfn: pure fn~(x: &K) -> uint,
            eqfn: pure fn~(x: &K, y: &K) -> bool,
            resize_at: uint,
            size: uint,
            buckets: ~[option<bucket<K,V>>]})
    }

    // FIXME(#2979) -- with #2979 we could rewrite found_entry
    // to have type option<&bucket<K,V>> which would be nifty
    enum search_result {
        found_entry(uint), found_hole(uint), table_full
    }

    fn resize_at(capacity: uint) -> uint {
        ((capacity as float) * 3. / 4.) as uint
    }

    fn linear_map<K,V>(
        +hashfn: pure fn~(x: &K) -> uint,
        +eqfn: pure fn~(x: &K, y: &K) -> bool) -> linear_map<K,V> {

        linear_map_with_capacity(hashfn, eqfn, 32)
    }

    fn linear_map_with_capacity<K,V>(
        +hashfn: pure fn~(x: &K) -> uint,
        +eqfn: pure fn~(x: &K, y: &K) -> bool,
        initial_capacity: uint) -> linear_map<K,V> {

        linear_map_({
            hashfn: hashfn,
            eqfn: eqfn,
            resize_at: resize_at(initial_capacity),
            size: 0,
            buckets: vec::from_fn(initial_capacity, |_i| none)})
    }

    // FIXME(#2979) would allow us to use region type for k
    unsafe fn borrow<K>(&&k: K) -> &K {
        let p: *K = ptr::addr_of(k);
        p as &K
    }

    impl private_const_methods<K,V> for &const linear_map<K,V> {
        #[inline(always)]
        pure fn to_bucket(h: uint) -> uint {
            // FIXME(#3041) borrow a more sophisticated technique here from
            // Gecko, for example borrowing from Knuth, as Eich so
            // colorfully argues for here:
            // https://bugzilla.mozilla.org/show_bug.cgi?id=743107#c22
            h % self.buckets.len()
        }

        #[inline(always)]
        pure fn next_bucket(idx: uint, len_buckets: uint) -> uint {
            let n = (idx + 1) % len_buckets;
            unsafe{ // argh. log not considered pure.
                #debug["next_bucket(%?, %?) = %?", idx, len_buckets, n];
            }
            ret n;
        }

        #[inline(always)]
        pure fn bucket_sequence(hash: uint, op: fn(uint) -> bool) -> uint {
            let start_idx = self.to_bucket(hash);
            let len_buckets = self.buckets.len();
            let mut idx = start_idx;
            loop {
                if !op(idx) {
                    ret idx;
                }
                idx = self.next_bucket(idx, len_buckets);
                if idx == start_idx {
                    ret start_idx;
                }
            }
        }

        #[inline(always)]
        pure fn bucket_for_key(
            buckets: &[option<bucket<K,V>>],
            k: &K) -> search_result {

            let hash = self.hashfn(k);
            self.bucket_for_key_with_hash(buckets, hash, k)
        }

        #[inline(always)]
        pure fn bucket_for_key_with_hash(
            buckets: &[option<bucket<K,V>>],
            hash: uint,
            k: &K) -> search_result {

            let _ = for self.bucket_sequence(hash) |i| {
                alt buckets[i] {
                  some(bkt) {
                    if bkt.hash == hash && self.eqfn(k, &bkt.key) {
                        ret found_entry(i);
                    }
                  }
                  none => {
                    ret found_hole(i);
                  }
                }
            };
            ret table_full;
        }
    }

    impl private_mut_methods<K,V> for &mut linear_map<K,V> {
        /// Expands the capacity of the array and re-inserts each
        /// of the existing buckets.
        fn expand() {
            let old_capacity = self.buckets.len();
            let new_capacity = old_capacity * 2;
            self.resize_at = ((new_capacity as float) * 3.0 / 4.0) as uint;

            let mut old_buckets = vec::from_fn(new_capacity, |_i| none);
            self.buckets <-> old_buckets;

            for uint::range(0, old_capacity) |i| {
                let mut bucket = none;
                bucket <-> old_buckets[i];
                if bucket.is_some() {
                    self.insert_bucket(bucket);
                }
            }
        }

        fn insert_bucket(+bucket: option<bucket<K,V>>) {
            let {hash, key, value} <- option::unwrap(bucket);
            let _ = self.insert_internal(hash, key, value);
        }

        /// Inserts the key value pair into the buckets.
        /// Assumes that there will be a bucket.
        /// True if there was no previous entry with that key
        fn insert_internal(hash: uint, +k: K, +v: V) -> bool {
            alt self.bucket_for_key_with_hash(self.buckets, hash,
                                              unsafe{borrow(k)}) {
              table_full => {fail ~"Internal logic error";}
              found_hole(idx) {
                #debug["insert fresh (%?->%?) at idx %?, hash %?",
                       k, v, idx, hash];
                self.buckets[idx] = some({hash: hash, key: k, value: v});
                self.size += 1;
                ret true;
              }
              found_entry(idx) => {
                #debug["insert overwrite (%?->%?) at idx %?, hash %?",
                       k, v, idx, hash];
                self.buckets[idx] = some({hash: hash, key: k, value: v});
                ret false;
              }
            }
        }
    }

    impl mut_methods<K,V> for &mut linear_map<K,V> {
        fn insert(+k: K, +v: V) -> bool {
            if self.size >= self.resize_at {
                // n.b.: We could also do this after searching, so
                // that we do not resize if this call to insert is
                // simply going to update a key in place.  My sense
                // though is that it's worse to have to search through
                // buckets to find the right spot twice than to just
                // resize in this corner case.
                self.expand();
            }

            let hash = self.hashfn(unsafe{borrow(k)});
            self.insert_internal(hash, k, v)
        }

        fn remove(k: &K) -> bool {
            // Removing from an open-addressed hashtable
            // is, well, painful.  The problem is that
            // the entry may lie on the probe path for other
            // entries, so removing it would make you think that
            // those probe paths are empty.
            //
            // To address this we basically have to keep walking,
            // re-inserting entries we find until we reach an empty
            // bucket.  We know we will eventually reach one because
            // we insert one ourselves at the beginning (the removed
            // entry).
            //
            // I found this explanation elucidating:
            // http://www.maths.lse.ac.uk/Courses/MA407/del-hash.pdf

            let mut idx = alt self.bucket_for_key(self.buckets, k) {
              table_full | found_hole(_) => {
                ret false;
              }
              found_entry(idx) => {
                idx
              }
            };

            let len_buckets = self.buckets.len();
            self.buckets[idx] = none;
            idx = self.next_bucket(idx, len_buckets);
            while self.buckets[idx].is_some() {
                let mut bucket = none;
                bucket <-> self.buckets[idx];
                self.insert_bucket(bucket);
                idx = self.next_bucket(idx, len_buckets);
            }
            ret true;
        }
    }

    impl private_imm_methods<K,V> for &linear_map<K,V> {
        fn search(hash: uint, op: fn(x: &option<bucket<K,V>>) -> bool) {
            let _ = self.bucket_sequence(hash, |i| op(&self.buckets[i]));
        }
    }

    impl const_methods<K,V> for &const linear_map<K,V> {
        fn size() -> uint {
            self.size
        }

        fn contains_key(k: &K) -> bool {
            alt self.bucket_for_key(self.buckets, k) {
              found_entry(_) => {true}
              table_full | found_hole(_) => {false}
            }
        }
    }

    impl const_methods<K,V: copy> for &const linear_map<K,V> {
        fn find(k: &K) -> option<V> {
            alt self.bucket_for_key(self.buckets, k) {
              found_entry(idx) => {
                alt check self.buckets[idx] {
                  some(bkt) => {some(copy bkt.value)}
                }
              }
              table_full | found_hole(_) => {
                none
              }
            }
        }

        fn get(k: &K) -> V {
            let value = self.find(k);
            if value.is_none() {
                fail #fmt["No entry found for key: %?", k];
            }
            option::unwrap(value)
        }

        fn [](k: &K) -> V {
            self.get(k)
        }
    }

    /*
    FIXME --- #2979 must be fixed to typecheck this
    impl imm_methods<K,V> for &linear_map<K,V> {
        fn find_ptr(k: K) -> option<&V> {
            //XXX this should not type check as written, but it should
            //be *possible* to typecheck it...
            self.with_ptr(k, |v| v)
        }
    }
    */
}

#[test]
mod test {

    import linear::linear_map;

    pure fn uint_hash(x: &uint) -> uint { *x }
    pure fn uint_eq(x: &uint, y: &uint) -> bool { *x == *y }

    fn int_linear_map<V>() -> linear_map<uint,V> {
        ret linear_map(uint_hash, uint_eq);
    }

    #[test]
    fn inserts() {
        let mut m = int_linear_map();
        assert (&mut m).insert(1, 2);
        assert (&mut m).insert(2, 4);
        assert (&m).get(&1) == 2;
        assert (&m).get(&2) == 4;
    }

    #[test]
    fn overwrite() {
        let mut m = int_linear_map();
        assert (&mut m).insert(1, 2);
        assert (&m).get(&1) == 2;
        assert !(&mut m).insert(1, 3);
        assert (&m).get(&1) == 3;
    }

    #[test]
    fn conflicts() {
        let mut m = linear::linear_map_with_capacity(uint_hash, uint_eq, 4);
        assert (&mut m).insert(1, 2);
        assert (&mut m).insert(5, 3);
        assert (&mut m).insert(9, 4);
        assert (&m).get(&9) == 4;
        assert (&m).get(&5) == 3;
        assert (&m).get(&1) == 2;
    }

    #[test]
    fn conflict_remove() {
        let mut m = linear::linear_map_with_capacity(uint_hash, uint_eq, 4);
        assert (&mut m).insert(1, 2);
        assert (&mut m).insert(5, 3);
        assert (&mut m).insert(9, 4);
        assert (&mut m).remove(&1);
        assert (&m).get(&9) == 4;
        assert (&m).get(&5) == 3;
    }
}