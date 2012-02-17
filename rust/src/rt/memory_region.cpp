#include "rust_internal.h"
#include "memory_region.h"

#if RUSTRT_TRACK_ALLOCATIONS >= 1
// For some platforms, 16 byte alignment is required.
#  define PTR_SIZE 16
#  define ALIGN_PTR(x) (((x)+PTR_SIZE-1)/PTR_SIZE*PTR_SIZE)
#  define HEADER_SIZE ALIGN_PTR(sizeof(alloc_header))
#  define MAGIC 0xbadc0ffe
#else
#  define HEADER_SIZE 0
#endif

memory_region::alloc_header *memory_region::get_header(void *mem) {
    return (alloc_header *)((char *)mem - HEADER_SIZE);
}

void *memory_region::get_data(alloc_header *ptr) {
    return (void*)((char *)ptr + HEADER_SIZE);
}

memory_region::memory_region(rust_srv *srv, bool synchronized) :
    _srv(srv), _parent(NULL), _live_allocations(0),
    _detailed_leaks(srv->env->detailed_leaks),
    _synchronized(synchronized) {
}

memory_region::memory_region(memory_region *parent) :
    _srv(parent->_srv), _parent(parent), _live_allocations(0),
    _detailed_leaks(parent->_detailed_leaks),
    _synchronized(parent->_synchronized) {
}

void memory_region::add_alloc() {
    //_live_allocations++;
    sync::increment(_live_allocations);
}

void memory_region::dec_alloc() {
    //_live_allocations--;
    sync::decrement(_live_allocations);
}

void memory_region::free(void *mem) {
    // printf("free: ptr 0x%" PRIxPTR" region=%p\n", (uintptr_t) mem, this);
    if (!mem) { return; }
    alloc_header *alloc = get_header(mem);

#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    assert(alloc->magic == MAGIC);
#   endif

    if (_live_allocations < 1) {
        _srv->fatal("live_allocs < 1", __FILE__, __LINE__, "");
    }
    release_alloc(mem);
    maybe_poison(mem);
    _srv->free(alloc);
}

void *
memory_region::realloc(void *mem, size_t orig_size) {
    if (!mem) {
        add_alloc();
    }

    alloc_header *alloc = get_header(mem);
#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    assert(alloc->magic == MAGIC);
#   endif

    size_t size = orig_size + HEADER_SIZE;
    alloc_header *newMem = (alloc_header *)_srv->realloc(alloc, size);

#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    assert(newMem->magic == MAGIC);
    newMem->size = orig_size;
#   endif

#   if RUSTRT_TRACK_ALLOCATIONS >= 2
    if (_synchronized) { _lock.lock(); }
    if (_allocation_list[newMem->index] != alloc) {
        printf("at index %d, found %p, expected %p\n",
               alloc->index, _allocation_list[alloc->index], alloc);
        printf("realloc: ptr 0x%" PRIxPTR " (%s) is not in allocation_list\n",
               (uintptr_t) get_data(alloc), alloc->tag);
        _srv->fatal("not in allocation_list", __FILE__, __LINE__, "");
    }
    else {
        _allocation_list[newMem->index] = newMem;
        // printf("realloc: stored %p at index %d, replacing %p\n",
        //        newMem, index, mem);
    }
    if (_synchronized) { _lock.unlock(); }
#   endif

    return get_data(newMem);
}

void *
memory_region::malloc(size_t size, const char *tag, bool zero) {
    size_t old_size = size;
    size += HEADER_SIZE;
    alloc_header *mem = (alloc_header *)_srv->malloc(size);

#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    mem->magic = MAGIC;
    mem->tag = tag;
    mem->index = -1;
    mem->size = old_size;
#   endif

    void *data = get_data(mem);
    claim_alloc(data);

    if(zero) {
        memset(data, 0, old_size);
    }

    return data;
}

void *
memory_region::calloc(size_t size, const char *tag) {
    return malloc(size, tag, true);
}

memory_region::~memory_region() {
    if (_synchronized) { _lock.lock(); }
    if (_live_allocations == 0 && !_detailed_leaks) {
        if (_synchronized) { _lock.unlock(); }
        return;
    }
    char msg[128];
    if(_live_allocations > 0) {
        snprintf(msg, sizeof(msg),
                 "leaked memory in rust main loop (%d objects)",
                 _live_allocations);
    }

#   if RUSTRT_TRACK_ALLOCATIONS >= 2
    if (_detailed_leaks) {
        int leak_count = 0;
        for (size_t i = 0; i < _allocation_list.size(); i++) {
            if (_allocation_list[i] != NULL) {
                alloc_header *header = (alloc_header*)_allocation_list[i];
                printf("allocation (%s) 0x%" PRIxPTR " was not freed\n",
                       header->tag,
                       (uintptr_t) get_data(header));
                ++leak_count;
            }
        }
        assert(leak_count == _live_allocations);
    }
#   endif

    if (_live_allocations > 0) {
        _srv->fatal(msg, __FILE__, __LINE__,
                    "%d objects", _live_allocations);
    }
    if (_synchronized) { _lock.unlock(); }
}

void
memory_region::release_alloc(void *mem) {
#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    alloc_header *alloc = get_header(mem);
    assert(alloc->magic == MAGIC);
#   endif

#   if RUSTRT_TRACK_ALLOCATIONS >= 2
    if (_synchronized) { _lock.lock(); }
    if (_allocation_list[alloc->index] != alloc) {
        printf("free: ptr 0x%" PRIxPTR " (%s) is not in allocation_list\n",
               (uintptr_t) get_data(alloc), alloc->tag);
        _srv->fatal("not in allocation_list", __FILE__, __LINE__, "");
    }
    else {
        // printf("freed index %d\n", index);
        _allocation_list[alloc->index] = NULL;
        alloc->index = -1;
    }
    if (_synchronized) { _lock.unlock(); }
#   endif

    dec_alloc();
}

void
memory_region::claim_alloc(void *mem) {
#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    alloc_header *alloc = get_header(mem);
    assert(alloc->magic == MAGIC);
#   endif

#   if RUSTRT_TRACK_ALLOCATIONS >= 2
    if (_synchronized) { _lock.lock(); }
    alloc->index = _allocation_list.append(alloc);
    if (_synchronized) { _lock.unlock(); }
#   endif

    add_alloc();
}

void
memory_region::maybe_poison(void *mem) {
    // TODO: We should lock this, in case the compiler doesn't.
    static int poison = -1;
    if (poison < 0) {
        char *env_str = getenv("RUST_POISON_ON_FREE");
        poison = env_str != NULL && env_str[0] != '\0';
    }

    if (!poison)
        return;

#   if RUSTRT_TRACK_ALLOCATIONS >= 1
    alloc_header *alloc = get_header(mem);
    memset(mem, '\xcd', alloc->size);
#   endif
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
