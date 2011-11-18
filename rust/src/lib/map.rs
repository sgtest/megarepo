/*
Module: map

A hashmap
*/

/* Section: Types */

/*
Type: hashfn

A function that returns a hash of a value
*/
type hashfn<K> = fn(K) -> uint;

/*
Type: eqfn

Equality
*/
type eqfn<K> = fn(K, K) -> bool;

/*
Type: hashset

A convenience type to treat a hashmap as a set
*/
type hashset<K> = hashmap<K, ()>;

/*
Obj: hashmap
*/
type hashmap<K, V> = obj {
    /*
    Method: size

    Return the number of elements in the map
    */
    fn size() -> uint;
    /*
    Method: insert

    Add a value to the map. If the map already contains a value for
    the specified key then the original value is replaced.

    Returns:

    True if the key did not already exist in the map
    */
    fn insert(K, V) -> bool;
    /*
    Method: contains_key

    Returns true if the map contains a value for the specified key
    */
    fn contains_key(K) -> bool;
    /*
    Method: get

    Get the value for the specified key

    Failure:

    If the key does not exist in the map
    */
    fn get(K) -> V;
    /*
    Method: find

    Get the value for the specified key. If the key does not exist
    in the map then returns none.
    */
    fn find(K) -> option::t<V>;
    /*
    Method: remove

    Remove and return a value from the map. If the key does not exist
    in the map then returns none.
    */
    fn remove(K) -> option::t<V>;
    /*
    Method: rehash

    Force map growth and rehashing
    */
    fn rehash();
    /*
    Method: items

    Iterate over all the key/value pairs in the map
    */
    fn items(block(K, V));
    /*
    Method: keys

    Iterate over all the keys in the map
    */
    fn keys(block(K));
    /*
    Iterate over all the values in the map
    */
    fn values(block(V));
};

/* Section: Operations */

