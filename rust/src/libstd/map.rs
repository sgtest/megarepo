#[doc = "A map type"];

import chained::hashmap;
export hashmap, hashfn, eqfn, set, map, chained, hashmap, str_hash;
export bytes_hash, int_hash, uint_hash, set_add;
export hash_from_vec, hash_from_strs, hash_from_bytes;
export hash_from_ints, hash_from_uints;
export vec_from_set;

#[doc = "
A function that returns a hash of a value

The hash should concentrate entropy in the lower bits.
"]
type hashfn<K> = fn@(K) -> uint;

type eqfn<K> = fn@(K, K) -> bool;

#[doc = "A convenience type to treat a hashmap as a set"]
type set<K> = hashmap<K, ()>;

type hashmap<K, V> = chained::t<K, V>;

iface map<K: copy, V: copy> {
    #[doc = "Return the number of elements in the map"]
    fn size() -> uint;

    #[doc = "
    Add a value to the map.

    If the map already contains a value for the specified key then the
    original value is replaced.

    Returns true if the key did not already exist in the map
    "]
    fn insert(K, V) -> bool;

    #[doc = "Returns true if the map contains a value for the specified key"]
    fn contains_key(K) -> bool;

    #[doc = "
    Get the value for the specified key. Fails if the key does not exist in
    the map.
    "]
    fn get(K) -> V;

    #[doc = "
    Get the value for the specified key. If the key does not exist in
    the map then returns none.
    "]
    fn find(K) -> option<V>;

    #[doc = "
    Remove and return a value from the map. If the key does not exist
    in the map then returns none.
    "]
    fn remove(K) -> option<V>;

    #[doc = "Iterate over all the key/value pairs in the map"]
    fn each(fn(K, V) -> bool);

    #[doc = "Iterate over all the keys in the map"]
    fn each_key(fn(K) -> bool);

    #[doc = "Iterate over all the values in the map"]
    fn each_value(fn(V) -> bool);
}

// FIXME: package this up and export it as a datatype usable for
// external code that doesn't want to pay the cost of a box. (#2344)
mod chained {
    type entry<K, V> = {
        hash: uint,
        key: K,
        mut value: V,
        mut next: chain<K, V>
    };

    enum chain<K, V> {
        present(@entry<K, V>),
        absent
    }

    type t<K, V> = @{
        mut count: uint,
        mut chains: [mut chain<K,V>],
        hasher: hashfn<K>,
        eqer: eqfn<K>
    };

    enum search_result<K, V> {
        not_found,
        found_first(uint, @entry<K,V>),
        found_after(@entry<K,V>, @entry<K,V>)
    }

