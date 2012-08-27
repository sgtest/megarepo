// Rust JSON serialization library
// Copyright (c) 2011 Google Inc.

//! json serialization

import result::{Result, Ok, Err};
import io;
import io::WriterUtil;
import map;
import map::hashmap;
import map::map;

export json;
export error;
export to_writer;
export to_str;
export from_reader;
export from_str;
export eq;
export to_json;

export num;
export string;
export boolean;
export list;
export dict;
export null;

/// Represents a json value
enum json {
    num(float),
    string(@~str),
    boolean(bool),
    list(@~[json]),
    dict(map::hashmap<~str, json>),
    null,
}

type error = {
    line: uint,
    col: uint,
    msg: @~str,
};

/// Serializes a json value into a io::writer
fn to_writer(wr: io::Writer, j: json) {
    match j {
      num(n) => wr.write_str(float::to_str(n, 6u)),
      string(s) => wr.write_str(escape_str(*s)),
      boolean(b) => wr.write_str(if b { ~"true" } else { ~"false" }),
      list(v) => {
        wr.write_char('[');
        let mut first = true;
        for (*v).each |item| {
            if !first {
                wr.write_str(~", ");
            }
            first = false;
            to_writer(wr, item);
        };
        wr.write_char(']');
      }
      dict(d) => {
        if d.size() == 0u {
            wr.write_str(~"{}");
            return;
        }

        wr.write_str(~"{ ");
        let mut first = true;
        for d.each |key, value| {
            if !first {
                wr.write_str(~", ");
            }
            first = false;
            wr.write_str(escape_str(key));
            wr.write_str(~": ");
            to_writer(wr, value);
        };
        wr.write_str(~" }");
      }
      null => wr.write_str(~"null")
    }
}

fn escape_str(s: ~str) -> ~str {
    let mut escaped = ~"\"";
    do str::chars_iter(s) |c| {
        match c {
          '"' => escaped += ~"\\\"",
          '\\' => escaped += ~"\\\\",
          '\x08' => escaped += ~"\\b",
          '\x0c' => escaped += ~"\\f",
          '\n' => escaped += ~"\\n",
          '\r' => escaped += ~"\\r",
          '\t' => escaped += ~"\\t",
          _ => escaped += str::from_char(c)
        }
    };

    escaped += ~"\"";

    escaped
}

/// Serializes a json value into a string
fn to_str(j: json) -> ~str {
    io::with_str_writer(|wr| to_writer(wr, j))
}

type parser_ = {
    rdr: io::Reader,
    mut ch: char,
    mut line: uint,
    mut col: uint,
};

enum parser {
    parser_(parser_)
}

impl parser {
    fn eof() -> bool { self.ch == -1 as char }

    fn bump() {
        self.ch = self.rdr.read_char();

        if self.ch == '\n' {
            self.line += 1u;
            self.col = 1u;
        } else {
            self.col += 1u;
        }
    }

    fn next_char() -> char {
        self.bump();
        self.ch
    }

    fn error<T>(+msg: ~str) -> Result<T, error> {
        Err({ line: self.line, col: self.col, msg: @msg })
    }

    fn parse() -> Result<json, error> {
        match self.parse_value() {
          Ok(value) => {
            // Skip trailing whitespaces.
            self.parse_whitespace();
            // Make sure there is no trailing characters.
            if self.eof() {
                Ok(value)
            } else {
                self.error(~"trailing characters")
            }
          }
          e => e
        }
    }

    fn parse_value() -> Result<json, error> {
        self.parse_whitespace();

        if self.eof() { return self.error(~"EOF while parsing value"); }

        match self.ch {
          'n' => self.parse_ident(~"ull", null),
          't' => self.parse_ident(~"rue", boolean(true)),
          'f' => self.parse_ident(~"alse", boolean(false)),
          '0' to '9' | '-' => self.parse_number(),
          '"' => match self.parse_str() {
            Ok(s) => Ok(string(s)),
            Err(e) => Err(e)
          },
          '[' => self.parse_list(),
          '{' => self.parse_object(),
          _ => self.error(~"invalid syntax")
        }
    }

