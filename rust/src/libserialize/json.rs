// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Rust JSON serialization library
// Copyright (c) 2011 Google Inc.

#![forbid(non_camel_case_types)]
#![allow(missing_doc)]

/*!
JSON parsing and serialization

# What is JSON?

JSON (JavaScript Object Notation) is a way to write data in Javascript.
Like XML it allows one to encode structured data in a text format that can be read by humans easily.
Its native compatibility with JavaScript and its simple syntax make it used widely.

Json data are encoded in a form of "key":"value".
Data types that can be encoded are JavaScript types :
boolean (`true` or `false`), number (`f64`), string, array, object, null.
An object is a series of string keys mapping to values, in `"key": value` format.
Arrays are enclosed in square brackets ([ ... ]) and objects in curly brackets ({ ... }).
A simple JSON document encoding a person, his/her age, address and phone numbers could look like:

```ignore
{
    "FirstName": "John",
    "LastName": "Doe",
    "Age": 43,
    "Address": {
        "Street": "Downing Street 10",
        "City": "London",
        "Country": "Great Britain"
    },
    "PhoneNumbers": [
        "+44 1234567",
        "+44 2345678"
    ]
}
```

# Rust Type-based Encoding and Decoding

Rust provides a mechanism for low boilerplate encoding & decoding
of values to and from JSON via the serialization API.
To be able to encode a piece of data, it must implement the `serialize::Encodable` trait.
To be able to decode a piece of data, it must implement the `serialize::Decodable` trait.
The Rust compiler provides an annotation to automatically generate
the code for these traits: `#[deriving(Decodable, Encodable)]`

To encode using Encodable :

```rust
use std::io;
use serialize::{json, Encodable};

 #[deriving(Encodable)]
 pub struct TestStruct   {
    data_str: ~str,
 }

fn main() {
    let to_encode_object = TestStruct{data_str:~"example of string to encode"};
    let mut m = io::MemWriter::new();
    {
        let mut encoder = json::Encoder::new(&mut m as &mut std::io::Writer);
        match to_encode_object.encode(&mut encoder) {
            Ok(()) => (),
            Err(e) => fail!("json encoding error: {}", e)
        };
    }
}
```

Two wrapper functions are provided to encode a Encodable object
into a string (~str) or buffer (~[u8]): `str_encode(&m)` and `buffer_encode(&m)`.

```rust
use serialize::json;
let to_encode_object = ~"example of string to encode";
let encoded_str: ~str = json::Encoder::str_encode(&to_encode_object);
```

JSON API provide an enum `json::Json` and a trait `ToJson` to encode object.
The trait `ToJson` encode object into a container `json::Json` and the API provide writer
to encode them into a stream or a string ...

When using `ToJson` the `Encodable` trait implementation is not mandatory.

A basic `ToJson` example using a TreeMap of attribute name / attribute value:


```rust
extern crate collections;
extern crate serialize;

use serialize::json;
use serialize::json::ToJson;
use collections::TreeMap;

pub struct MyStruct  {
    attr1: u8,
    attr2: ~str,
}

impl ToJson for MyStruct {
    fn to_json( &self ) -> json::Json {
        let mut d = ~TreeMap::new();
        d.insert(~"attr1", self.attr1.to_json());
        d.insert(~"attr2", self.attr2.to_json());
        json::Object(d)
    }
}

fn main() {
    let test2: MyStruct = MyStruct {attr1: 1, attr2:~"test"};
    let tjson: json::Json = test2.to_json();
    let json_str: ~str = tjson.to_str();
}
```

To decode a JSON string using `Decodable` trait :

```rust
extern crate serialize;
use serialize::{json, Decodable};

#[deriving(Decodable)]
pub struct MyStruct  {
     attr1: u8,
     attr2: ~str,
}

fn main() {
    let json_str_to_decode: ~str =
            ~"{\"attr1\":1,\"attr2\":\"toto\"}";
    let json_object = json::from_str(json_str_to_decode);
    let mut decoder = json::Decoder::new(json_object.unwrap());
    let decoded_object: MyStruct = match Decodable::decode(&mut decoder) {
        Ok(v) => v,
        Err(e) => fail!("Decoding error: {}", e)
    }; // create the final object
}
```

# Examples of use

## Using Autoserialization

Create a struct called TestStruct1 and serialize and deserialize it to and from JSON
using the serialization API, using the derived serialization code.

```rust
extern crate serialize;
use serialize::{json, Encodable, Decodable};

 #[deriving(Decodable, Encodable)] //generate Decodable, Encodable impl.
 pub struct TestStruct1  {
    data_int: u8,
    data_str: ~str,
    data_vector: ~[u8],
 }

// To serialize use the `json::str_encode` to encode an object in a string.
// It calls the generated `Encodable` impl.
fn main() {
    let to_encode_object = TestStruct1
         {data_int: 1, data_str:~"toto", data_vector:~[2,3,4,5]};
    let encoded_str: ~str = json::Encoder::str_encode(&to_encode_object);

    // To deserialize use the `json::from_str` and `json::Decoder`

    let json_object = json::from_str(encoded_str);
    let mut decoder = json::Decoder::new(json_object.unwrap());
    let decoded1: TestStruct1 = Decodable::decode(&mut decoder).unwrap(); // create the final object
}
```

## Using `ToJson`

This example use the ToJson impl to deserialize the JSON string.
Example of `ToJson` trait implementation for TestStruct1.

```rust
extern crate serialize;
extern crate collections;

use serialize::json::ToJson;
use serialize::{json, Encodable, Decodable};
use collections::TreeMap;

#[deriving(Decodable, Encodable)] // generate Decodable, Encodable impl.
pub struct TestStruct1  {
    data_int: u8,
    data_str: ~str,
    data_vector: ~[u8],
}

impl ToJson for TestStruct1 {
    fn to_json( &self ) -> json::Json {
        let mut d = ~TreeMap::new();
        d.insert(~"data_int", self.data_int.to_json());
        d.insert(~"data_str", self.data_str.to_json());
        d.insert(~"data_vector", self.data_vector.to_json());
        json::Object(d)
    }
}

fn main() {
    // Serialization using our impl of to_json

    let test2: TestStruct1 = TestStruct1 {data_int: 1, data_str:~"toto", data_vector:~[2,3,4,5]};
    let tjson: json::Json = test2.to_json();
    let json_str: ~str = tjson.to_str();

    // Deserialize like before.

    let mut decoder = json::Decoder::new(json::from_str(json_str).unwrap());
    // create the final object
    let decoded2: TestStruct1 = Decodable::decode(&mut decoder).unwrap();
}
```

*/

use collections::HashMap;
use std::char;
use std::f64;
use std::fmt;
use std::io::MemWriter;
use std::io;
use std::num;
use std::str;
use std::strbuf::StrBuf;

use Encodable;
use collections::TreeMap;

/// Represents a json value
#[deriving(Clone, Eq)]
pub enum Json {
    Number(f64),
    String(~str),
    Boolean(bool),
    List(List),
    Object(~Object),
    Null,
}

pub type List = ~[Json];
pub type Object = TreeMap<~str, Json>;

#[deriving(Eq, Show)]
pub enum Error {
    /// msg, line, col
    ParseError(~str, uint, uint),
    ExpectedError(~str, ~str),
    MissingFieldError(~str),
    UnknownVariantError(~str),
    IoError(io::IoError)
}

pub type EncodeResult = io::IoResult<()>;
pub type DecodeResult<T> = Result<T, Error>;

fn escape_str(s: &str) -> ~str {
    let mut escaped = StrBuf::from_str("\"");
    for c in s.chars() {
        match c {
          '"' => escaped.push_str("\\\""),
          '\\' => escaped.push_str("\\\\"),
          '\x08' => escaped.push_str("\\b"),
          '\x0c' => escaped.push_str("\\f"),
          '\n' => escaped.push_str("\\n"),
          '\r' => escaped.push_str("\\r"),
          '\t' => escaped.push_str("\\t"),
          _ => escaped.push_char(c),
        }
    };
    escaped.push_char('"');
    escaped.into_owned()
}

fn spaces(n: uint) -> ~str {
    let mut ss = StrBuf::new();
    for _ in range(0, n) {
        ss.push_str(" ");
    }
    return ss.into_owned();
}

/// A structure for implementing serialization to JSON.
pub struct Encoder<'a> {
    wr: &'a mut io::Writer,
}

impl<'a> Encoder<'a> {
    /// Creates a new JSON encoder whose output will be written to the writer
    /// specified.
    pub fn new<'a>(wr: &'a mut io::Writer) -> Encoder<'a> {
        Encoder { wr: wr }
    }

    /// Encode the specified struct into a json [u8]
    pub fn buffer_encode<T:Encodable<Encoder<'a>, io::IoError>>(to_encode_object: &T) -> Vec<u8>  {
       //Serialize the object in a string using a writer
        let mut m = MemWriter::new();
        {
            let mut encoder = Encoder::new(&mut m as &mut io::Writer);
            // MemWriter never Errs
            let _ = to_encode_object.encode(&mut encoder);
        }
        m.unwrap()
    }

    /// Encode the specified struct into a json str
    pub fn str_encode<T:Encodable<Encoder<'a>, io::IoError>>(to_encode_object: &T) -> ~str  {
        let buff = Encoder::buffer_encode(to_encode_object);
        str::from_utf8(buff.as_slice()).unwrap().to_owned()
    }
}

impl<'a> ::Encoder<io::IoError> for Encoder<'a> {
    fn emit_nil(&mut self) -> EncodeResult { write!(self.wr, "null") }

