// -*- rust -*-

#[doc = "Classic Boolean logic reified as ADT"];

export t;
export not, and, or, xor, implies;
export eq, ne, is_true, is_false;
export from_str, to_str, all_values, to_bit;

#[doc = "The type of boolean logic values"]
type t = bool;

#[doc(
  brief = "Negation/Inverse"
)]
pure fn not(v: t) -> t { !v }

#[doc(
  brief = "Conjunction"
)]
pure fn and(a: t, b: t) -> t { a && b }

#[doc(
  brief = "Disjunction"
)]
pure fn or(a: t, b: t) -> t { a || b }

#[doc(
  brief = "Exclusive or, i.e. `or(and(a, not(b)), and(not(a), b))`"
)]
pure fn xor(a: t, b: t) -> t { (a && !b) || (!a && b) }

#[doc(
  brief = "Implication in the logic, i.e. from `a` follows `b`"
)]
pure fn implies(a: t, b: t) -> t { !a || b }

#[doc(
  brief = "true if truth values `a` and `b` \
           are indistinguishable in the logic"
)]
pure fn eq(a: t, b: t) -> bool { a == b }

#[doc(
  brief = "true if truth values `a` and `b` are distinguishable in the logic"
)]
pure fn ne(a: t, b: t) -> bool { a != b }

#[doc(
  brief = "true if `v` represents truth in the logic"
)]
pure fn is_true(v: t) -> bool { v }

#[doc(
  brief = "true if `v` represents falsehood in the logic"
)]
pure fn is_false(v: t) -> bool { !v }

#[doc(
  brief = "Parse logic value from `s`"
)]
pure fn from_str(s: str) -> t {
    alt s {
      "true" { true }
      "false" { false }
    }
}

#[doc(
  brief = "Convert `v` into a string"
)]
pure fn to_str(v: t) -> str { if v { "true" } else { "false" } }

#[doc(
  brief = "Iterates over all truth values by passing them to `blk` \
           in an unspecified order"
)]
fn all_values(blk: fn(v: t)) {
    blk(true);
    blk(false);
}

#[doc(
  brief = "converts truth value to an 8 bit byte"
)]
pure fn to_bit(v: t) -> u8 { if v { 1u8 } else { 0u8 } }

#[test]
fn test_bool_from_str() {
    all_values { |v|
        assert v == from_str(bool::to_str(v))
    }
}

#[test]
fn test_bool_to_str() {
    assert to_str(false) == "false";
    assert to_str(true) == "true";
}

#[test]
fn test_bool_to_bit() {
    all_values { |v|
        assert to_bit(v) == if is_true(v) { 1u8 } else { 0u8 };
    }
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