    fn parse_whitespace() {
        while char::is_whitespace(self.ch) { self.bump(); }
    }

    fn parse_ident(ident: ~str, value: json) -> Result<json, error> {
        if str::all(ident, |c| c == self.next_char()) {
            self.bump();
            Ok(value)
        } else {
            self.error(~"invalid syntax")
        }
    }

    fn parse_number() -> Result<json, error> {
        let mut neg = 1f;

        if self.ch == '-' {
            self.bump();
            neg = -1f;
        }

        let mut res = match self.parse_integer() {
          Ok(res) => res,
          Err(e) => return Err(e)
        };

        if self.ch == '.' {
            match self.parse_decimal(res) {
              Ok(r) => res = r,
              Err(e) => return Err(e)
            }
        }

        if self.ch == 'e' || self.ch == 'E' {
            match self.parse_exponent(res) {
              Ok(r) => res = r,
              Err(e) => return Err(e)
            }
        }

        Ok(num(neg * res))
    }

    fn parse_integer() -> Result<float, error> {
        let mut res = 0f;

        match self.ch {
          '0' => {
            self.bump();

            // There can be only one leading '0'.
            match self.ch {
              '0' to '9' => return self.error(~"invalid number"),
              _ => ()
            }
          }
          '1' to '9' => {
            while !self.eof() {
                match self.ch {
                  '0' to '9' => {
                    res *= 10f;
                    res += ((self.ch as int) - ('0' as int)) as float;

                    self.bump();
                  }
                  _ => break
                }
            }
          }
          _ => return self.error(~"invalid number")
        }

        Ok(res)
    }

    fn parse_decimal(res: float) -> Result<float, error> {
        self.bump();

        // Make sure a digit follows the decimal place.
        match self.ch {
          '0' to '9' => (),
          _ => return self.error(~"invalid number")
        }

        let mut res = res;
        let mut dec = 1f;
        while !self.eof() {
            match self.ch {
              '0' to '9' => {
                dec /= 10f;
                res += (((self.ch as int) - ('0' as int)) as float) * dec;

                self.bump();
              }
              _ => break
            }
        }

        Ok(res)
    }

    fn parse_exponent(res: float) -> Result<float, error> {
        self.bump();

        let mut res = res;
        let mut exp = 0u;
        let mut neg_exp = false;

        match self.ch {
          '+' => self.bump(),
          '-' => { self.bump(); neg_exp = true; }
          _ => ()
        }

        // Make sure a digit follows the exponent place.
        match self.ch {
          '0' to '9' => (),
          _ => return self.error(~"invalid number")
        }

        while !self.eof() {
            match self.ch {
              '0' to '9' => {
                exp *= 10u;
                exp += (self.ch as uint) - ('0' as uint);

                self.bump();
              }
              _ => break
            }
        }

        let exp = float::pow_with_uint(10u, exp);
        if neg_exp {
            res /= exp;
        } else {
            res *= exp;
        }

        Ok(res)
    }

    fn parse_str() -> Result<@~str, error> {
        let mut escape = false;
        let mut res = ~"";

        while !self.eof() {
            self.bump();

            if (escape) {
                match self.ch {
                  '"' => str::push_char(res, '"'),
                  '\\' => str::push_char(res, '\\'),
                  '/' => str::push_char(res, '/'),
                  'b' => str::push_char(res, '\x08'),
                  'f' => str::push_char(res, '\x0c'),
                  'n' => str::push_char(res, '\n'),
                  'r' => str::push_char(res, '\r'),
                  't' => str::push_char(res, '\t'),
                  'u' => {
                      // Parse \u1234.
                      let mut i = 0u;
                      let mut n = 0u;
                      while i < 4u {
                          match self.next_char() {
                            '0' to '9' => {
                              n = n * 10u +
                                  (self.ch as uint) - ('0' as uint);
                            }
                            _ => return self.error(~"invalid \\u escape")
                          }
                          i += 1u;
                      }

                      // Error out if we didn't parse 4 digits.
                      if i != 4u {
                          return self.error(~"invalid \\u escape");
                      }

                      str::push_char(res, n as char);
                  }
                  _ => return self.error(~"invalid escape")
                }
                escape = false;
            } else if self.ch == '\\' {
                escape = true;
            } else {
                if self.ch == '"' {
                    self.bump();
                    return Ok(@res);
                }
                str::push_char(res, self.ch);
            }
        }

        self.error(~"EOF while parsing string")
    }