    fn search_rem<K: copy, V: copy>(
        tbl: t<K,V>, k: K, h: uint, idx: uint,
        e_root: @entry<K,V>) -> search_result<K,V> {
        let mut e0 = e_root;
        let mut comp = 1u;   // for logging
        loop {
            alt e0.next {
              absent {
                #debug("search_tbl: absent, comp %u, hash %u, idx %u",
                       comp, h, idx);
                ret not_found;
              }
              present(e1) {
                comp += 1u;
                let e1_key = e1.key; // Satisfy alias checker.
                if e1.hash == h && tbl.eqer(e1_key, k) {
                    #debug("search_tbl: present, comp %u, hash %u, idx %u",
                           comp, h, idx);
                    ret found_after(e0, e1);
                } else {
                    e0 = e1;
                }
              }
            }
        };
    }

    fn search_tbl<K: copy, V: copy>(
        tbl: t<K,V>, k: K, h: uint) -> search_result<K,V> {
        let idx = h % vec::len(tbl.chains);
        alt tbl.chains[idx] {
          absent {
            #debug("search_tbl: absent, comp %u, hash %u, idx %u",
                   0u, h, idx);
            ret not_found;
          }
          present(e) {
            // FIXME: This copy of the key is not good for perf
            if e.hash == h && tbl.eqer(copy e.key, k) {
                #debug("search_tbl: present, comp %u, hash %u, idx %u",
                       1u, h, idx);
                ret found_first(idx, e);
            } else {
                ret search_rem(tbl, k, h, idx, e);
            }
          }
        }
    }

    fn insert<K: copy, V: copy>(tbl: t<K,V>, k: K, v: V) -> bool {
        let hash = tbl.hasher(k);
        alt search_tbl(tbl, k, hash) {
          not_found {
            tbl.count += 1u;
            let idx = hash % vec::len(tbl.chains);
            let old_chain = tbl.chains[idx];
            tbl.chains[idx] = present(@{
                hash: hash,
                key: k,
                mut value: v,
                mut next: old_chain});
            ret true;
          }
          found_first(_, entry) {
            entry.value = v;
            ret false;
          }
          found_after(_, entry) {
            entry.value = v;
            ret false
          }
        }
    }

    fn get<K: copy, V: copy>(tbl: t<K,V>, k: K) -> core::option<V> {
        alt search_tbl(tbl, k, tbl.hasher(k)) {
          not_found {
            ret core::option::none;
          }

          found_first(_, entry) {
            ret core::option::some(entry.value);
          }

          found_after(_, entry) {
            ret core::option::some(entry.value);
          }
        }
    }

    fn remove<K: copy, V: copy>(tbl: t<K,V>, k: K) -> core::option<V> {
        alt search_tbl(tbl, k, tbl.hasher(k)) {
          not_found {
            ret core::option::none;
          }

          found_first(idx, entry) {
            tbl.count -= 1u;
            tbl.chains[idx] = entry.next;
            ret core::option::some(entry.value);
          }

          found_after(eprev, entry) {
            tbl.count -= 1u;
            eprev.next = entry.next;
            ret core::option::some(entry.value);
          }
        }
    }

    fn chains<K: copy, V: copy>(nchains: uint) -> [mut chain<K,V>] {
        ret vec::to_mut(vec::from_elem(nchains, absent));
    }

    fn each_entry<K: copy, V: copy>(tbl: t<K, V>,
                                    blk: fn(@entry<K,V>) -> bool) {
        let mut i = 0u, n = vec::len(tbl.chains);
        while i < n {
            let mut chain = tbl.chains[i];
            loop {
                chain = alt chain {
                  absent { break; }
                  present(entry) {
                    let next = entry.next;
                    if !blk(entry) { ret; }
                    next
                  }
                }
            }
            i += 1u;
        }
    }

    fn rehash<K: copy, V: copy>(tbl: t<K,V>) {
        let n_old_chains = vec::len(tbl.chains);
        let n_new_chains: uint = uint::next_power_of_two(n_old_chains + 1u);
        let new_chains = chains(n_new_chains);
        for each_entry(tbl) {|entry|
            let idx = entry.hash % n_new_chains;
            entry.next = new_chains[idx];
            new_chains[idx] = present(entry);
        }
        tbl.chains = new_chains;
    }

    fn each<K: copy, V: copy>(tbl: t<K,V>, blk: fn(K,V) -> bool) {
        for each_entry(tbl) {|entry|
            if !blk(copy entry.key, copy entry.value) { break; }
        }
    }

    impl hashmap<K: copy, V: copy> of map<K, V> for t<K, V> {
        fn size() -> uint { self.count }

        fn insert(k: K, v: V) -> bool {
            let grew = insert(self, k, v);
            if grew {
                let nchains = vec::len(self.chains);
                let load = {num: (self.count + 1u) as int,
                            den: nchains as int};
                // Structural consts would be nice. This is a const 3/4
                // load factor that we compare against.
                if !util::rational_leq(load, {num:3, den:4}) { rehash(self); }
            }
            grew
        }

        fn contains_key(k: K) -> bool { option::is_some(get(self, k)) }

        fn get(k: K) -> V { option::get(get(self, k)) }

        fn find(k: K) -> option<V> { get(self, k) }

        fn remove(k: K) -> option<V> { remove(self, k) }

        fn each(blk: fn(K, V) -> bool) { each(self, blk); }

        fn each_key(blk: fn(K) -> bool) { each(self) { |k, _v| blk(k)} }

        fn each_value(blk: fn(V) -> bool) { each(self) { |_k, v| blk(v)} }
    }

    fn mk<K: copy, V: copy>(hasher: hashfn<K>, eqer: eqfn<K>) -> t<K,V> {
        let initial_capacity: uint = 32u; // 2^5
        let slf: t<K, V> = @{mut count: 0u,
                             mut chains: chains(initial_capacity),
                             hasher: hasher,
                             eqer: eqer};
        slf
    }
}

