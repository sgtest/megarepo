// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#ifndef RUST_EXCHANGE_ALLOC_H
#define RUST_EXCHANGE_ALLOC_H

#include <stddef.h>
#include <stdint.h>

class rust_exchange_alloc {
 public:
    void *malloc(size_t size);
    void *realloc(void *mem, size_t size);
    void free(void *mem);
};

extern "C" uintptr_t *
rust_get_exchange_count_ptr();

void
rust_check_exchange_count_on_exit();

#endif