    fn parse_list() -> Result<json, error> {
        self.bump();
        self.parse_whitespace();

        let mut values = ~[];

        if self.ch == ']' {
            self.bump();
            return Ok(list(@values));
        }

        loop {
            match self.parse_value() {
              Ok(v) => vec::push(values, v),
              e => return e
            }

            self.parse_whitespace();
            if self.eof() {
                return self.error(~"EOF while parsing list");
            }

            match self.ch {
              ',' => self.bump(),
              ']' => { self.bump(); return Ok(list(@values)); }
              _ => return self.error(~"expected `,` or `]`")
            }
        };
    }

    fn parse_object() -> Result<json, error> {
        self.bump();
        self.parse_whitespace();

        let values = map::str_hash();

        if self.ch == '}' {
          self.bump();
          return Ok(dict(values));
        }

        while !self.eof() {
            self.parse_whitespace();

            if self.ch != '"' {
                return self.error(~"key must be a string");
            }

            let key = match self.parse_str() {
              Ok(key) => key,
              Err(e) => return Err(e)
            };

            self.parse_whitespace();

            if self.ch != ':' {
                if self.eof() { break; }
                return self.error(~"expected `:`");
            }
            self.bump();

            match self.parse_value() {
              Ok(value) => { values.insert(copy *key, value); }
              e => return e
            }
            self.parse_whitespace();

            match self.ch {
              ',' => self.bump(),
              '}' => { self.bump(); return Ok(dict(values)); }
              _ => {
                  if self.eof() { break; }
                  return self.error(~"expected `,` or `}`");
              }
            }
        }

        return self.error(~"EOF while parsing object");
    }
}

/// Deserializes a json value from an io::reader
fn from_reader(rdr: io::Reader) -> Result<json, error> {
    let parser = parser_({
        rdr: rdr,
        mut ch: rdr.read_char(),
        mut line: 1u,
        mut col: 1u,
    });

    parser.parse()
}

/// Deserializes a json value from a string
fn from_str(s: ~str) -> Result<json, error> {
    io::with_str_reader(s, from_reader)
}

/// Test if two json values are equal
fn eq(value0: json, value1: json) -> bool {
    match (value0, value1) {
      (num(f0), num(f1)) => f0 == f1,
      (string(s0), string(s1)) => s0 == s1,
      (boolean(b0), boolean(b1)) => b0 == b1,
      (list(l0), list(l1)) => vec::all2(*l0, *l1, eq),
      (dict(d0), dict(d1)) => {
          if d0.size() == d1.size() {
              let mut equal = true;
              for d0.each |k, v0| {
                  match d1.find(k) {
                    Some(v1) => if !eq(v0, v1) { equal = false },
                    None => equal = false
                  }
              };
              equal
          } else {
              false
          }
      }
      (null, null) => true,
      _ => false
    }
}

trait to_json { fn to_json() -> json; }

impl json: to_json {
    fn to_json() -> json { self }
}

impl @json: to_json {
    fn to_json() -> json { *self }
}

impl int: to_json {
    fn to_json() -> json { num(self as float) }
}

impl i8: to_json {
    fn to_json() -> json { num(self as float) }
}

impl i16: to_json {
    fn to_json() -> json { num(self as float) }
}

impl i32: to_json {
    fn to_json() -> json { num(self as float) }
}

impl i64: to_json {
    fn to_json() -> json { num(self as float) }
}

impl uint: to_json {
    fn to_json() -> json { num(self as float) }
}

impl u8: to_json {
    fn to_json() -> json { num(self as float) }
}

impl u16: to_json {
    fn to_json() -> json { num(self as float) }
}

impl u32: to_json {
    fn to_json() -> json { num(self as float) }
}

impl u64: to_json {
    fn to_json() -> json { num(self as float) }
}

impl float: to_json {
    fn to_json() -> json { num(self) }
}

impl f32: to_json {
    fn to_json() -> json { num(self as float) }
}