    fn emit_uint(&mut self, v: uint) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u64(&mut self, v: u64) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u32(&mut self, v: u32) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u16(&mut self, v: u16) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u8(&mut self, v: u8) -> EncodeResult  { self.emit_f64(v as f64) }

    fn emit_int(&mut self, v: int) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i64(&mut self, v: i64) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i32(&mut self, v: i32) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i16(&mut self, v: i16) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i8(&mut self, v: i8) -> EncodeResult  { self.emit_f64(v as f64) }

    fn emit_bool(&mut self, v: bool) -> EncodeResult {
        if v {
            write!(self.wr, "true")
        } else {
            write!(self.wr, "false")
        }
    }

    fn emit_f64(&mut self, v: f64) -> EncodeResult {
        write!(self.wr, "{}", f64::to_str_digits(v, 6u))
    }
    fn emit_f32(&mut self, v: f32) -> EncodeResult { self.emit_f64(v as f64) }

    fn emit_char(&mut self, v: char) -> EncodeResult { self.emit_str(str::from_char(v)) }
    fn emit_str(&mut self, v: &str) -> EncodeResult {
        write!(self.wr, "{}", escape_str(v))
    }

    fn emit_enum(&mut self,
                 _name: &str,
                 f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult { f(self) }

    fn emit_enum_variant(&mut self,
                         name: &str,
                         _id: uint,
                         cnt: uint,
                         f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        // enums are encoded as strings or objects
        // Bunny => "Bunny"
        // Kangaroo(34,"William") => {"variant": "Kangaroo", "fields": [34,"William"]}
        if cnt == 0 {
            write!(self.wr, "{}", escape_str(name))
        } else {
            try!(write!(self.wr, "\\{\"variant\":"));
            try!(write!(self.wr, "{}", escape_str(name)));
            try!(write!(self.wr, ",\"fields\":["));
            try!(f(self));
            write!(self.wr, "]\\}")
        }
    }

    fn emit_enum_variant_arg(&mut self,
                             idx: uint,
                             f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        if idx != 0 {
            try!(write!(self.wr, ","));
        }
        f(self)
    }

    fn emit_enum_struct_variant(&mut self,
                                name: &str,
                                id: uint,
                                cnt: uint,
                                f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_enum_variant(name, id, cnt, f)
    }

    fn emit_enum_struct_variant_field(&mut self,
                                      _: &str,
                                      idx: uint,
                                      f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_enum_variant_arg(idx, f)
    }

    fn emit_struct(&mut self,
                   _: &str,
                   _: uint,
                   f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        try!(write!(self.wr, r"\{"));
        try!(f(self));
        write!(self.wr, r"\}")
    }

    fn emit_struct_field(&mut self,
                         name: &str,
                         idx: uint,
                         f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        if idx != 0 { try!(write!(self.wr, ",")); }
        try!(write!(self.wr, "{}:", escape_str(name)));
        f(self)
    }

    fn emit_tuple(&mut self, len: uint, f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq(len, f)
    }
    fn emit_tuple_arg(&mut self,
                      idx: uint,
                      f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq_elt(idx, f)
    }

    fn emit_tuple_struct(&mut self,
                         _name: &str,
                         len: uint,
                         f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq(len, f)
    }
    fn emit_tuple_struct_arg(&mut self,
                             idx: uint,
                             f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq_elt(idx, f)
    }

    fn emit_option(&mut self, f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        f(self)
    }
    fn emit_option_none(&mut self) -> EncodeResult { self.emit_nil() }
    fn emit_option_some(&mut self, f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        f(self)
    }

    fn emit_seq(&mut self, _len: uint, f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        try!(write!(self.wr, "["));
        try!(f(self));
        write!(self.wr, "]")
    }

    fn emit_seq_elt(&mut self, idx: uint, f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        if idx != 0 {
            try!(write!(self.wr, ","));
        }
        f(self)
    }

    fn emit_map(&mut self, _len: uint, f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        try!(write!(self.wr, r"\{"));
        try!(f(self));
        write!(self.wr, r"\}")
    }

    fn emit_map_elt_key(&mut self,
                        idx: uint,
                        f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        use std::str::from_utf8;
        if idx != 0 { try!(write!(self.wr, ",")) }
        // ref #12967, make sure to wrap a key in double quotes,
        // in the event that its of a type that omits them (eg numbers)
        let mut buf = MemWriter::new();
        let mut check_encoder = Encoder::new(&mut buf);
        try!(f(&mut check_encoder));
        let buf = buf.unwrap();
        let out = from_utf8(buf.as_slice()).unwrap();
        let needs_wrapping = out.char_at(0) != '"' &&
            out.char_at_reverse(out.len()) != '"';
        if needs_wrapping { try!(write!(self.wr, "\"")); }
        try!(f(self));
        if needs_wrapping { try!(write!(self.wr, "\"")); }
        Ok(())
    }

    fn emit_map_elt_val(&mut self,
                        _idx: uint,
                        f: |&mut Encoder<'a>| -> EncodeResult) -> EncodeResult {
        try!(write!(self.wr, ":"));
        f(self)
    }
}

/// Another encoder for JSON, but prints out human-readable JSON instead of
/// compact data
pub struct PrettyEncoder<'a> {
    wr: &'a mut io::Writer,
    indent: uint,
}

impl<'a> PrettyEncoder<'a> {
    /// Creates a new encoder whose output will be written to the specified writer
    pub fn new<'a>(wr: &'a mut io::Writer) -> PrettyEncoder<'a> {
        PrettyEncoder {
            wr: wr,
            indent: 0,
        }
    }
}

impl<'a> ::Encoder<io::IoError> for PrettyEncoder<'a> {
    fn emit_nil(&mut self) -> EncodeResult { write!(self.wr, "null") }

    fn emit_uint(&mut self, v: uint) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u64(&mut self, v: u64) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u32(&mut self, v: u32) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u16(&mut self, v: u16) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_u8(&mut self, v: u8) -> EncodeResult { self.emit_f64(v as f64) }

    fn emit_int(&mut self, v: int) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i64(&mut self, v: i64) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i32(&mut self, v: i32) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i16(&mut self, v: i16) -> EncodeResult { self.emit_f64(v as f64) }
    fn emit_i8(&mut self, v: i8) -> EncodeResult { self.emit_f64(v as f64) }

    fn emit_bool(&mut self, v: bool) -> EncodeResult {
        if v {
            write!(self.wr, "true")
        } else {
            write!(self.wr, "false")
        }
    }

    fn emit_f64(&mut self, v: f64) -> EncodeResult {
        write!(self.wr, "{}", f64::to_str_digits(v, 6u))
    }
    fn emit_f32(&mut self, v: f32) -> EncodeResult { self.emit_f64(v as f64) }

    fn emit_char(&mut self, v: char) -> EncodeResult { self.emit_str(str::from_char(v)) }
    fn emit_str(&mut self, v: &str) -> EncodeResult {
        write!(self.wr, "{}", escape_str(v))
    }

    fn emit_enum(&mut self,
                 _name: &str,
                 f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        f(self)
    }

    fn emit_enum_variant(&mut self,
                         name: &str,
                         _: uint,
                         cnt: uint,
                         f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if cnt == 0 {
            write!(self.wr, "{}", escape_str(name))
        } else {
            self.indent += 2;
            try!(write!(self.wr, "[\n{}{},\n", spaces(self.indent),
                          escape_str(name)));
            try!(f(self));
            self.indent -= 2;
            write!(self.wr, "\n{}]", spaces(self.indent))
        }
    }

    fn emit_enum_variant_arg(&mut self,
                             idx: uint,
                             f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if idx != 0 {
            try!(write!(self.wr, ",\n"));
        }
        try!(write!(self.wr, "{}", spaces(self.indent)));
        f(self)
    }

    fn emit_enum_struct_variant(&mut self,
                                name: &str,
                                id: uint,
                                cnt: uint,
                                f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_enum_variant(name, id, cnt, f)
    }

    fn emit_enum_struct_variant_field(&mut self,
                                      _: &str,
                                      idx: uint,
                                      f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_enum_variant_arg(idx, f)
    }


    fn emit_struct(&mut self,
                   _: &str,
                   len: uint,
                   f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if len == 0 {
            write!(self.wr, "\\{\\}")
        } else {
            try!(write!(self.wr, "\\{"));
            self.indent += 2;
            try!(f(self));
            self.indent -= 2;
            write!(self.wr, "\n{}\\}", spaces(self.indent))
        }
    }

    fn emit_struct_field(&mut self,
                         name: &str,
                         idx: uint,
                         f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if idx == 0 {
            try!(write!(self.wr, "\n"));
        } else {
            try!(write!(self.wr, ",\n"));
        }
        try!(write!(self.wr, "{}{}: ", spaces(self.indent), escape_str(name)));
        f(self)
    }

    fn emit_tuple(&mut self,
                  len: uint,
                  f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq(len, f)
    }
    fn emit_tuple_arg(&mut self,
                      idx: uint,
                      f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq_elt(idx, f)
    }

    fn emit_tuple_struct(&mut self,
                         _: &str,
                         len: uint,
                         f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq(len, f)
    }
    fn emit_tuple_struct_arg(&mut self,
                             idx: uint,
                             f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        self.emit_seq_elt(idx, f)
    }

    fn emit_option(&mut self, f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        f(self)
    }
    fn emit_option_none(&mut self) -> EncodeResult { self.emit_nil() }
    fn emit_option_some(&mut self, f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        f(self)
    }

    fn emit_seq(&mut self,
                len: uint,
                f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if len == 0 {
            write!(self.wr, "[]")
        } else {
            try!(write!(self.wr, "["));
            self.indent += 2;
            try!(f(self));
            self.indent -= 2;
            write!(self.wr, "\n{}]", spaces(self.indent))
        }
    }