/*
Function: hashmap

Construct a hashmap.

Parameters:

hasher - The hash function for key type K
eqer - The equality function for key type K
*/
fn hashmap<K: copy, V: copy>(hasher: hashfn<K>, eqer: eqfn<K>)
        -> hashmap<K, V> {
    chained::mk(hasher, eqer)
}

#[doc = "Construct a hashmap for string keys"]
fn str_hash<V: copy>() -> hashmap<str, V> {
    ret hashmap(str::hash, str::eq);
}

#[doc = "Construct a hashmap for byte string keys"]
fn bytes_hash<V: copy>() -> hashmap<[u8], V> {
    ret hashmap(vec::u8::hash, vec::u8::eq);
}

fn hash_int(&&x: int) -> uint { int::hash(x) }
fn eq_int(&&a: int, &&b: int) -> bool { ret a == b; }

#[doc = "Construct a hashmap for int keys"]
fn int_hash<V: copy>() -> hashmap<int, V> {
    ret hashmap(hash_int, eq_int);
}

fn hash_uint(&&x: uint) -> uint { uint::hash(x) }
fn eq_uint(&&a: uint, &&b: uint) -> bool { ret a == b; }

#[doc = "Construct a hashmap for uint keys"]
fn uint_hash<V: copy>() -> hashmap<uint, V> {
    ret hashmap(hash_uint, eq_uint);
}

#[doc = "
Convenience function for adding keys to a hashmap with nil type keys
"]
fn set_add<K: copy>(set: set<K>, key: K) -> bool { ret set.insert(key, ()); }

#[doc = "
Convert a set into a vector.
"]
fn vec_from_set<T: copy>(s: set<T>) -> [T] {
    let mut v = [];
    s.each_key() {|k|
        v += [k];
        true
    };
    v
}

#[doc = "Construct a hashmap from a vector"]
fn hash_from_vec<K: copy, V: copy>(hasher: hashfn<K>, eqer: eqfn<K>,
                                   items: [(K, V)]) -> hashmap<K, V> {
    let map = hashmap(hasher, eqer);
    vec::iter(items) { |item|
        let (key, value) = item;
        map.insert(key, value);
    }
    map
}

#[doc = "Construct a hashmap from a vector with string keys"]
fn hash_from_strs<V: copy>(items: [(str, V)]) -> hashmap<str, V> {
    hash_from_vec(str::hash, str::eq, items)
}

#[doc = "Construct a hashmap from a vector with byte keys"]
fn hash_from_bytes<V: copy>(items: [([u8], V)]) -> hashmap<[u8], V> {
    hash_from_vec(vec::u8::hash, vec::u8::eq, items)
}

#[doc = "Construct a hashmap from a vector with int keys"]
fn hash_from_ints<V: copy>(items: [(int, V)]) -> hashmap<int, V> {
    hash_from_vec(hash_int, eq_int, items)
}

