// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*
 * A simple resizable circular buffer.
 */

#include "circular_buffer.h"
#include "rust_globals.h"
#include "rust_kernel.h"

circular_buffer::circular_buffer(rust_kernel *kernel, size_t unit_sz) :
    kernel(kernel),
    unit_sz(unit_sz),
    _buffer_sz(initial_size()),
    _next(0),
    _unread(0),
    _buffer((uint8_t *)kernel->malloc(_buffer_sz, "circular_buffer")) {

    assert(unit_sz && "Unit size must be larger than zero.");

    KLOG(kernel, mem, "new circular_buffer(buffer_sz=%d, unread=%d)"
         "-> circular_buffer=0x%" PRIxPTR,
         _buffer_sz, _unread, this);

    assert(_buffer && "Failed to allocate buffer.");
}

circular_buffer::~circular_buffer() {
    KLOG(kernel, mem, "~circular_buffer 0x%" PRIxPTR, this);
    assert(_buffer);
    assert(_unread == 0 && "didn't expect bytes in the circular buffer");

    kernel->free(_buffer);
}

size_t
circular_buffer::initial_size() {
    assert(unit_sz > 0);
    return INITIAL_CIRCULAR_BUFFER_SIZE_IN_UNITS * unit_sz;
}

/**
 * Copies the unread data from this buffer to the "dst" address.
 */
void
circular_buffer::transfer(void *dst) {
    assert(dst);
    assert(_unread <= _buffer_sz);

    uint8_t *ptr = (uint8_t *) dst;

    // First copy from _next to either the end of the unread
    // items or the end of the buffer
    size_t head_sz;
    if (_next + _unread <= _buffer_sz) {
        head_sz = _unread;
    } else {
        head_sz = _buffer_sz - _next;
    }
    assert(_next + head_sz <= _buffer_sz);
    memcpy(ptr, _buffer + _next, head_sz);

    // Then copy any other items from the beginning of the buffer
    assert(_unread >= head_sz);
    size_t tail_sz = _unread - head_sz;
    assert(head_sz + tail_sz <= _buffer_sz);
    memcpy(ptr + head_sz, _buffer, tail_sz);
}

/**
 * Copies the data at the "src" address into this buffer. The buffer is
 * grown if it isn't large enough.
 */
void
circular_buffer::enqueue(void *src) {
    assert(src);
    assert(_unread <= _buffer_sz);
    assert(_buffer);

    // Grow if necessary.
    if (_unread == _buffer_sz) {
        grow();
    }

    KLOG(kernel, mem, "circular_buffer enqueue "
         "unread: %d, next: %d, buffer_sz: %d, unit_sz: %d",
         _unread, _next, _buffer_sz, unit_sz);

    assert(_unread < _buffer_sz);
    assert(_unread + unit_sz <= _buffer_sz);

    // Copy data
    size_t dst_idx = _next + _unread;
    assert(dst_idx >= _buffer_sz || dst_idx + unit_sz <= _buffer_sz);
    if (dst_idx >= _buffer_sz) {
        dst_idx -= _buffer_sz;

        assert(_next >= unit_sz);
        assert(dst_idx <= _next - unit_sz);
    }

    assert(dst_idx + unit_sz <= _buffer_sz);
    memcpy(&_buffer[dst_idx], src, unit_sz);
    _unread += unit_sz;

    KLOG(kernel, mem, "circular_buffer pushed data at index: %d", dst_idx);
}

/**
 * Copies data from this buffer to the "dst" address. The buffer is
 * shrunk if possible. If the "dst" address is NULL, then the message
 * is dequeued but is not copied.
 */
void
circular_buffer::dequeue(void *dst) {
    assert(unit_sz > 0);
    assert(_unread >= unit_sz);
    assert(_unread <= _buffer_sz);
    assert(_buffer);

    KLOG(kernel, mem,
             "circular_buffer dequeue "
             "unread: %d, next: %d, buffer_sz: %d, unit_sz: %d",
             _unread, _next, _buffer_sz, unit_sz);

    assert(_next + unit_sz <= _buffer_sz);
    if (dst != NULL) {
        memcpy(dst, &_buffer[_next], unit_sz);
    }
    KLOG(kernel, mem, "shifted data from index %d", _next);
    _unread -= unit_sz;
    _next += unit_sz;
    if (_next == _buffer_sz) {
        _next = 0;
    }

    // Shrink if possible.
    if (_buffer_sz > initial_size() && _unread <= _buffer_sz / 4) {
        shrink();
    }
}

void
circular_buffer::grow() {
    size_t new_buffer_sz = _buffer_sz * 2;
    KLOG(kernel, mem, "circular_buffer is growing to %d bytes",
         new_buffer_sz);
    void *new_buffer = kernel->malloc(new_buffer_sz,
                                    "new circular_buffer (grow)");
    transfer(new_buffer);
    kernel->free(_buffer);
    _buffer = (uint8_t *)new_buffer;
    _next = 0;
    _buffer_sz = new_buffer_sz;
}

void
circular_buffer::shrink() {
    size_t new_buffer_sz = _buffer_sz / 2;
    assert(initial_size() <= new_buffer_sz);
    KLOG(kernel, mem, "circular_buffer is shrinking to %d bytes",
         new_buffer_sz);
    void *new_buffer = kernel->malloc(new_buffer_sz,
                                    "new circular_buffer (shrink)");
    transfer(new_buffer);
    kernel->free(_buffer);
    _buffer = (uint8_t *)new_buffer;
    _next = 0;
    _buffer_sz = new_buffer_sz;
}

uint8_t *
circular_buffer::peek() {
    return &_buffer[_next];
}

bool
circular_buffer::is_empty() {
    return _unread == 0;
}

size_t
circular_buffer::size() {
    return _unread;
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