    fn emit_seq_elt(&mut self,
                    idx: uint,
                    f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if idx == 0 {
            try!(write!(self.wr, "\n"));
        } else {
            try!(write!(self.wr, ",\n"));
        }
        try!(write!(self.wr, "{}", spaces(self.indent)));
        f(self)
    }

    fn emit_map(&mut self,
                len: uint,
                f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        if len == 0 {
            write!(self.wr, "\\{\\}")
        } else {
            try!(write!(self.wr, "\\{"));
            self.indent += 2;
            try!(f(self));
            self.indent -= 2;
            write!(self.wr, "\n{}\\}", spaces(self.indent))
        }
    }

    fn emit_map_elt_key(&mut self,
                        idx: uint,
                        f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        use std::str::from_utf8;
        if idx == 0 {
            try!(write!(self.wr, "\n"));
        } else {
            try!(write!(self.wr, ",\n"));
        }
        try!(write!(self.wr, "{}", spaces(self.indent)));
        // ref #12967, make sure to wrap a key in double quotes,
        // in the event that its of a type that omits them (eg numbers)
        let mut buf = MemWriter::new();
        let mut check_encoder = PrettyEncoder::new(&mut buf);
        try!(f(&mut check_encoder));
        let buf = buf.unwrap();
        let out = from_utf8(buf.as_slice()).unwrap();
        let needs_wrapping = out.char_at(0) != '"' &&
            out.char_at_reverse(out.len()) != '"';
        if needs_wrapping { try!(write!(self.wr, "\"")); }
        try!(f(self));
        if needs_wrapping { try!(write!(self.wr, "\"")); }
        Ok(())
    }

    fn emit_map_elt_val(&mut self,
                        _idx: uint,
                        f: |&mut PrettyEncoder<'a>| -> EncodeResult) -> EncodeResult {
        try!(write!(self.wr, ": "));
        f(self)
    }
}

impl<E: ::Encoder<S>, S> Encodable<E, S> for Json {
    fn encode(&self, e: &mut E) -> Result<(), S> {
        match *self {
            Number(v) => v.encode(e),
            String(ref v) => v.encode(e),
            Boolean(v) => v.encode(e),
            List(ref v) => v.encode(e),
            Object(ref v) => v.encode(e),
            Null => e.emit_nil(),
        }
    }
}

impl Json {
    /// Encodes a json value into a io::writer.  Uses a single line.
    pub fn to_writer(&self, wr: &mut io::Writer) -> EncodeResult {
        let mut encoder = Encoder::new(wr);
        self.encode(&mut encoder)
    }

    /// Encodes a json value into a io::writer.
    /// Pretty-prints in a more readable format.
    pub fn to_pretty_writer(&self, wr: &mut io::Writer) -> EncodeResult {
        let mut encoder = PrettyEncoder::new(wr);
        self.encode(&mut encoder)
    }

    /// Encodes a json value into a string
    pub fn to_pretty_str(&self) -> ~str {
        let mut s = MemWriter::new();
        self.to_pretty_writer(&mut s as &mut io::Writer).unwrap();
        str::from_utf8(s.unwrap().as_slice()).unwrap().to_owned()
    }

     /// If the Json value is an Object, returns the value associated with the provided key.
    /// Otherwise, returns None.
    pub fn find<'a>(&'a self, key: &~str) -> Option<&'a Json>{
        match self {
            &Object(ref map) => map.find(key),
            _ => None
        }
    }

    /// Attempts to get a nested Json Object for each key in `keys`.
    /// If any key is found not to exist, find_path will return None.
    /// Otherwise, it will return the Json value associated with the final key.
    pub fn find_path<'a>(&'a self, keys: &[&~str]) -> Option<&'a Json>{
        let mut target = self;
        for key in keys.iter() {
            match target.find(*key) {
                Some(t) => { target = t; },
                None => return None
            }
        }
        Some(target)
    }

    /// If the Json value is an Object, performs a depth-first search until
    /// a value associated with the provided key is found. If no value is found
    /// or the Json value is not an Object, returns None.
    pub fn search<'a>(&'a self, key: &~str) -> Option<&'a Json> {
        match self {
            &Object(ref map) => {
                match map.find(key) {
                    Some(json_value) => Some(json_value),
                    None => {
                        let mut value : Option<&'a Json> = None;
                        for (_, v) in map.iter() {
                            value = v.search(key);
                            if value.is_some() {
                                break;
                            }
                        }
                        value
                    }
                }
            },
            _ => None
        }
    }

    /// Returns true if the Json value is an Object. Returns false otherwise.
    pub fn is_object<'a>(&'a self) -> bool {
        self.as_object().is_some()
    }

    /// If the Json value is an Object, returns the associated TreeMap.
    /// Returns None otherwise.
    pub fn as_object<'a>(&'a self) -> Option<&'a Object> {
        match self {
            &Object(ref map) => Some(&**map),
            _ => None
        }
    }

    /// Returns true if the Json value is a List. Returns false otherwise.
    pub fn is_list<'a>(&'a self) -> bool {
        self.as_list().is_some()
    }

    /// If the Json value is a List, returns the associated vector.
    /// Returns None otherwise.
    pub fn as_list<'a>(&'a self) -> Option<&'a List> {
        match self {
            &List(ref list) => Some(&*list),
            _ => None
        }
    }

    /// Returns true if the Json value is a String. Returns false otherwise.
    pub fn is_string<'a>(&'a self) -> bool {
        self.as_string().is_some()
    }

    /// If the Json value is a String, returns the associated str.
    /// Returns None otherwise.
    pub fn as_string<'a>(&'a self) -> Option<&'a str> {
        match *self {
            String(ref s) => Some(s.as_slice()),
            _ => None
        }
    }

    /// Returns true if the Json value is a Number. Returns false otherwise.
    pub fn is_number(&self) -> bool {
        self.as_number().is_some()
    }

    /// If the Json value is a Number, returns the associated f64.
    /// Returns None otherwise.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            &Number(n) => Some(n),
            _ => None
        }
    }

    /// Returns true if the Json value is a Boolean. Returns false otherwise.
    pub fn is_boolean(&self) -> bool {
        self.as_boolean().is_some()
    }

    /// If the Json value is a Boolean, returns the associated bool.
    /// Returns None otherwise.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            &Boolean(b) => Some(b),
            _ => None
        }
    }

    /// Returns true if the Json value is a Null. Returns false otherwise.
    pub fn is_null(&self) -> bool {
        self.as_null().is_some()
    }

    /// If the Json value is a Null, returns ().
    /// Returns None otherwise.
    pub fn as_null(&self) -> Option<()> {
        match self {
            &Null => Some(()),
            _ => None
        }
    }
}

pub struct Parser<T> {
    rdr: T,
    ch: Option<char>,
    line: uint,
    col: uint,
}

impl<T: Iterator<char>> Parser<T> {
    /// Decode a json value from an Iterator<char>
    pub fn new(rdr: T) -> Parser<T> {
        let mut p = Parser {
            rdr: rdr,
            ch: Some('\x00'),
            line: 1,
            col: 0,
        };
        p.bump();
        p
    }
}

impl<T: Iterator<char>> Parser<T> {
    pub fn parse(&mut self) -> DecodeResult<Json> {
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
          Err(e) => Err(e)
        }
    }
}

impl<T : Iterator<char>> Parser<T> {
    fn eof(&self) -> bool { self.ch.is_none() }
    fn ch_or_null(&self) -> char { self.ch.unwrap_or('\x00') }
    fn bump(&mut self) {
        self.ch = self.rdr.next();

        if self.ch_is('\n') {
            self.line += 1u;
            self.col = 1u;
        } else {
            self.col += 1u;
        }
    }

    fn next_char(&mut self) -> Option<char> {
        self.bump();
        self.ch
    }
    fn ch_is(&self, c: char) -> bool {
        self.ch == Some(c)
    }

    fn error<T>(&self, msg: ~str) -> DecodeResult<T> {
        Err(ParseError(msg, self.line, self.col))
    }

    fn parse_value(&mut self) -> DecodeResult<Json> {
        self.parse_whitespace();

        if self.eof() { return self.error(~"EOF while parsing value"); }

        match self.ch_or_null() {
            'n' => self.parse_ident("ull", Null),
            't' => self.parse_ident("rue", Boolean(true)),
            'f' => self.parse_ident("alse", Boolean(false)),
            '0' .. '9' | '-' => self.parse_number(),
            '"' => {
                match self.parse_str() {
                    Ok(s) => Ok(String(s)),
                    Err(e) => Err(e),
                }
            },
            '[' => self.parse_list(),
            '{' => self.parse_object(),
            _ => self.error(~"invalid syntax"),
        }
    }

    fn parse_whitespace(&mut self) {
        while self.ch_is(' ') ||
              self.ch_is('\n') ||
              self.ch_is('\t') ||
              self.ch_is('\r') { self.bump(); }
    }

    fn parse_ident(&mut self, ident: &str, value: Json) -> DecodeResult<Json> {
        if ident.chars().all(|c| Some(c) == self.next_char()) {
            self.bump();
            Ok(value)
        } else {
            self.error(~"invalid syntax")
        }
    }

    fn parse_number(&mut self) -> DecodeResult<Json> {
        let mut neg = 1.0;

        if self.ch_is('-') {
            self.bump();
            neg = -1.0;
        }

        let mut res = match self.parse_integer() {
          Ok(res) => res,
          Err(e) => return Err(e)
        };

        if self.ch_is('.') {
            match self.parse_decimal(res) {
              Ok(r) => res = r,
              Err(e) => return Err(e)
            }
        }

        if self.ch_is('e') || self.ch_is('E') {
            match self.parse_exponent(res) {
              Ok(r) => res = r,
              Err(e) => return Err(e)
            }
        }

        Ok(Number(neg * res))
    }