impl f64: to_json {
    fn to_json() -> json { num(self as float) }
}

impl (): to_json {
    fn to_json() -> json { null }
}

impl bool: to_json {
    fn to_json() -> json { boolean(self) }
}

impl ~str: to_json {
    fn to_json() -> json { string(@copy self) }
}

impl @~str: to_json {
    fn to_json() -> json { string(self) }
}

impl <A: to_json, B: to_json> (A, B): to_json {
    fn to_json() -> json {
        match self {
          (a, b) => {
            list(@~[a.to_json(), b.to_json()])
          }
        }
    }
}

impl <A: to_json, B: to_json, C: to_json> (A, B, C): to_json {

    fn to_json() -> json {
        match self {
          (a, b, c) => {
            list(@~[a.to_json(), b.to_json(), c.to_json()])
          }
        }
    }
}

impl <A: to_json> ~[A]: to_json {
    fn to_json() -> json { list(@self.map(|elt| elt.to_json())) }
}

impl <A: to_json copy> hashmap<~str, A>: to_json {
    fn to_json() -> json {
        let d = map::str_hash();
        for self.each() |key, value| {
            d.insert(copy key, value.to_json());
        }
        dict(d)
    }
}

impl <A: to_json> Option<A>: to_json {
    fn to_json() -> json {
        match self {
          None => null,
          Some(value) => value.to_json()
        }
    }
}

impl json: to_str::ToStr {
    fn to_str() -> ~str { to_str(self) }
}

impl error: to_str::ToStr {
    fn to_str() -> ~str {
        fmt!("%u:%u: %s", self.line, self.col, *self.msg)
    }
}

#[cfg(test)]
mod tests {
    fn mk_dict(items: ~[(~str, json)]) -> json {
        let d = map::str_hash();

        do vec::iter(items) |item| {
            let (key, value) = copy item;
            d.insert(key, value);
        };

        dict(d)
    }

    #[test]
    fn test_write_null() {
        assert to_str(null) == ~"null";
    }

    #[test]
    fn test_write_num() {
        assert to_str(num(3f)) == ~"3";
        assert to_str(num(3.1f)) == ~"3.1";
        assert to_str(num(-1.5f)) == ~"-1.5";
        assert to_str(num(0.5f)) == ~"0.5";
    }

    #[test]
    fn test_write_str() {
        assert to_str(string(@~"")) == ~"\"\"";
        assert to_str(string(@~"foo")) == ~"\"foo\"";
    }

    #[test]
    fn test_write_bool() {
        assert to_str(boolean(true)) == ~"true";
        assert to_str(boolean(false)) == ~"false";
    }

    #[test]
    fn test_write_list() {
        assert to_str(list(@~[])) == ~"[]";
        assert to_str(list(@~[boolean(true)])) == ~"[true]";
        assert to_str(list(@~[
            boolean(false),
            null,
            list(@~[string(@~"foo\nbar"), num(3.5f)])
        ])) == ~"[false, null, [\"foo\\nbar\", 3.5]]";
    }

    #[test]
    fn test_write_dict() {
        assert to_str(mk_dict(~[])) == ~"{}";
        assert to_str(mk_dict(~[(~"a", boolean(true))]))
            == ~"{ \"a\": true }";
        assert to_str(mk_dict(~[
            (~"a", boolean(true)),
            (~"b", list(@~[
                mk_dict(~[(~"c", string(@~"\x0c\r"))]),
                mk_dict(~[(~"d", string(@~""))])
            ]))
        ])) ==
            ~"{ " +
                ~"\"a\": true, " +
                ~"\"b\": [" +
                    ~"{ \"c\": \"\\f\\r\" }, " +
                    ~"{ \"d\": \"\" }" +
                ~"]" +
            ~" }";
    }

