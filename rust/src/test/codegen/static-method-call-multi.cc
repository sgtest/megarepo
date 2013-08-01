// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#include <stdlib.h>

struct Struct {
  size_t field;
  size_t method(size_t x) {
	return this->field + x;
  }
};

extern "C"
size_t test(Struct &a,
			Struct &b,
			Struct &c,
			Struct &d,
			Struct &e) {
  return a.method(b.method(c.method(d.method(e.method(1)))));
}
