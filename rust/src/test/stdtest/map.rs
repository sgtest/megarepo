

// -*- rust -*-
use std;
import std::map;
import std::istr;
import std::uint;
import std::util;
import std::option;

#[test]
fn test_simple() {
    log "*** starting test_simple";
    fn eq_uint(x: &uint, y: &uint) -> bool { ret x == y; }
    let hasher_uint: map::hashfn<uint> = util::id;
    let eqer_uint: map::eqfn<uint> = eq_uint;
    let hasher_str: map::hashfn<istr> = istr::hash;
    let eqer_str: map::eqfn<istr> = istr::eq;
    log "uint -> uint";
    let hm_uu: map::hashmap<uint, uint> =
        map::mk_hashmap::<uint, uint>(hasher_uint, eqer_uint);
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
    let ten: istr = ~"ten";
    let eleven: istr = ~"eleven";
    let twelve: istr = ~"twelve";
    log "str -> uint";
    let hm_su: map::hashmap<istr, uint> =
        map::mk_hashmap::<istr, uint>(hasher_str, eqer_str);
    assert (hm_su.insert(~"ten", 12u));
    assert (hm_su.insert(eleven, 13u));
    assert (hm_su.insert(~"twelve", 14u));
    assert (hm_su.get(eleven) == 13u);
    assert (hm_su.get(~"eleven") == 13u);
    assert (hm_su.get(~"twelve") == 14u);
    assert (hm_su.get(~"ten") == 12u);
    assert (!hm_su.insert(~"twelve", 14u));
    assert (hm_su.get(~"twelve") == 14u);
    assert (!hm_su.insert(~"twelve", 12u));
    assert (hm_su.get(~"twelve") == 12u);
    log "uint -> str";
    let hm_us: map::hashmap<uint, istr> =
        map::mk_hashmap::<uint, istr>(hasher_uint, eqer_uint);
    assert (hm_us.insert(10u, ~"twelve"));
    assert (hm_us.insert(11u, ~"thirteen"));
    assert (hm_us.insert(12u, ~"fourteen"));
    assert (istr::eq(hm_us.get(11u), ~"thirteen"));
    assert (istr::eq(hm_us.get(12u), ~"fourteen"));
    assert (istr::eq(hm_us.get(10u), ~"twelve"));
    assert (!hm_us.insert(12u, ~"fourteen"));
    assert (istr::eq(hm_us.get(12u), ~"fourteen"));
    assert (!hm_us.insert(12u, ~"twelve"));
    assert (istr::eq(hm_us.get(12u), ~"twelve"));
    log "str -> str";
    let hm_ss: map::hashmap<istr, istr> =
        map::mk_hashmap::<istr, istr>(hasher_str, eqer_str);
    assert (hm_ss.insert(ten, ~"twelve"));
    assert (hm_ss.insert(eleven, ~"thirteen"));
    assert (hm_ss.insert(twelve, ~"fourteen"));
    assert (istr::eq(hm_ss.get(~"eleven"), ~"thirteen"));
    assert (istr::eq(hm_ss.get(~"twelve"), ~"fourteen"));
    assert (istr::eq(hm_ss.get(~"ten"), ~"twelve"));
    assert (!hm_ss.insert(~"twelve", ~"fourteen"));
    assert (istr::eq(hm_ss.get(~"twelve"), ~"fourteen"));
    assert (!hm_ss.insert(~"twelve", ~"twelve"));
    assert (istr::eq(hm_ss.get(~"twelve"), ~"twelve"));
    log "*** finished test_simple";
}


/**
 * Force map growth and rehashing.
 */