    #[test]
    fn test_trailing_characters() {
        assert from_str(~"nulla") ==
            Err({line: 1u, col: 5u, msg: @~"trailing characters"});
        assert from_str(~"truea") ==
            Err({line: 1u, col: 5u, msg: @~"trailing characters"});
        assert from_str(~"falsea") ==
            Err({line: 1u, col: 6u, msg: @~"trailing characters"});
        assert from_str(~"1a") ==
            Err({line: 1u, col: 2u, msg: @~"trailing characters"});
        assert from_str(~"[]a") ==
            Err({line: 1u, col: 3u, msg: @~"trailing characters"});
        assert from_str(~"{}a") ==
            Err({line: 1u, col: 3u, msg: @~"trailing characters"});
    }

    #[test]
    fn test_read_identifiers() {
        assert from_str(~"n") ==
            Err({line: 1u, col: 2u, msg: @~"invalid syntax"});
        assert from_str(~"nul") ==
            Err({line: 1u, col: 4u, msg: @~"invalid syntax"});

        assert from_str(~"t") ==
            Err({line: 1u, col: 2u, msg: @~"invalid syntax"});
        assert from_str(~"truz") ==
            Err({line: 1u, col: 4u, msg: @~"invalid syntax"});

        assert from_str(~"f") ==
            Err({line: 1u, col: 2u, msg: @~"invalid syntax"});
        assert from_str(~"faz") ==
            Err({line: 1u, col: 3u, msg: @~"invalid syntax"});

        assert from_str(~"null") == Ok(null);
        assert from_str(~"true") == Ok(boolean(true));
        assert from_str(~"false") == Ok(boolean(false));
        assert from_str(~" null ") == Ok(null);
        assert from_str(~" true ") == Ok(boolean(true));
        assert from_str(~" false ") == Ok(boolean(false));
    }

    #[test]
    fn test_read_num() {
        assert from_str(~"+") ==
            Err({line: 1u, col: 1u, msg: @~"invalid syntax"});
        assert from_str(~".") ==
            Err({line: 1u, col: 1u, msg: @~"invalid syntax"});

        assert from_str(~"-") ==
            Err({line: 1u, col: 2u, msg: @~"invalid number"});
        assert from_str(~"00") ==
            Err({line: 1u, col: 2u, msg: @~"invalid number"});
        assert from_str(~"1.") ==
            Err({line: 1u, col: 3u, msg: @~"invalid number"});
        assert from_str(~"1e") ==
            Err({line: 1u, col: 3u, msg: @~"invalid number"});
        assert from_str(~"1e+") ==
            Err({line: 1u, col: 4u, msg: @~"invalid number"});

        assert from_str(~"3") == Ok(num(3f));
        assert from_str(~"3.1") == Ok(num(3.1f));
        assert from_str(~"-1.2") == Ok(num(-1.2f));
        assert from_str(~"0.4") == Ok(num(0.4f));
        assert from_str(~"0.4e5") == Ok(num(0.4e5f));
        assert from_str(~"0.4e+15") == Ok(num(0.4e15f));
        assert from_str(~"0.4e-01") == Ok(num(0.4e-01f));
        assert from_str(~" 3 ") == Ok(num(3f));
    }

    #[test]
    fn test_read_str() {
        assert from_str(~"\"") ==
            Err({line: 1u, col: 2u, msg: @~"EOF while parsing string"});
        assert from_str(~"\"lol") ==
            Err({line: 1u, col: 5u, msg: @~"EOF while parsing string"});

        assert from_str(~"\"\"") == Ok(string(@~""));
        assert from_str(~"\"foo\"") == Ok(string(@~"foo"));
        assert from_str(~"\"\\\"\"") == Ok(string(@~"\""));
        assert from_str(~"\"\\b\"") == Ok(string(@~"\x08"));
        assert from_str(~"\"\\n\"") == Ok(string(@~"\n"));
        assert from_str(~"\"\\r\"") == Ok(string(@~"\r"));
        assert from_str(~"\"\\t\"") == Ok(string(@~"\t"));
        assert from_str(~" \"foo\" ") == Ok(string(@~"foo"));
    }