    fn parse_integer(&mut self) -> DecodeResult<f64> {
        let mut res = 0.0;

        match self.ch_or_null() {
            '0' => {
                self.bump();

                // There can be only one leading '0'.
                match self.ch_or_null() {
                    '0' .. '9' => return self.error(~"invalid number"),
                    _ => ()
                }
            },
            '1' .. '9' => {
                while !self.eof() {
                    match self.ch_or_null() {
                        c @ '0' .. '9' => {
                            res *= 10.0;
                            res += ((c as int) - ('0' as int)) as f64;

                            self.bump();
                        }
                        _ => break,
                    }
                }
            }
            _ => return self.error(~"invalid number"),
        }
        Ok(res)
    }

    fn parse_decimal(&mut self, res: f64) -> DecodeResult<f64> {
        self.bump();

        // Make sure a digit follows the decimal place.
        match self.ch_or_null() {
            '0' .. '9' => (),
             _ => return self.error(~"invalid number")
        }

        let mut res = res;
        let mut dec = 1.0;
        while !self.eof() {
            match self.ch_or_null() {
                c @ '0' .. '9' => {
                    dec /= 10.0;
                    res += (((c as int) - ('0' as int)) as f64) * dec;

                    self.bump();
                }
                _ => break,
            }
        }

        Ok(res)
    }

    fn parse_exponent(&mut self, mut res: f64) -> DecodeResult<f64> {
        self.bump();

        let mut exp = 0u;
        let mut neg_exp = false;

        if self.ch_is('+') {
            self.bump();
        } else if self.ch_is('-') {
            self.bump();
            neg_exp = true;
        }

        // Make sure a digit follows the exponent place.
        match self.ch_or_null() {
            '0' .. '9' => (),
            _ => return self.error(~"invalid number")
        }
        while !self.eof() {
            match self.ch_or_null() {
                c @ '0' .. '9' => {
                    exp *= 10;
                    exp += (c as uint) - ('0' as uint);

                    self.bump();
                }
                _ => break
            }
        }

        let exp: f64 = num::pow(10u as f64, exp);
        if neg_exp {
            res /= exp;
        } else {
            res *= exp;
        }

        Ok(res)
    }

    fn parse_str(&mut self) -> DecodeResult<~str> {
        let mut escape = false;
        let mut res = StrBuf::new();

        loop {
            self.bump();
            if self.eof() {
                return self.error(~"EOF while parsing string");
            }

            if escape {
                match self.ch_or_null() {
                    '"' => res.push_char('"'),
                    '\\' => res.push_char('\\'),
                    '/' => res.push_char('/'),
                    'b' => res.push_char('\x08'),
                    'f' => res.push_char('\x0c'),
                    'n' => res.push_char('\n'),
                    'r' => res.push_char('\r'),
                    't' => res.push_char('\t'),
                    'u' => {
                        // Parse \u1234.
                        let mut i = 0u;
                        let mut n = 0u;
                        while i < 4u && !self.eof() {
                            self.bump();
                            n = match self.ch_or_null() {
                                c @ '0' .. '9' => n * 16u + (c as uint) - ('0' as uint),
                                'a' | 'A' => n * 16u + 10u,
                                'b' | 'B' => n * 16u + 11u,
                                'c' | 'C' => n * 16u + 12u,
                                'd' | 'D' => n * 16u + 13u,
                                'e' | 'E' => n * 16u + 14u,
                                'f' | 'F' => n * 16u + 15u,
                                _ => return self.error(
                                    ~"invalid \\u escape (unrecognized hex)")
                            };

                            i += 1u;
                        }

                        // Error out if we didn't parse 4 digits.
                        if i != 4u {
                            return self.error(
                                ~"invalid \\u escape (not four digits)");
                        }

                        res.push_char(char::from_u32(n as u32).unwrap());
                    }
                    _ => return self.error(~"invalid escape"),
                }
                escape = false;
            } else if self.ch_is('\\') {
                escape = true;
            } else {
                match self.ch {
                    Some('"') => {
                        self.bump();
                        return Ok(res.into_owned());
                    },
                    Some(c) => res.push_char(c),
                    None => unreachable!()
                }
            }
        }
    }

    fn parse_list(&mut self) -> DecodeResult<Json> {
        self.bump();
        self.parse_whitespace();

        let mut values = ~[];

        if self.ch_is(']') {
            self.bump();
            return Ok(List(values));
        }

        loop {
            match self.parse_value() {
              Ok(v) => values.push(v),
              Err(e) => return Err(e)
            }

            self.parse_whitespace();
            if self.eof() {
                return self.error(~"EOF while parsing list");
            }

            if self.ch_is(',') {
                self.bump();
            } else if self.ch_is(']') {
                self.bump();
                return Ok(List(values));
            } else {
                return self.error(~"expected `,` or `]`")
            }
        };
    }

    fn parse_object(&mut self) -> DecodeResult<Json> {
        self.bump();
        self.parse_whitespace();

        let mut values = ~TreeMap::new();

        if self.ch_is('}') {
          self.bump();
          return Ok(Object(values));
        }

        while !self.eof() {
            self.parse_whitespace();

            if !self.ch_is('"') {
                return self.error(~"key must be a string");
            }

            let key = match self.parse_str() {
              Ok(key) => key,
              Err(e) => return Err(e)
            };

            self.parse_whitespace();

            if !self.ch_is(':') {
                if self.eof() { break; }
                return self.error(~"expected `:`");
            }
            self.bump();

            match self.parse_value() {
              Ok(value) => { values.insert(key, value); }
              Err(e) => return Err(e)
            }
            self.parse_whitespace();

            match self.ch_or_null() {
                ',' => self.bump(),
                '}' => { self.bump(); return Ok(Object(values)); },
                _ => {
                    if self.eof() { break; }
                    return self.error(~"expected `,` or `}`");
                }
            }
        }

        return self.error(~"EOF while parsing object");
    }
}

/// Decodes a json value from an `&mut io::Reader`
pub fn from_reader(rdr: &mut io::Reader) -> DecodeResult<Json> {
    let contents = match rdr.read_to_end() {
        Ok(c) => c,
        Err(e) => return Err(IoError(e))
    };
    let s = match str::from_utf8(contents.as_slice()) {
        Some(s) => s.to_owned(),
        None => return Err(ParseError(~"contents not utf-8", 0, 0))
    };
    let mut parser = Parser::new(s.chars());
    parser.parse()
}

/// Decodes a json value from a string
pub fn from_str(s: &str) -> DecodeResult<Json> {
    let mut parser = Parser::new(s.chars());
    parser.parse()
}

/// A structure to decode JSON to values in rust.
pub struct Decoder {
    stack: ~[Json],
}

impl Decoder {
    /// Creates a new decoder instance for decoding the specified JSON value.
    pub fn new(json: Json) -> Decoder {
        Decoder {
            stack: ~[json]
        }
    }
}

impl Decoder {
    fn pop(&mut self) -> Json {
        self.stack.pop().unwrap()
    }
}

macro_rules! expect(
    ($e:expr, Null) => ({
        match $e {
            Null => Ok(()),
            other => Err(ExpectedError(~"Null", format!("{}", other)))
        }
    });
    ($e:expr, $t:ident) => ({
        match $e {
            $t(v) => Ok(v),
            other => Err(ExpectedError(stringify!($t).to_owned(), format!("{}", other)))
        }
    })
)

impl ::Decoder<Error> for Decoder {
    fn read_nil(&mut self) -> DecodeResult<()> {
        debug!("read_nil");
        try!(expect!(self.pop(), Null));
        Ok(())
    }

    fn read_u64(&mut self)  -> DecodeResult<u64 > { Ok(try!(self.read_f64()) as u64) }
    fn read_u32(&mut self)  -> DecodeResult<u32 > { Ok(try!(self.read_f64()) as u32) }
    fn read_u16(&mut self)  -> DecodeResult<u16 > { Ok(try!(self.read_f64()) as u16) }
    fn read_u8 (&mut self)  -> DecodeResult<u8  > { Ok(try!(self.read_f64()) as u8) }
    fn read_uint(&mut self) -> DecodeResult<uint> { Ok(try!(self.read_f64()) as uint) }

    fn read_i64(&mut self) -> DecodeResult<i64> { Ok(try!(self.read_f64()) as i64) }
    fn read_i32(&mut self) -> DecodeResult<i32> { Ok(try!(self.read_f64()) as i32) }
    fn read_i16(&mut self) -> DecodeResult<i16> { Ok(try!(self.read_f64()) as i16) }
    fn read_i8 (&mut self) -> DecodeResult<i8 > { Ok(try!(self.read_f64()) as i8) }
    fn read_int(&mut self) -> DecodeResult<int> { Ok(try!(self.read_f64()) as int) }

    fn read_bool(&mut self) -> DecodeResult<bool> {
        debug!("read_bool");
        Ok(try!(expect!(self.pop(), Boolean)))
    }

    fn read_f64(&mut self) -> DecodeResult<f64> {
        use std::from_str::FromStr;
        debug!("read_f64");
        match self.pop() {
            Number(f) => Ok(f),
            String(s) => {
                // re: #12967.. a type w/ numeric keys (ie HashMap<uint, V> etc)
                // is going to have a string here, as per JSON spec..
                Ok(FromStr::from_str(s).unwrap())
            },
            value => Err(ExpectedError(~"Number", format!("{}", value)))
        }
    }