#[doc = "Construct a hashmap from a vector with uint keys"]
fn hash_from_uints<V: copy>(items: [(uint, V)]) -> hashmap<uint, V> {
    hash_from_vec(hash_uint, eq_uint, items)
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_simple() {
        #debug("*** starting test_simple");
        fn eq_uint(&&x: uint, &&y: uint) -> bool { ret x == y; }
        fn uint_id(&&x: uint) -> uint { x }
        let hasher_uint: map::hashfn<uint> = uint_id;
        let eqer_uint: map::eqfn<uint> = eq_uint;
        let hasher_str: map::hashfn<str> = str::hash;
        let eqer_str: map::eqfn<str> = str::eq;
        #debug("uint -> uint");
        let hm_uu: map::hashmap<uint, uint> =
            map::hashmap::<uint, uint>(hasher_uint, eqer_uint);
        assert (hm_uu.insert(10u, 12u));
        assert (hm_uu.insert(11u, 13u));
        assert (hm_uu.insert(12u, 14u));
        assert (hm_uu.get(11u) == 13u);
        assert (hm_uu.get(12u) == 14u);
        assert (hm_uu.get(10u) == 12u);
        assert (!hm_uu.insert(12u, 14u));
        assert (hm_uu.get(12u) == 14u);
        assert (!hm_uu.insert(12u, 12u));
        assert (hm_uu.get(12u) == 12u);
        let ten: str = "ten";
        let eleven: str = "eleven";
        let twelve: str = "twelve";
        #debug("str -> uint");
        let hm_su: map::hashmap<str, uint> =
            map::hashmap::<str, uint>(hasher_str, eqer_str);
        assert (hm_su.insert("ten", 12u));
        assert (hm_su.insert(eleven, 13u));
        assert (hm_su.insert("twelve", 14u));
        assert (hm_su.get(eleven) == 13u);
        assert (hm_su.get("eleven") == 13u);
        assert (hm_su.get("twelve") == 14u);
        assert (hm_su.get("ten") == 12u);
        assert (!hm_su.insert("twelve", 14u));
        assert (hm_su.get("twelve") == 14u);
        assert (!hm_su.insert("twelve", 12u));
        assert (hm_su.get("twelve") == 12u);
        #debug("uint -> str");
        let hm_us: map::hashmap<uint, str> =
            map::hashmap::<uint, str>(hasher_uint, eqer_uint);
        assert (hm_us.insert(10u, "twelve"));
        assert (hm_us.insert(11u, "thirteen"));
        assert (hm_us.insert(12u, "fourteen"));
        assert (str::eq(hm_us.get(11u), "thirteen"));
        assert (str::eq(hm_us.get(12u), "fourteen"));
        assert (str::eq(hm_us.get(10u), "twelve"));
        assert (!hm_us.insert(12u, "fourteen"));
        assert (str::eq(hm_us.get(12u), "fourteen"));
        assert (!hm_us.insert(12u, "twelve"));
        assert (str::eq(hm_us.get(12u), "twelve"));
        #debug("str -> str");
        let hm_ss: map::hashmap<str, str> =
            map::hashmap::<str, str>(hasher_str, eqer_str);
        assert (hm_ss.insert(ten, "twelve"));
        assert (hm_ss.insert(eleven, "thirteen"));
        assert (hm_ss.insert(twelve, "fourteen"));
        assert (str::eq(hm_ss.get("eleven"), "thirteen"));
        assert (str::eq(hm_ss.get("twelve"), "fourteen"));
        assert (str::eq(hm_ss.get("ten"), "twelve"));
        assert (!hm_ss.insert("twelve", "fourteen"));
        assert (str::eq(hm_ss.get("twelve"), "fourteen"));
        assert (!hm_ss.insert("twelve", "twelve"));
        assert (str::eq(hm_ss.get("twelve"), "twelve"));
        #debug("*** finished test_simple");
    }


    /**
    * Force map growth
    */
    #[test]
    fn test_growth() {
        #debug("*** starting test_growth");
        let num_to_insert: uint = 64u;
        fn eq_uint(&&x: uint, &&y: uint) -> bool { ret x == y; }
        fn uint_id(&&x: uint) -> uint { x }
        #debug("uint -> uint");
        let hasher_uint: map::hashfn<uint> = uint_id;
        let eqer_uint: map::eqfn<uint> = eq_uint;
        let hm_uu: map::hashmap<uint, uint> =
            map::hashmap::<uint, uint>(hasher_uint, eqer_uint);
        let mut i: uint = 0u;
        while i < num_to_insert {
            assert (hm_uu.insert(i, i * i));
            #debug("inserting %u -> %u", i, i*i);
            i += 1u;
        }
        #debug("-----");
        i = 0u;
        while i < num_to_insert {
            #debug("get(%u) = %u", i, hm_uu.get(i));
            assert (hm_uu.get(i) == i * i);
            i += 1u;
        }
        assert (hm_uu.insert(num_to_insert, 17u));
        assert (hm_uu.get(num_to_insert) == 17u);
        #debug("-----");
        i = 0u;
        while i < num_to_insert {
            #debug("get(%u) = %u", i, hm_uu.get(i));
            assert (hm_uu.get(i) == i * i);
            i += 1u;
        }
        #debug("str -> str");
        let hasher_str: map::hashfn<str> = str::hash;
        let eqer_str: map::eqfn<str> = str::eq;
        let hm_ss: map::hashmap<str, str> =
            map::hashmap::<str, str>(hasher_str, eqer_str);
        i = 0u;
        while i < num_to_insert {
            assert hm_ss.insert(uint::to_str(i, 2u), uint::to_str(i * i, 2u));
            #debug("inserting \"%s\" -> \"%s\"",
                   uint::to_str(i, 2u),
                   uint::to_str(i*i, 2u));
            i += 1u;
        }
        #debug("-----");
        i = 0u;
        while i < num_to_insert {
            #debug("get(\"%s\") = \"%s\"",
                   uint::to_str(i, 2u),
                   hm_ss.get(uint::to_str(i, 2u)));
            assert (str::eq(hm_ss.get(uint::to_str(i, 2u)),
                            uint::to_str(i * i, 2u)));
            i += 1u;
        }
        assert (hm_ss.insert(uint::to_str(num_to_insert, 2u),
                             uint::to_str(17u, 2u)));
        assert (str::eq(hm_ss.get(uint::to_str(num_to_insert, 2u)),
                        uint::to_str(17u, 2u)));
        #debug("-----");
        i = 0u;
        while i < num_to_insert {
            #debug("get(\"%s\") = \"%s\"",
                   uint::to_str(i, 2u),
                   hm_ss.get(uint::to_str(i, 2u)));
            assert (str::eq(hm_ss.get(uint::to_str(i, 2u)),
                            uint::to_str(i * i, 2u)));
            i += 1u;
        }
        #debug("*** finished test_growth");
    }

    #[test]
    fn test_removal() {
        #debug("*** starting test_removal");
        let num_to_insert: uint = 64u;
        fn eq(&&x: uint, &&y: uint) -> bool { ret x == y; }
        fn hash(&&u: uint) -> uint {
            // This hash function intentionally causes collisions between
            // consecutive integer pairs.

            ret u / 2u * 2u;
        }
        assert (hash(0u) == hash(1u));
        assert (hash(2u) == hash(3u));
        assert (hash(0u) != hash(2u));
        let hasher: map::hashfn<uint> = hash;
        let eqer: map::eqfn<uint> = eq;
        let hm: map::hashmap<uint, uint> =
            map::hashmap::<uint, uint>(hasher, eqer);
        let mut i: uint = 0u;
        while i < num_to_insert {
            assert (hm.insert(i, i * i));
            #debug("inserting %u -> %u", i, i*i);
            i += 1u;
        }
        assert (hm.size() == num_to_insert);
        #debug("-----");
        #debug("removing evens");
        i = 0u;
        while i < num_to_insert {
            let v = hm.remove(i);
            alt v {
              option::some(u) { assert (u == i * i); }
              option::none { fail; }
            }
            i += 2u;
        }
        assert (hm.size() == num_to_insert / 2u);
        #debug("-----");
        i = 1u;
        while i < num_to_insert {
            #debug("get(%u) = %u", i, hm.get(i));
            assert (hm.get(i) == i * i);
            i += 2u;
        }
        #debug("-----");
        i = 1u;
        while i < num_to_insert {
            #debug("get(%u) = %u", i, hm.get(i));
            assert (hm.get(i) == i * i);
            i += 2u;
        }
        #debug("-----");
        i = 0u;
        while i < num_to_insert {
            assert (hm.insert(i, i * i));
            #debug("inserting %u -> %u", i, i*i);
            i += 2u;
        }
        assert (hm.size() == num_to_insert);
        #debug("-----");
        i = 0u;
        while i < num_to_insert {
            #debug("get(%u) = %u", i, hm.get(i));
            assert (hm.get(i) == i * i);
            i += 1u;
        }
        #debug("-----");
        assert (hm.size() == num_to_insert);
        i = 0u;
        while i < num_to_insert {
            #debug("get(%u) = %u", i, hm.get(i));
            assert (hm.get(i) == i * i);
            i += 1u;
        }
        #debug("*** finished test_removal");
    }

    #[test]
    fn test_contains_key() {
        let key = "k";
        let map = map::hashmap::<str, str>(str::hash, str::eq);
        assert (!map.contains_key(key));
        map.insert(key, "val");
        assert (map.contains_key(key));
    }

    #[test]
    fn test_find() {
        let key = "k";
        let map = map::hashmap::<str, str>(str::hash, str::eq);
        assert (option::is_none(map.find(key)));
        map.insert(key, "val");
        assert (option::get(map.find(key)) == "val");
    }

    #[test]
    fn test_hash_from_vec() {
        let map = map::hash_from_strs([
            ("a", 1),
            ("b", 2),
            ("c", 3)
        ]);
        assert map.size() == 3u;
        assert map.get("a") == 1;
        assert map.get("b") == 2;
        assert map.get("c") == 3;
    }
}