    #[test]
    fn test_read_list() {
        assert from_str(~"[") ==
            Err({line: 1u, col: 2u, msg: @~"EOF while parsing value"});
        assert from_str(~"[1") ==
            Err({line: 1u, col: 3u, msg: @~"EOF while parsing list"});
        assert from_str(~"[1,") ==
            Err({line: 1u, col: 4u, msg: @~"EOF while parsing value"});
        assert from_str(~"[1,]") ==
            Err({line: 1u, col: 4u, msg: @~"invalid syntax"});
        assert from_str(~"[6 7]") ==
            Err({line: 1u, col: 4u, msg: @~"expected `,` or `]`"});

        assert from_str(~"[]") == Ok(list(@~[]));
        assert from_str(~"[ ]") == Ok(list(@~[]));
        assert from_str(~"[true]") == Ok(list(@~[boolean(true)]));
        assert from_str(~"[ false ]") == Ok(list(@~[boolean(false)]));
        assert from_str(~"[null]") == Ok(list(@~[null]));
        assert from_str(~"[3, 1]") == Ok(list(@~[num(3f), num(1f)]));
        assert from_str(~"\n[3, 2]\n") == Ok(list(@~[num(3f), num(2f)]));
        assert from_str(~"[2, [4, 1]]") ==
               Ok(list(@~[num(2f), list(@~[num(4f), num(1f)])]));
    }

    #[test]
    fn test_read_dict() {
        assert from_str(~"{") ==
            Err({line: 1u, col: 2u, msg: @~"EOF while parsing object"});
        assert from_str(~"{ ") ==
            Err({line: 1u, col: 3u, msg: @~"EOF while parsing object"});
        assert from_str(~"{1") ==
            Err({line: 1u, col: 2u, msg: @~"key must be a string"});
        assert from_str(~"{ \"a\"") ==
            Err({line: 1u, col: 6u, msg: @~"EOF while parsing object"});
        assert from_str(~"{\"a\"") ==
            Err({line: 1u, col: 5u, msg: @~"EOF while parsing object"});
        assert from_str(~"{\"a\" ") ==
            Err({line: 1u, col: 6u, msg: @~"EOF while parsing object"});

        assert from_str(~"{\"a\" 1") ==
            Err({line: 1u, col: 6u, msg: @~"expected `:`"});
        assert from_str(~"{\"a\":") ==
            Err({line: 1u, col: 6u, msg: @~"EOF while parsing value"});
        assert from_str(~"{\"a\":1") ==
            Err({line: 1u, col: 7u, msg: @~"EOF while parsing object"});
        assert from_str(~"{\"a\":1 1") ==
            Err({line: 1u, col: 8u, msg: @~"expected `,` or `}`"});
        assert from_str(~"{\"a\":1,") ==
            Err({line: 1u, col: 8u, msg: @~"EOF while parsing object"});

        assert eq(result::get(from_str(~"{}")), mk_dict(~[]));
        assert eq(result::get(from_str(~"{\"a\": 3}")),
                  mk_dict(~[(~"a", num(3.0f))]));

        assert eq(result::get(from_str(~"{ \"a\": null, \"b\" : true }")),
                  mk_dict(~[
                      (~"a", null),
                      (~"b", boolean(true))]));
        assert eq(result::get(from_str(~"\n{ \"a\": null, \"b\" : true }\n")),
                  mk_dict(~[
                      (~"a", null),
                      (~"b", boolean(true))]));
        assert eq(result::get(from_str(~"{\"a\" : 1.0 ,\"b\": [ true ]}")),
                  mk_dict(~[
                      (~"a", num(1.0)),
                      (~"b", list(@~[boolean(true)]))
                  ]));
        assert eq(result::get(from_str(
                      ~"{" +
                          ~"\"a\": 1.0, " +
                          ~"\"b\": [" +
                              ~"true," +
                              ~"\"foo\\nbar\", " +
                              ~"{ \"c\": {\"d\": null} } " +
                          ~"]" +
                      ~"}")),
                  mk_dict(~[
                      (~"a", num(1.0f)),
                      (~"b", list(@~[
                          boolean(true),
                          string(@~"foo\nbar"),
                          mk_dict(~[
                              (~"c", mk_dict(~[(~"d", null)]))
                          ])
                      ]))
                  ]));
    }

    #[test]
    fn test_multiline_errors() {
        assert from_str(~"{\n  \"foo\":\n \"bar\"") ==
            Err({line: 3u, col: 8u, msg: @~"EOF while parsing object"});
    }
}