    fn read_f32(&mut self) -> DecodeResult<f32> { Ok(try!(self.read_f64()) as f32) }

    fn read_char(&mut self) -> DecodeResult<char> {
        let s = try!(self.read_str());
        {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                // exactly one character
                (Some(c), None) => return Ok(c),
                _ => ()
            }
        }
        Err(ExpectedError(~"single character string", format!("{}", s)))
    }

    fn read_str(&mut self) -> DecodeResult<~str> {
        debug!("read_str");
        Ok(try!(expect!(self.pop(), String)))
    }

    fn read_enum<T>(&mut self,
                    name: &str,
                    f: |&mut Decoder| -> DecodeResult<T>) -> DecodeResult<T> {
        debug!("read_enum({})", name);
        f(self)
    }

    fn read_enum_variant<T>(&mut self,
                            names: &[&str],
                            f: |&mut Decoder, uint| -> DecodeResult<T>)
                            -> DecodeResult<T> {
        debug!("read_enum_variant(names={:?})", names);
        let name = match self.pop() {
            String(s) => s,
            Object(mut o) => {
                let n = match o.pop(&~"variant") {
                    Some(String(s)) => s,
                    Some(val) => return Err(ExpectedError(~"String", format!("{}", val))),
                    None => return Err(MissingFieldError(~"variant"))
                };
                match o.pop(&~"fields") {
                    Some(List(l)) => {
                        for field in l.move_rev_iter() {
                            self.stack.push(field.clone());
                        }
                    },
                    Some(val) => return Err(ExpectedError(~"List", format!("{}", val))),
                    None => return Err(MissingFieldError(~"fields"))
                }
                n
            }
            json => return Err(ExpectedError(~"String or Object", format!("{}", json)))
        };
        let idx = match names.iter().position(|n| str::eq_slice(*n, name)) {
            Some(idx) => idx,
            None => return Err(UnknownVariantError(name))
        };
        f(self, idx)
    }

    fn read_enum_variant_arg<T>(&mut self, idx: uint, f: |&mut Decoder| -> DecodeResult<T>)
                                -> DecodeResult<T> {
        debug!("read_enum_variant_arg(idx={})", idx);
        f(self)
    }

    fn read_enum_struct_variant<T>(&mut self,
                                   names: &[&str],
                                   f: |&mut Decoder, uint| -> DecodeResult<T>)
                                   -> DecodeResult<T> {
        debug!("read_enum_struct_variant(names={:?})", names);
        self.read_enum_variant(names, f)
    }


    fn read_enum_struct_variant_field<T>(&mut self,
                                         name: &str,
                                         idx: uint,
                                         f: |&mut Decoder| -> DecodeResult<T>)
                                         -> DecodeResult<T> {
        debug!("read_enum_struct_variant_field(name={}, idx={})", name, idx);
        self.read_enum_variant_arg(idx, f)
    }

    fn read_struct<T>(&mut self,
                      name: &str,
                      len: uint,
                      f: |&mut Decoder| -> DecodeResult<T>)
                      -> DecodeResult<T> {
        debug!("read_struct(name={}, len={})", name, len);
        let value = try!(f(self));
        self.pop();
        Ok(value)
    }

    fn read_struct_field<T>(&mut self,
                            name: &str,
                            idx: uint,
                            f: |&mut Decoder| -> DecodeResult<T>)
                            -> DecodeResult<T> {
        debug!("read_struct_field(name={}, idx={})", name, idx);
        let mut obj = try!(expect!(self.pop(), Object));

        let value = match obj.pop(&name.to_owned()) {
            None => return Err(MissingFieldError(name.to_owned())),
            Some(json) => {
                self.stack.push(json);
                try!(f(self))
            }
        };
        self.stack.push(Object(obj));
        Ok(value)
    }

    fn read_tuple<T>(&mut self, f: |&mut Decoder, uint| -> DecodeResult<T>) -> DecodeResult<T> {
        debug!("read_tuple()");
        self.read_seq(f)
    }

    fn read_tuple_arg<T>(&mut self,
                         idx: uint,
                         f: |&mut Decoder| -> DecodeResult<T>) -> DecodeResult<T> {
        debug!("read_tuple_arg(idx={})", idx);
        self.read_seq_elt(idx, f)
    }

    fn read_tuple_struct<T>(&mut self,
                            name: &str,
                            f: |&mut Decoder, uint| -> DecodeResult<T>)
                            -> DecodeResult<T> {
        debug!("read_tuple_struct(name={})", name);
        self.read_tuple(f)
    }

    fn read_tuple_struct_arg<T>(&mut self,
                                idx: uint,
                                f: |&mut Decoder| -> DecodeResult<T>)
                                -> DecodeResult<T> {
        debug!("read_tuple_struct_arg(idx={})", idx);
        self.read_tuple_arg(idx, f)
    }

    fn read_option<T>(&mut self, f: |&mut Decoder, bool| -> DecodeResult<T>) -> DecodeResult<T> {
        match self.pop() {
            Null => f(self, false),
            value => { self.stack.push(value); f(self, true) }
        }
    }

    fn read_seq<T>(&mut self, f: |&mut Decoder, uint| -> DecodeResult<T>) -> DecodeResult<T> {
        debug!("read_seq()");
        let list = try!(expect!(self.pop(), List));
        let len = list.len();
        for v in list.move_rev_iter() {
            self.stack.push(v);
        }
        f(self, len)
    }

    fn read_seq_elt<T>(&mut self,
                       idx: uint,
                       f: |&mut Decoder| -> DecodeResult<T>) -> DecodeResult<T> {
        debug!("read_seq_elt(idx={})", idx);
        f(self)
    }

    fn read_map<T>(&mut self, f: |&mut Decoder, uint| -> DecodeResult<T>) -> DecodeResult<T> {
        debug!("read_map()");
        let obj = try!(expect!(self.pop(), Object));
        let len = obj.len();
        for (key, value) in obj.move_iter() {
            self.stack.push(value);
            self.stack.push(String(key));
        }
        f(self, len)
    }

    fn read_map_elt_key<T>(&mut self, idx: uint, f: |&mut Decoder| -> DecodeResult<T>)
                           -> DecodeResult<T> {
        debug!("read_map_elt_key(idx={})", idx);
        f(self)
    }

    fn read_map_elt_val<T>(&mut self, idx: uint, f: |&mut Decoder| -> DecodeResult<T>)
                           -> DecodeResult<T> {
        debug!("read_map_elt_val(idx={})", idx);
        f(self)
    }
}

/// Test if two json values are less than one another
impl Ord for Json {
    fn lt(&self, other: &Json) -> bool {
        match *self {
            Number(f0) => {
                match *other {
                    Number(f1) => f0 < f1,
                    String(_) | Boolean(_) | List(_) | Object(_) |
                    Null => true
                }
            }

            String(ref s0) => {
                match *other {
                    Number(_) => false,
                    String(ref s1) => s0 < s1,
                    Boolean(_) | List(_) | Object(_) | Null => true
                }
            }

            Boolean(b0) => {
                match *other {
                    Number(_) | String(_) => false,
                    Boolean(b1) => b0 < b1,
                    List(_) | Object(_) | Null => true
                }
            }

            List(ref l0) => {
                match *other {
                    Number(_) | String(_) | Boolean(_) => false,
                    List(ref l1) => (*l0) < (*l1),
                    Object(_) | Null => true
                }
            }

            Object(ref d0) => {
                match *other {
                    Number(_) | String(_) | Boolean(_) | List(_) => false,
                    Object(ref d1) => d0 < d1,
                    Null => true
                }
            }

            Null => {
                match *other {
                    Number(_) | String(_) | Boolean(_) | List(_) |
                    Object(_) =>
                        false,
                    Null => true
                }
            }
        }
    }
}

/// A trait for converting values to JSON
pub trait ToJson {
    /// Converts the value of `self` to an instance of JSON
    fn to_json(&self) -> Json;
}

impl ToJson for Json {
    fn to_json(&self) -> Json { (*self).clone() }
}

impl ToJson for int {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for i8 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for i16 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for i32 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for i64 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for uint {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for u8 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for u16 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for u32 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for u64 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for f32 {
    fn to_json(&self) -> Json { Number(*self as f64) }
}

impl ToJson for f64 {
    fn to_json(&self) -> Json { Number(*self) }
}

impl ToJson for () {
    fn to_json(&self) -> Json { Null }
}

impl ToJson for bool {
    fn to_json(&self) -> Json { Boolean(*self) }
}

impl ToJson for ~str {
    fn to_json(&self) -> Json { String((*self).clone()) }
}

impl<A:ToJson,B:ToJson> ToJson for (A, B) {
    fn to_json(&self) -> Json {
        match *self {
          (ref a, ref b) => {
            List(~[a.to_json(), b.to_json()])
          }
        }
    }
}

impl<A:ToJson,B:ToJson,C:ToJson> ToJson for (A, B, C) {
    fn to_json(&self) -> Json {
        match *self {
          (ref a, ref b, ref c) => {
            List(~[a.to_json(), b.to_json(), c.to_json()])
          }
        }
    }
}

impl<A:ToJson> ToJson for ~[A] {
    fn to_json(&self) -> Json { List(self.iter().map(|elt| elt.to_json()).collect()) }
}

impl<A:ToJson> ToJson for TreeMap<~str, A> {
    fn to_json(&self) -> Json {
        let mut d = TreeMap::new();
        for (key, value) in self.iter() {
            d.insert((*key).clone(), value.to_json());
        }
        Object(~d)
    }
}

impl<A:ToJson> ToJson for HashMap<~str, A> {
    fn to_json(&self) -> Json {
        let mut d = TreeMap::new();
        for (key, value) in self.iter() {
            d.insert((*key).clone(), value.to_json());
        }
        Object(~d)
    }
}

impl<A:ToJson> ToJson for Option<A> {
    fn to_json(&self) -> Json {
        match *self {
          None => Null,
          Some(ref value) => value.to_json()
        }
    }
}

impl fmt::Show for Json {
    /// Encodes a json value into a string
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.to_writer(f.buf)
    }
}

#[cfg(test)]
mod tests {
    use {Encodable, Decodable};
    use super::{Encoder, Decoder, Error, Boolean, Number, List, String, Null,
                PrettyEncoder, Object, Json, from_str, ParseError, ExpectedError,
                MissingFieldError, UnknownVariantError, DecodeResult };
    use std::io;
    use collections::TreeMap;

