#ifndef RUST_UTIL_H
#define RUST_UTIL_H

#include "rust_task.h"
#include <limits.h>

// Inline fn used regularly elsewhere.

static inline size_t
next_power_of_two(size_t s)
{
    size_t tmp = s - 1;
    tmp |= tmp >> 1;
    tmp |= tmp >> 2;
    tmp |= tmp >> 4;
    tmp |= tmp >> 8;
    tmp |= tmp >> 16;
#ifdef _LP64
    tmp |= tmp >> 32;
#endif
    return tmp + 1;
}

// Rounds |size| to the nearest |alignment|. Invariant: |alignment| is a power
// of two.
template<typename T>
static inline T
align_to(T size, size_t alignment) {
    assert(alignment);
    T x = (T)(((uintptr_t)size + alignment - 1) & ~(alignment - 1));
    return x;
}

// Initialization helper for ISAAC RNG

inline void
isaac_init(rust_kernel *kernel, randctx *rctx)
{
        memset(rctx, 0, sizeof(randctx));

        char *rust_seed = kernel->env->rust_seed;
        if (rust_seed != NULL) {
            ub4 seed = (ub4) atoi(rust_seed);
            for (size_t i = 0; i < RANDSIZ; i ++) {
                memcpy(&rctx->randrsl[i], &seed, sizeof(ub4));
                seed = (seed + 0x7ed55d16) + (seed << 12);
            }
        } else {
#ifdef __WIN32__
            HCRYPTPROV hProv;
            kernel->win32_require
                (_T("CryptAcquireContext"),
                 CryptAcquireContext(&hProv, NULL, NULL, PROV_RSA_FULL,
                                     CRYPT_VERIFYCONTEXT|CRYPT_SILENT));
            kernel->win32_require
                (_T("CryptGenRandom"),
                 CryptGenRandom(hProv, sizeof(rctx->randrsl),
                                (BYTE*)(&rctx->randrsl)));
            kernel->win32_require
                (_T("CryptReleaseContext"),
                 CryptReleaseContext(hProv, 0));
#else
            int fd = open("/dev/urandom", O_RDONLY);
            I(kernel, fd > 0);
            I(kernel,
              read(fd, (void*) &rctx->randrsl, sizeof(rctx->randrsl))
              == sizeof(rctx->randrsl));
            I(kernel, close(fd) == 0);
#endif
        }

        randinit(rctx, 1);
}

// Interior vectors (rust-user-code level).

struct
rust_vec
{
    size_t fill;    // in bytes; if zero, heapified
    size_t alloc;   // in bytes
    uint8_t data[0];
};

template <typename T>
inline size_t vec_size(size_t elems) {
    return sizeof(rust_vec) + sizeof(T) * elems;
}

template <typename T>
inline T *
vec_data(rust_vec *v) {
    return reinterpret_cast<T*>(v->data);
}

inline void reserve_vec_exact(rust_task* task, rust_vec** vpp, size_t size) {
    if (size > (*vpp)->alloc) {
        *vpp = (rust_vec*)task->kernel
            ->realloc(*vpp, size + sizeof(rust_vec));
        (*vpp)->alloc = size;
    }
}

inline void reserve_vec(rust_task* task, rust_vec** vpp, size_t size) {
    reserve_vec_exact(task, vpp, next_power_of_two(size));
}

typedef rust_vec rust_str;

inline rust_str *
make_str(rust_kernel* kernel, const char* c, size_t strlen,
         const char* name) {
    size_t str_fill = strlen + 1;
    size_t str_alloc = str_fill;
    rust_str *str = (rust_str *)
        kernel->malloc(vec_size<char>(str_fill), name);
    str->fill = str_fill;
    str->alloc = str_alloc;
    memcpy(&str->data, c, strlen);
    str->data[strlen] = '\0';
    return str;
}

inline rust_vec *
make_str_vec(rust_kernel* kernel, size_t nstrs, char **strs) {
    rust_vec *v = (rust_vec *)
        kernel->malloc(vec_size<rust_vec*>(nstrs),
                       "str vec interior");
    v->fill = v->alloc = sizeof(rust_vec*) * nstrs;
    for (size_t i = 0; i < nstrs; ++i) {
        rust_str *str = make_str(kernel, strs[i],
                                 strlen(strs[i]),
                                 "str");
        ((rust_str**)&v->data)[i] = str;
    }
    return v;
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//

#endif