#[test]
fn test_growth() {
    log "*** starting test_growth";
    let num_to_insert: uint = 64u;
    fn eq_uint(x: &uint, y: &uint) -> bool { ret x == y; }
    log "uint -> uint";
    let hasher_uint: map::hashfn<uint> = util::id;
    let eqer_uint: map::eqfn<uint> = eq_uint;
    let hm_uu: map::hashmap<uint, uint> =
        map::mk_hashmap::<uint, uint>(hasher_uint, eqer_uint);
    let i: uint = 0u;
    while i < num_to_insert {
        assert (hm_uu.insert(i, i * i));
        log ~"inserting " + uint::to_str(i, 10u) + ~" -> " +
                uint::to_str(i * i, 10u);
        i += 1u;
    }
    log "-----";
    i = 0u;
    while i < num_to_insert {
        log ~"get(" + uint::to_str(i, 10u) + ~") = " +
                uint::to_str(hm_uu.get(i), 10u);
        assert (hm_uu.get(i) == i * i);
        i += 1u;
    }
    assert (hm_uu.insert(num_to_insert, 17u));
    assert (hm_uu.get(num_to_insert) == 17u);
    log "-----";
    hm_uu.rehash();
    i = 0u;
    while i < num_to_insert {
        log ~"get(" + uint::to_str(i, 10u) + ~") = " +
                uint::to_str(hm_uu.get(i), 10u);
        assert (hm_uu.get(i) == i * i);
        i += 1u;
    }
    log "str -> str";
    let hasher_str: map::hashfn<istr> = istr::hash;
    let eqer_str: map::eqfn<istr> = istr::eq;
    let hm_ss: map::hashmap<istr, istr> =
        map::mk_hashmap::<istr, istr>(hasher_str, eqer_str);
    i = 0u;
    while i < num_to_insert {
        assert (hm_ss.insert(uint::to_str(i, 2u),
                             uint::to_str(i * i, 2u)));
        log ~"inserting \"" + uint::to_str(i, 2u) + ~"\" -> \"" +
                uint::to_str(i * i, 2u) + ~"\"";
        i += 1u;
    }
    log "-----";
    i = 0u;
    while i < num_to_insert {
        log ~"get(\"" + uint::to_str(i, 2u) + ~"\") = \"" +
                hm_ss.get(uint::to_str(i, 2u)) + ~"\"";
        assert (istr::eq(hm_ss.get(uint::to_str(i, 2u)),
                        uint::to_str(i * i, 2u)));
        i += 1u;
    }
    assert (hm_ss.insert(uint::to_str(num_to_insert, 2u),
                         uint::to_str(17u, 2u)));
    assert (istr::eq(hm_ss.get(
        uint::to_str(num_to_insert, 2u)),
                    uint::to_str(17u, 2u)));
    log "-----";
    hm_ss.rehash();
    i = 0u;
    while i < num_to_insert {
        log ~"get(\"" + uint::to_str(i, 2u) + ~"\") = \"" +
                hm_ss.get(uint::to_str(i, 2u)) + ~"\"";
        assert (istr::eq(hm_ss.get(uint::to_str(i, 2u)),
                        uint::to_str(i * i, 2u)));
        i += 1u;
    }
    log "*** finished test_growth";
}

#[test]
fn test_removal() {
    log "*** starting test_removal";
    let num_to_insert: uint = 64u;
    fn eq(x: &uint, y: &uint) -> bool { ret x == y; }
    fn hash(u: &uint) -> uint {
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
        map::mk_hashmap::<uint, uint>(hasher, eqer);
    let i: uint = 0u;
    while i < num_to_insert {
        assert (hm.insert(i, i * i));
        log ~"inserting " + uint::to_str(i, 10u) + ~" -> " +
                uint::to_str(i * i, 10u);
        i += 1u;
    }
    assert (hm.size() == num_to_insert);
    log "-----";
    log "removing evens";
    i = 0u;
    while i < num_to_insert {
        let v = hm.remove(i);
        alt (v) {
          option::some(u) {
            assert (u == (i * i));
          }
          option::none. { fail; }
        }
        i += 2u;
    }
    assert (hm.size() == num_to_insert / 2u);
    log "-----";
    i = 1u;
    while i < num_to_insert {
        log ~"get(" + uint::to_str(i, 10u) + ~") = " +
                uint::to_str(hm.get(i), 10u);
        assert (hm.get(i) == i * i);
        i += 2u;
    }
    log "-----";
    log "rehashing";
    hm.rehash();
    log "-----";
    i = 1u;
    while i < num_to_insert {
        log ~"get(" + uint::to_str(i, 10u) + ~") = " +
                uint::to_str(hm.get(i), 10u);
        assert (hm.get(i) == i * i);
        i += 2u;
    }
    log "-----";
    i = 0u;
    while i < num_to_insert {
        assert (hm.insert(i, i * i));
        log ~"inserting " + uint::to_str(i, 10u) + ~" -> " +
                uint::to_str(i * i, 10u);
        i += 2u;
    }
    assert (hm.size() == num_to_insert);
    log "-----";
    i = 0u;
    while i < num_to_insert {
        log ~"get(" + uint::to_str(i, 10u) + ~") = " +
                uint::to_str(hm.get(i), 10u);
        assert (hm.get(i) == i * i);
        i += 1u;
    }
    log "-----";
    log "rehashing";
    hm.rehash();
    log "-----";
    assert (hm.size() == num_to_insert);
    i = 0u;
    while i < num_to_insert {
        log ~"get(" + uint::to_str(i, 10u) + ~") = " +
                uint::to_str(hm.get(i), 10u);
        assert (hm.get(i) == i * i);
        i += 1u;
    }
    log "*** finished test_removal";
}

#[test]
fn test_contains_key() {
    let key = ~"k";
    let map = map::mk_hashmap::<istr, istr>(istr::hash, istr::eq);
    assert (!map.contains_key(key));
    map.insert(key, ~"val");
    assert (map.contains_key(key));
}

#[test]
fn test_find() {
    let key = ~"k";
    let map = map::mk_hashmap::<istr, istr>(istr::hash, istr::eq);
    assert (std::option::is_none(map.find(key)));
    map.insert(key, ~"val");
    assert (std::option::get(map.find(key)) == ~"val");
}