    #[deriving(Eq, Encodable, Decodable, Show)]
    enum Animal {
        Dog,
        Frog(~str, int)
    }

    #[deriving(Eq, Encodable, Decodable, Show)]
    struct Inner {
        a: (),
        b: uint,
        c: ~[~str],
    }

    #[deriving(Eq, Encodable, Decodable, Show)]
    struct Outer {
        inner: ~[Inner],
    }

    fn mk_object(items: &[(~str, Json)]) -> Json {
        let mut d = ~TreeMap::new();

        for item in items.iter() {
            match *item {
                (ref key, ref value) => { d.insert((*key).clone(), (*value).clone()); },
            }
        };

        Object(d)
    }

    #[test]
    fn test_write_null() {
        assert_eq!(Null.to_str(), ~"null");
        assert_eq!(Null.to_pretty_str(), ~"null");
    }


    #[test]
    fn test_write_number() {
        assert_eq!(Number(3.0).to_str(), ~"3");
        assert_eq!(Number(3.0).to_pretty_str(), ~"3");

        assert_eq!(Number(3.1).to_str(), ~"3.1");
        assert_eq!(Number(3.1).to_pretty_str(), ~"3.1");

        assert_eq!(Number(-1.5).to_str(), ~"-1.5");
        assert_eq!(Number(-1.5).to_pretty_str(), ~"-1.5");

        assert_eq!(Number(0.5).to_str(), ~"0.5");
        assert_eq!(Number(0.5).to_pretty_str(), ~"0.5");
    }

    #[test]
    fn test_write_str() {
        assert_eq!(String(~"").to_str(), ~"\"\"");
        assert_eq!(String(~"").to_pretty_str(), ~"\"\"");

        assert_eq!(String(~"foo").to_str(), ~"\"foo\"");
        assert_eq!(String(~"foo").to_pretty_str(), ~"\"foo\"");
    }

    #[test]
    fn test_write_bool() {
        assert_eq!(Boolean(true).to_str(), ~"true");
        assert_eq!(Boolean(true).to_pretty_str(), ~"true");

        assert_eq!(Boolean(false).to_str(), ~"false");
        assert_eq!(Boolean(false).to_pretty_str(), ~"false");
    }

    #[test]
    fn test_write_list() {
        assert_eq!(List(~[]).to_str(), ~"[]");
        assert_eq!(List(~[]).to_pretty_str(), ~"[]");

        assert_eq!(List(~[Boolean(true)]).to_str(), ~"[true]");
        assert_eq!(
            List(~[Boolean(true)]).to_pretty_str(),
            ~"\
            [\n  \
                true\n\
            ]"
        );

        let long_test_list = List(~[
            Boolean(false),
            Null,
            List(~[String(~"foo\nbar"), Number(3.5)])]);

        assert_eq!(long_test_list.to_str(),
            ~"[false,null,[\"foo\\nbar\",3.5]]");
        assert_eq!(
            long_test_list.to_pretty_str(),
            ~"\
            [\n  \
                false,\n  \
                null,\n  \
                [\n    \
                    \"foo\\nbar\",\n    \
                    3.5\n  \
                ]\n\
            ]"
        );
    }

    #[test]
    fn test_write_object() {
        assert_eq!(mk_object([]).to_str(), ~"{}");
        assert_eq!(mk_object([]).to_pretty_str(), ~"{}");

        assert_eq!(
            mk_object([(~"a", Boolean(true))]).to_str(),
            ~"{\"a\":true}"
        );
        assert_eq!(
            mk_object([(~"a", Boolean(true))]).to_pretty_str(),
            ~"\
            {\n  \
                \"a\": true\n\
            }"
        );

        let complex_obj = mk_object([
                (~"b", List(~[
                    mk_object([(~"c", String(~"\x0c\r"))]),
                    mk_object([(~"d", String(~""))])
                ]))
            ]);

        assert_eq!(
            complex_obj.to_str(),
            ~"{\
                \"b\":[\
                    {\"c\":\"\\f\\r\"},\
                    {\"d\":\"\"}\
                ]\
            }"
        );
        assert_eq!(
            complex_obj.to_pretty_str(),
            ~"\
            {\n  \
                \"b\": [\n    \
                    {\n      \
                        \"c\": \"\\f\\r\"\n    \
                    },\n    \
                    {\n      \
                        \"d\": \"\"\n    \
                    }\n  \
                ]\n\
            }"
        );

        let a = mk_object([
            (~"a", Boolean(true)),
            (~"b", List(~[
                mk_object([(~"c", String(~"\x0c\r"))]),
                mk_object([(~"d", String(~""))])
            ]))
        ]);

        // We can't compare the strings directly because the object fields be
        // printed in a different order.
        assert_eq!(a.clone(), from_str(a.to_str()).unwrap());
        assert_eq!(a.clone(), from_str(a.to_pretty_str()).unwrap());
    }

    fn with_str_writer(f: |&mut io::Writer|) -> ~str {
        use std::io::MemWriter;
        use std::str;

        let mut m = MemWriter::new();
        f(&mut m as &mut io::Writer);
        str::from_utf8(m.unwrap().as_slice()).unwrap().to_owned()
    }

    #[test]
    fn test_write_enum() {
        let animal = Dog;
        assert_eq!(
            with_str_writer(|wr| {
                let mut encoder = Encoder::new(wr);
                animal.encode(&mut encoder).unwrap();
            }),
            ~"\"Dog\""
        );
        assert_eq!(
            with_str_writer(|wr| {
                let mut encoder = PrettyEncoder::new(wr);
                animal.encode(&mut encoder).unwrap();
            }),
            ~"\"Dog\""
        );

        let animal = Frog(~"Henry", 349);
        assert_eq!(
            with_str_writer(|wr| {
                let mut encoder = Encoder::new(wr);
                animal.encode(&mut encoder).unwrap();
            }),
            ~"{\"variant\":\"Frog\",\"fields\":[\"Henry\",349]}"
        );
        assert_eq!(
            with_str_writer(|wr| {
                let mut encoder = PrettyEncoder::new(wr);
                animal.encode(&mut encoder).unwrap();
            }),
            ~"\
            [\n  \
                \"Frog\",\n  \
                \"Henry\",\n  \
                349\n\
            ]"
        );
    }

    #[test]
    fn test_write_some() {
        let value = Some(~"jodhpurs");
        let s = with_str_writer(|wr| {
            let mut encoder = Encoder::new(wr);
            value.encode(&mut encoder).unwrap();
        });
        assert_eq!(s, ~"\"jodhpurs\"");

        let value = Some(~"jodhpurs");
        let s = with_str_writer(|wr| {
            let mut encoder = PrettyEncoder::new(wr);
            value.encode(&mut encoder).unwrap();
        });
        assert_eq!(s, ~"\"jodhpurs\"");
    }

    #[test]
    fn test_write_none() {
        let value: Option<~str> = None;
        let s = with_str_writer(|wr| {
            let mut encoder = Encoder::new(wr);
            value.encode(&mut encoder).unwrap();
        });
        assert_eq!(s, ~"null");

        let s = with_str_writer(|wr| {
            let mut encoder = Encoder::new(wr);
            value.encode(&mut encoder).unwrap();
        });
        assert_eq!(s, ~"null");
    }

    #[test]
    fn test_trailing_characters() {
        assert_eq!(from_str("nulla"),
            Err(ParseError(~"trailing characters", 1u, 5u)));
        assert_eq!(from_str("truea"),
            Err(ParseError(~"trailing characters", 1u, 5u)));
        assert_eq!(from_str("falsea"),
            Err(ParseError(~"trailing characters", 1u, 6u)));
        assert_eq!(from_str("1a"),
            Err(ParseError(~"trailing characters", 1u, 2u)));
        assert_eq!(from_str("[]a"),
            Err(ParseError(~"trailing characters", 1u, 3u)));
        assert_eq!(from_str("{}a"),
            Err(ParseError(~"trailing characters", 1u, 3u)));
    }

    #[test]
    fn test_read_identifiers() {
        assert_eq!(from_str("n"),
            Err(ParseError(~"invalid syntax", 1u, 2u)));
        assert_eq!(from_str("nul"),
            Err(ParseError(~"invalid syntax", 1u, 4u)));

        assert_eq!(from_str("t"),
            Err(ParseError(~"invalid syntax", 1u, 2u)));
        assert_eq!(from_str("truz"),
            Err(ParseError(~"invalid syntax", 1u, 4u)));

        assert_eq!(from_str("f"),
            Err(ParseError(~"invalid syntax", 1u, 2u)));
        assert_eq!(from_str("faz"),
            Err(ParseError(~"invalid syntax", 1u, 3u)));

        assert_eq!(from_str("null"), Ok(Null));
        assert_eq!(from_str("true"), Ok(Boolean(true)));
        assert_eq!(from_str("false"), Ok(Boolean(false)));
        assert_eq!(from_str(" null "), Ok(Null));
        assert_eq!(from_str(" true "), Ok(Boolean(true)));
        assert_eq!(from_str(" false "), Ok(Boolean(false)));
    }