/*
Function: mk_hashmap

Construct a hashmap

Parameters:

hasher - The hash function for key type K
eqer - The equality function for key type K
*/
fn mk_hashmap<copy K, copy V>(hasher: hashfn<K>, eqer: eqfn<K>)
    -> hashmap<K, V> {
    let initial_capacity: uint = 32u; // 2^5

    let load_factor: util::rational = {num: 3, den: 4};
    tag bucket<copy K, copy V> { nil; deleted; some(K, V); }
    fn make_buckets<copy K, copy V>(nbkts: uint) -> [mutable bucket<K, V>] {
        ret vec::init_elt_mut::<bucket<K, V>>(nil::<K, V>, nbkts);
    }
    // Derive two hash functions from the one given by taking the upper
    // half and lower half of the uint bits.  Our bucket probing
    // sequence is then defined by
    //
    //   hash(key, i) := hashl(key) * i + hashr(key)   for i = 0, 1, 2, ...
    //
    // Tearing the hash function apart this way is kosher in practice
    // as, assuming 32-bit uints, the table would have to be at 2^32
    // buckets before the resulting pair of hash functions no longer
    // probes all buckets for a fixed key.  Note that hashl is made to
    // output odd numbers (hence coprime to the number of nbkts, which
    // is always a power of 2), so that all buckets are probed for a
    // fixed key.

    fn hashl(n: uint, _nbkts: uint) -> uint { ret (n >>> 16u) * 2u + 1u; }
    fn hashr(n: uint, _nbkts: uint) -> uint { ret 0x0000_ffff_u & n; }
    fn hash(h: uint, nbkts: uint, i: uint) -> uint {
        ret (hashl(h, nbkts) * i + hashr(h, nbkts)) % nbkts;
    }
    /**
     * We attempt to never call this with a full table.  If we do, it
     * will fail.
     */

    fn insert_common<copy K, copy V>(hasher: hashfn<K>, eqer: eqfn<K>,
                                     bkts: [mutable bucket<K, V>],
                                     nbkts: uint, key: K, val: V) -> bool {
        let i: uint = 0u;
        let h: uint = hasher(key);
        while i < nbkts {
            let j: uint = hash(h, nbkts, i);
            alt bkts[j] {
              some(k, _) {
                // Copy key to please alias analysis.

                let k_ = k;
                if eqer(key, k_) { bkts[j] = some(k_, val); ret false; }
                i += 1u;
              }
              _ { bkts[j] = some(key, val); ret true; }
            }
        }
        fail; // full table
    }
    fn find_common<copy K, copy V>(hasher: hashfn<K>, eqer: eqfn<K>,
                                   bkts: [mutable bucket<K, V>],
                                   nbkts: uint, key: K) -> option::t<V> {
        let i: uint = 0u;
        let h: uint = hasher(key);
        while i < nbkts {
            let j: uint = hash(h, nbkts, i);
            alt bkts[j] {
              some(k, v) {
                // Copy to please alias analysis.
                let k_ = k;
                let v_ = v;
                if eqer(key, k_) { ret option::some(v_); }
              }
              nil. { ret option::none; }
              deleted. { }
            }
            i += 1u;
        }
        ret option::none;
    }
    fn rehash<copy K, copy V>(hasher: hashfn<K>, eqer: eqfn<K>,
                              oldbkts: [mutable bucket<K, V>],
                              _noldbkts: uint,
                              newbkts: [mutable bucket<K, V>],
                              nnewbkts: uint) {
        for b: bucket<K, V> in oldbkts {
            alt b {
              some(k_, v_) {
                let k = k_;
                let v = v_;
                insert_common(hasher, eqer, newbkts, nnewbkts, k, v);
              }
              _ { }
            }
        }
    }
    obj hashmap<copy K, copy V>(hasher: hashfn<K>,
                                eqer: eqfn<K>,
                                mutable bkts: [mutable bucket<K, V>],
                                mutable nbkts: uint,
                                mutable nelts: uint,
                                lf: util::rational) {
        fn size() -> uint { ret nelts; }
        fn insert(key: K, val: V) -> bool {
            let load: util::rational =
                {num: nelts + 1u as int, den: nbkts as int};
            if !util::rational_leq(load, lf) {
                let nnewbkts: uint = uint::next_power_of_two(nbkts + 1u);
                let newbkts = make_buckets(nnewbkts);
                rehash(hasher, eqer, bkts, nbkts, newbkts, nnewbkts);
                bkts = newbkts;
                nbkts = nnewbkts;
            }
            if insert_common(hasher, eqer, bkts, nbkts, key, val) {
                nelts += 1u;
                ret true;
            }
            ret false;
        }
        fn contains_key(key: K) -> bool {
            ret alt find_common(hasher, eqer, bkts, nbkts, key) {
                  option::some(_) { true }
                  _ { false }
                };
        }
        fn get(key: K) -> V {
            ret alt find_common(hasher, eqer, bkts, nbkts, key) {
                  option::some(val) { val }
                  _ { fail }
                };
        }
        fn find(key: K) -> option::t<V> {
            be find_common(hasher, eqer, bkts, nbkts, key);
        }
        fn remove(key: K) -> option::t<V> {
            let i: uint = 0u;
            let h: uint = hasher(key);
            while i < nbkts {
                let j: uint = hash(h, nbkts, i);
                alt bkts[j] {
                  some(k, v) {
                    let k_ = k;
                    let vo = option::some(v);
                    if eqer(key, k_) {
                        bkts[j] = deleted;
                        nelts -= 1u;
                        ret vo;
                    }
                  }
                  deleted. { }
                  nil. { ret option::none; }
                }
                i += 1u;
            }
            ret option::none;
        }
        fn rehash() {
            let newbkts = make_buckets(nbkts);
            rehash(hasher, eqer, bkts, nbkts, newbkts, nbkts);
            bkts = newbkts;
        }
        fn items(it: block(K, V)) {
            for b in bkts {
                alt b { some(k, v) { it(copy k, copy v); } _ { } }
            }
        }
        fn keys(it: block(K)) {
            for b in bkts {
                alt b { some(k, _) { it(copy k); } _ { } }
            }
        }
        fn values(it: block(V)) {
            for b in bkts {
                alt b { some(_, v) { it(copy v); } _ { } }
            }
        }
    }
    let bkts = make_buckets(initial_capacity);
    ret hashmap(hasher, eqer, bkts, initial_capacity, 0u, load_factor);
}

/*
Function: new_str_hash

Construct a hashmap for string keys
*/
fn new_str_hash<copy V>() -> hashmap<str, V> {
    ret mk_hashmap(str::hash, str::eq);
}

/*
Function: new_int_hash

Construct a hashmap for int keys
*/
fn new_int_hash<copy V>() -> hashmap<int, V> {
    fn hash_int(&&x: int) -> uint { ret x as uint; }
    fn eq_int(&&a: int, &&b: int) -> bool { ret a == b; }
    ret mk_hashmap(hash_int, eq_int);
}

/*
Function: new_uint_hash

Construct a hashmap for uint keys
*/
fn new_uint_hash<copy V>() -> hashmap<uint, V> {
    fn hash_uint(&&x: uint) -> uint { ret x; }
    fn eq_uint(&&a: uint, &&b: uint) -> bool { ret a == b; }
    ret mk_hashmap(hash_uint, eq_uint);
}

/*
Function: set_add

Convenience function for adding keys to a hashmap with nil type keys
*/
fn set_add<K>(set: hashset<K>, key: K) -> bool { ret set.insert(key, ()); }

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
