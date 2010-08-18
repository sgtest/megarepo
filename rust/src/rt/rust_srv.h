/*
 *
 */

#ifndef RUST_SRV_H
#define RUST_SRV_H

#include "sync/spin_lock.h"
#include "memory_region.h"

class rust_srv {
public:
    memory_region local_region;
    memory_region synchronized_region;
    virtual void log(char const *msg);
    virtual void fatal(char const *expression,
        char const *file,
        size_t line,
        char const *format,
        ...);
    virtual void warning(char const *expression,
        char const *file,
        size_t line,
        char const *format,
        ...);
    virtual void free(void *);
    virtual void *malloc(size_t);
    virtual void *realloc(void *, size_t);
    rust_srv();
    virtual ~rust_srv();
    virtual rust_srv *clone();
};

#endif /* RUST_SRV_H */