    #[test]
    fn test_decode_identifiers() {
        let mut decoder = Decoder::new(from_str("null").unwrap());
        let v: () = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ());

        let mut decoder = Decoder::new(from_str("true").unwrap());
        let v: bool = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, true);

        let mut decoder = Decoder::new(from_str("false").unwrap());
        let v: bool = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, false);
    }

    #[test]
    fn test_read_number() {
        assert_eq!(from_str("+"),
            Err(ParseError(~"invalid syntax", 1u, 1u)));
        assert_eq!(from_str("."),
            Err(ParseError(~"invalid syntax", 1u, 1u)));

        assert_eq!(from_str("-"),
            Err(ParseError(~"invalid number", 1u, 2u)));
        assert_eq!(from_str("00"),
            Err(ParseError(~"invalid number", 1u, 2u)));
        assert_eq!(from_str("1."),
            Err(ParseError(~"invalid number", 1u, 3u)));
        assert_eq!(from_str("1e"),
            Err(ParseError(~"invalid number", 1u, 3u)));
        assert_eq!(from_str("1e+"),
            Err(ParseError(~"invalid number", 1u, 4u)));

        assert_eq!(from_str("3"), Ok(Number(3.0)));
        assert_eq!(from_str("3.1"), Ok(Number(3.1)));
        assert_eq!(from_str("-1.2"), Ok(Number(-1.2)));
        assert_eq!(from_str("0.4"), Ok(Number(0.4)));
        assert_eq!(from_str("0.4e5"), Ok(Number(0.4e5)));
        assert_eq!(from_str("0.4e+15"), Ok(Number(0.4e15)));
        assert_eq!(from_str("0.4e-01"), Ok(Number(0.4e-01)));
        assert_eq!(from_str(" 3 "), Ok(Number(3.0)));
    }

    #[test]
    fn test_decode_numbers() {
        let mut decoder = Decoder::new(from_str("3").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, 3.0);

        let mut decoder = Decoder::new(from_str("3.1").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, 3.1);

        let mut decoder = Decoder::new(from_str("-1.2").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, -1.2);

        let mut decoder = Decoder::new(from_str("0.4").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, 0.4);

        let mut decoder = Decoder::new(from_str("0.4e5").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, 0.4e5);

        let mut decoder = Decoder::new(from_str("0.4e15").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, 0.4e15);

        let mut decoder = Decoder::new(from_str("0.4e-01").unwrap());
        let v: f64 = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, 0.4e-01);
    }

    #[test]
    fn test_read_str() {
        assert_eq!(from_str("\""),
            Err(ParseError(~"EOF while parsing string", 1u, 2u)));
        assert_eq!(from_str("\"lol"),
            Err(ParseError(~"EOF while parsing string", 1u, 5u)));

        assert_eq!(from_str("\"\""), Ok(String(~"")));
        assert_eq!(from_str("\"foo\""), Ok(String(~"foo")));
        assert_eq!(from_str("\"\\\"\""), Ok(String(~"\"")));
        assert_eq!(from_str("\"\\b\""), Ok(String(~"\x08")));
        assert_eq!(from_str("\"\\n\""), Ok(String(~"\n")));
        assert_eq!(from_str("\"\\r\""), Ok(String(~"\r")));
        assert_eq!(from_str("\"\\t\""), Ok(String(~"\t")));
        assert_eq!(from_str(" \"foo\" "), Ok(String(~"foo")));
        assert_eq!(from_str("\"\\u12ab\""), Ok(String(~"\u12ab")));
        assert_eq!(from_str("\"\\uAB12\""), Ok(String(~"\uAB12")));
    }

    #[test]
    fn test_decode_str() {
        let mut decoder = Decoder::new(from_str("\"\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"");

        let mut decoder = Decoder::new(from_str("\"foo\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"foo");

        let mut decoder = Decoder::new(from_str("\"\\\"\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\"");

        let mut decoder = Decoder::new(from_str("\"\\b\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\x08");

        let mut decoder = Decoder::new(from_str("\"\\n\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\n");

        let mut decoder = Decoder::new(from_str("\"\\r\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\r");

        let mut decoder = Decoder::new(from_str("\"\\t\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\t");

        let mut decoder = Decoder::new(from_str("\"\\u12ab\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\u12ab");

        let mut decoder = Decoder::new(from_str("\"\\uAB12\"").unwrap());
        let v: ~str = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~"\uAB12");
    }

    #[test]
    fn test_read_list() {
        assert_eq!(from_str("["),
            Err(ParseError(~"EOF while parsing value", 1u, 2u)));
        assert_eq!(from_str("[1"),
            Err(ParseError(~"EOF while parsing list", 1u, 3u)));
        assert_eq!(from_str("[1,"),
            Err(ParseError(~"EOF while parsing value", 1u, 4u)));
        assert_eq!(from_str("[1,]"),
            Err(ParseError(~"invalid syntax", 1u, 4u)));
        assert_eq!(from_str("[6 7]"),
            Err(ParseError(~"expected `,` or `]`", 1u, 4u)));

        assert_eq!(from_str("[]"), Ok(List(~[])));
        assert_eq!(from_str("[ ]"), Ok(List(~[])));
        assert_eq!(from_str("[true]"), Ok(List(~[Boolean(true)])));
        assert_eq!(from_str("[ false ]"), Ok(List(~[Boolean(false)])));
        assert_eq!(from_str("[null]"), Ok(List(~[Null])));
        assert_eq!(from_str("[3, 1]"),
                     Ok(List(~[Number(3.0), Number(1.0)])));
        assert_eq!(from_str("\n[3, 2]\n"),
                     Ok(List(~[Number(3.0), Number(2.0)])));
        assert_eq!(from_str("[2, [4, 1]]"),
               Ok(List(~[Number(2.0), List(~[Number(4.0), Number(1.0)])])));
    }

    #[test]
    fn test_decode_list() {
        let mut decoder = Decoder::new(from_str("[]").unwrap());
        let v: ~[()] = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~[]);

        let mut decoder = Decoder::new(from_str("[null]").unwrap());
        let v: ~[()] = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~[()]);

        let mut decoder = Decoder::new(from_str("[true]").unwrap());
        let v: ~[bool] = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~[true]);

        let mut decoder = Decoder::new(from_str("[true]").unwrap());
        let v: ~[bool] = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~[true]);

        let mut decoder = Decoder::new(from_str("[3, 1]").unwrap());
        let v: ~[int] = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~[3, 1]);

        let mut decoder = Decoder::new(from_str("[[3], [1, 2]]").unwrap());
        let v: ~[~[uint]] = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(v, ~[~[3], ~[1, 2]]);
    }

    #[test]
    fn test_read_object() {
        assert_eq!(from_str("{"),
            Err(ParseError(~"EOF while parsing object", 1u, 2u)));
        assert_eq!(from_str("{ "),
            Err(ParseError(~"EOF while parsing object", 1u, 3u)));
        assert_eq!(from_str("{1"),
            Err(ParseError(~"key must be a string", 1u, 2u)));
        assert_eq!(from_str("{ \"a\""),
            Err(ParseError(~"EOF while parsing object", 1u, 6u)));
        assert_eq!(from_str("{\"a\""),
            Err(ParseError(~"EOF while parsing object", 1u, 5u)));
        assert_eq!(from_str("{\"a\" "),
            Err(ParseError(~"EOF while parsing object", 1u, 6u)));

        assert_eq!(from_str("{\"a\" 1"),
            Err(ParseError(~"expected `:`", 1u, 6u)));
        assert_eq!(from_str("{\"a\":"),
            Err(ParseError(~"EOF while parsing value", 1u, 6u)));
        assert_eq!(from_str("{\"a\":1"),
            Err(ParseError(~"EOF while parsing object", 1u, 7u)));
        assert_eq!(from_str("{\"a\":1 1"),
            Err(ParseError(~"expected `,` or `}`", 1u, 8u)));
        assert_eq!(from_str("{\"a\":1,"),
            Err(ParseError(~"EOF while parsing object", 1u, 8u)));

        assert_eq!(from_str("{}").unwrap(), mk_object([]));
        assert_eq!(from_str("{\"a\": 3}").unwrap(),
                  mk_object([(~"a", Number(3.0))]));

        assert_eq!(from_str(
                      "{ \"a\": null, \"b\" : true }").unwrap(),
                  mk_object([
                      (~"a", Null),
                      (~"b", Boolean(true))]));
        assert_eq!(from_str("\n{ \"a\": null, \"b\" : true }\n").unwrap(),
                  mk_object([
                      (~"a", Null),
                      (~"b", Boolean(true))]));
        assert_eq!(from_str(
                      "{\"a\" : 1.0 ,\"b\": [ true ]}").unwrap(),
                  mk_object([
                      (~"a", Number(1.0)),
                      (~"b", List(~[Boolean(true)]))
                  ]));
        assert_eq!(from_str(
                      ~"{" +
                          "\"a\": 1.0, " +
                          "\"b\": [" +
                              "true," +
                              "\"foo\\nbar\", " +
                              "{ \"c\": {\"d\": null} } " +
                          "]" +
                      "}").unwrap(),
                  mk_object([
                      (~"a", Number(1.0)),
                      (~"b", List(~[
                          Boolean(true),
                          String(~"foo\nbar"),
                          mk_object([
                              (~"c", mk_object([(~"d", Null)]))
                          ])
                      ]))
                  ]));
    }

    #[test]
    fn test_decode_struct() {
        let s = ~"{
            \"inner\": [
                { \"a\": null, \"b\": 2, \"c\": [\"abc\", \"xyz\"] }
            ]
        }";
        let mut decoder = Decoder::new(from_str(s).unwrap());
        let v: Outer = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(
            v,
            Outer {
                inner: ~[
                    Inner { a: (), b: 2, c: ~[~"abc", ~"xyz"] }
                ]
            }
        );
    }

    #[test]
    fn test_decode_option() {
        let mut decoder = Decoder::new(from_str("null").unwrap());
        let value: Option<~str> = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(value, None);

        let mut decoder = Decoder::new(from_str("\"jodhpurs\"").unwrap());
        let value: Option<~str> = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(value, Some(~"jodhpurs"));
    }

    #[test]
    fn test_decode_enum() {
        let mut decoder = Decoder::new(from_str("\"Dog\"").unwrap());
        let value: Animal = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(value, Dog);

        let s = "{\"variant\":\"Frog\",\"fields\":[\"Henry\",349]}";
        let mut decoder = Decoder::new(from_str(s).unwrap());
        let value: Animal = Decodable::decode(&mut decoder).unwrap();
        assert_eq!(value, Frog(~"Henry", 349));
    }

    #[test]
    fn test_decode_map() {
        let s = ~"{\"a\": \"Dog\", \"b\": {\"variant\":\"Frog\",\"fields\":[\"Henry\", 349]}}";
        let mut decoder = Decoder::new(from_str(s).unwrap());
        let mut map: TreeMap<~str, Animal> = Decodable::decode(&mut decoder).unwrap();

        assert_eq!(map.pop(&~"a"), Some(Dog));
        assert_eq!(map.pop(&~"b"), Some(Frog(~"Henry", 349)));
    }

    #[test]
    fn test_multiline_errors() {
        assert_eq!(from_str("{\n  \"foo\":\n \"bar\""),
            Err(ParseError(~"EOF while parsing object", 3u, 8u)));
    }

    #[deriving(Decodable)]
    struct DecodeStruct {
        x: f64,
        y: bool,
        z: ~str,
        w: ~[DecodeStruct]
    }
    #[deriving(Decodable)]
    enum DecodeEnum {
        A(f64),
        B(~str)
    }
    fn check_err<T: Decodable<Decoder, Error>>(to_parse: &'static str, expected: Error) {
        let res: DecodeResult<T> = match from_str(to_parse) {
            Err(e) => Err(e),
            Ok(json) => Decodable::decode(&mut Decoder::new(json))
        };
        match res {
            Ok(_) => fail!("`{}` parsed & decoded ok, expecting error `{}`",
                              to_parse, expected),
            Err(ParseError(e, _, _)) => fail!("`{}` is not valid json: {}",
                                           to_parse, e),
            Err(e) => {
                assert_eq!(e, expected);
            }

        }
    }
    #[test]
    fn test_decode_errors_struct() {
        check_err::<DecodeStruct>("[]", ExpectedError(~"Object", ~"[]"));
        check_err::<DecodeStruct>("{\"x\": true, \"y\": true, \"z\": \"\", \"w\": []}",
                                  ExpectedError(~"Number", ~"true"));
        check_err::<DecodeStruct>("{\"x\": 1, \"y\": [], \"z\": \"\", \"w\": []}",
                                  ExpectedError(~"Boolean", ~"[]"));
        check_err::<DecodeStruct>("{\"x\": 1, \"y\": true, \"z\": {}, \"w\": []}",
                                  ExpectedError(~"String", ~"{}"));
        check_err::<DecodeStruct>("{\"x\": 1, \"y\": true, \"z\": \"\", \"w\": null}",
                                  ExpectedError(~"List", ~"null"));
        check_err::<DecodeStruct>("{\"x\": 1, \"y\": true, \"z\": \"\"}",
                                  MissingFieldError(~"w"));
    }
    #[test]
    fn test_decode_errors_enum() {
        check_err::<DecodeEnum>("{}",
                                MissingFieldError(~"variant"));
        check_err::<DecodeEnum>("{\"variant\": 1}",
                                ExpectedError(~"String", ~"1"));
        check_err::<DecodeEnum>("{\"variant\": \"A\"}",
                                MissingFieldError(~"fields"));
        check_err::<DecodeEnum>("{\"variant\": \"A\", \"fields\": null}",
                                ExpectedError(~"List", ~"null"));
        check_err::<DecodeEnum>("{\"variant\": \"C\", \"fields\": []}",
                                UnknownVariantError(~"C"));
    }

    #[test]
    fn test_find(){
        let json_value = from_str("{\"dog\" : \"cat\"}").unwrap();
        let found_str = json_value.find(&~"dog");
        assert!(found_str.is_some() && found_str.unwrap().as_string().unwrap() == &"cat");
    }

    #[test]
    fn test_find_path(){
        let json_value = from_str("{\"dog\":{\"cat\": {\"mouse\" : \"cheese\"}}}").unwrap();
        let found_str = json_value.find_path(&[&~"dog", &~"cat", &~"mouse"]);
        assert!(found_str.is_some() && found_str.unwrap().as_string().unwrap() == &"cheese");
    }

    #[test]
    fn test_search(){
        let json_value = from_str("{\"dog\":{\"cat\": {\"mouse\" : \"cheese\"}}}").unwrap();
        let found_str = json_value.search(&~"mouse").and_then(|j| j.as_string());
        assert!(found_str.is_some());
        assert!(found_str.unwrap() == &"cheese");
    }

    #[test]
    fn test_is_object(){
        let json_value = from_str("{}").unwrap();
        assert!(json_value.is_object());
    }

    #[test]
    fn test_as_object(){
        let json_value = from_str("{}").unwrap();
        let json_object = json_value.as_object();
        assert!(json_object.is_some());
    }

    #[test]
    fn test_is_list(){
        let json_value = from_str("[1, 2, 3]").unwrap();
        assert!(json_value.is_list());
    }

    #[test]
    fn test_as_list(){
        let json_value = from_str("[1, 2, 3]").unwrap();
        let json_list = json_value.as_list();
        let expected_length = 3;
        assert!(json_list.is_some() && json_list.unwrap().len() == expected_length);
    }

    #[test]
    fn test_is_string(){
        let json_value = from_str("\"dog\"").unwrap();
        assert!(json_value.is_string());
    }

    #[test]
    fn test_as_string(){
        let json_value = from_str("\"dog\"").unwrap();
        let json_str = json_value.as_string();
        let expected_str = &"dog";
        assert_eq!(json_str, Some(expected_str));
    }

    #[test]
    fn test_is_number(){
        let json_value = from_str("12").unwrap();
        assert!(json_value.is_number());
    }

    #[test]
    fn test_as_number(){
        let json_value = from_str("12").unwrap();
        let json_num = json_value.as_number();
        let expected_num = 12f64;
        assert!(json_num.is_some() && json_num.unwrap() == expected_num);
    }

    #[test]
    fn test_is_boolean(){
        let json_value = from_str("false").unwrap();
        assert!(json_value.is_boolean());
    }

    #[test]
    fn test_as_boolean(){
        let json_value = from_str("false").unwrap();
        let json_bool = json_value.as_boolean();
        let expected_bool = false;
        assert!(json_bool.is_some() && json_bool.unwrap() == expected_bool);
    }

    #[test]
    fn test_is_null(){
        let json_value = from_str("null").unwrap();
        assert!(json_value.is_null());
    }

    #[test]
    fn test_as_null(){
        let json_value = from_str("null").unwrap();
        let json_null = json_value.as_null();
        let expected_null = ();
        assert!(json_null.is_some() && json_null.unwrap() == expected_null);
    }

    #[test]
    fn test_encode_hashmap_with_numeric_key() {
        use std::str::from_utf8;
        use std::io::Writer;
        use std::io::MemWriter;
        use collections::HashMap;
        let mut hm: HashMap<uint, bool> = HashMap::new();
        hm.insert(1, true);
        let mut mem_buf = MemWriter::new();
        {
            let mut encoder = Encoder::new(&mut mem_buf as &mut io::Writer);
            hm.encode(&mut encoder).unwrap();
        }
        let bytes = mem_buf.unwrap();
        let json_str = from_utf8(bytes.as_slice()).unwrap();
        match from_str(json_str) {
            Err(_) => fail!("Unable to parse json_str: {:?}", json_str),
            _ => {} // it parsed and we are good to go
        }
    }
    #[test]
    fn test_prettyencode_hashmap_with_numeric_key() {
        use std::str::from_utf8;
        use std::io::Writer;
        use std::io::MemWriter;
        use collections::HashMap;
        let mut hm: HashMap<uint, bool> = HashMap::new();
        hm.insert(1, true);
        let mut mem_buf = MemWriter::new();
        {
            let mut encoder = PrettyEncoder::new(&mut mem_buf as &mut io::Writer);
            hm.encode(&mut encoder).unwrap();
        }
        let bytes = mem_buf.unwrap();
        let json_str = from_utf8(bytes.as_slice()).unwrap();
        match from_str(json_str) {
            Err(_) => fail!("Unable to parse json_str: {:?}", json_str),
            _ => {} // it parsed and we are good to go
        }
    }
    #[test]
    fn test_hashmap_with_numeric_key_can_handle_double_quote_delimited_key() {
        use collections::HashMap;
        use Decodable;
        let json_str = "{\"1\":true}";
        let json_obj = match from_str(json_str) {
            Err(_) => fail!("Unable to parse json_str: {:?}", json_str),
            Ok(o) => o
        };
        let mut decoder = Decoder::new(json_obj);
        let _hm: HashMap<uint, bool> = Decodable::decode(&mut decoder).unwrap();
    }
}
