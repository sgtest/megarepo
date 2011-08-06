// Functions that interpret the shape of a type to perform various low-level
// actions, such as copying, freeing, comparing, and so on.

#include <algorithm>
#include <utility>
#include <cassert>
#include <cstdio>
#include <cstdlib>
#include "rust_internal.h"

#define ARENA_SIZE          256

#define DPRINT(fmt,...)     fprintf(stderr, fmt, ##__VA_ARGS__)
#define DPRINTCX(cx)        print::print_cx(cx)

//#define DPRINT(fmt,...)
//#define DPRINTCX(cx)

#ifdef _MSC_VER
#define ALIGNOF     __alignof
#else
#define ALIGNOF     __alignof__
#endif

namespace shape {

using namespace shape;

// Forward declarations

struct rust_obj;
struct size_align;
struct type_param;


// Constants

const uint8_t SHAPE_U8 = 0u;
const uint8_t SHAPE_U16 = 1u;
const uint8_t SHAPE_U32 = 2u;
const uint8_t SHAPE_U64 = 3u;
const uint8_t SHAPE_I8 = 4u;
const uint8_t SHAPE_I16 = 5u;
const uint8_t SHAPE_I32 = 6u;
const uint8_t SHAPE_I64 = 7u;
const uint8_t SHAPE_F32 = 8u;
const uint8_t SHAPE_F64 = 9u;
const uint8_t SHAPE_EVEC = 10u;
const uint8_t SHAPE_IVEC = 11u;
const uint8_t SHAPE_TAG = 12u;
const uint8_t SHAPE_BOX = 13u;
const uint8_t SHAPE_PORT = 14u;
const uint8_t SHAPE_CHAN = 15u;
const uint8_t SHAPE_TASK = 16u;
const uint8_t SHAPE_STRUCT = 17u;
const uint8_t SHAPE_FN = 18u;
const uint8_t SHAPE_OBJ = 19u;
const uint8_t SHAPE_RES = 20u;
const uint8_t SHAPE_VAR = 21u;

const uint8_t CMP_EQ = 0u;
const uint8_t CMP_LT = 1u;
const uint8_t CMP_LE = 2u;


// Utility functions

// Rounds |size| to the nearest |alignment|. Invariant: |alignment| is a power
// of two.
template<typename T>
static inline T
round_up(T size, size_t alignment) {
    assert(alignment);
    T x = (T)(((uintptr_t)size + alignment - 1) & ~(alignment - 1));
    return x;
}


// Utility classes

struct size_align {
    size_t size;
    size_t alignment;

    size_align(size_t in_size = 0, size_t in_align = 1) :
        size(in_size), alignment(in_align) {}

    bool is_set() const { return alignment != 0; }

    inline void set(size_t in_size, size_t in_align) {
        size = in_size;
        alignment = in_align;
    }

    inline void add(const size_align &other) {
        add(other.size, other.alignment);
    }

    inline void add(size_t extra_size, size_t extra_align) {
        size += extra_size;
        alignment = max(alignment, extra_align);
    }

    static inline size_align make(size_t in_size) {
        size_align sa;
        sa.size = sa.alignment = in_size;
        return sa;
    }

    static inline size_align make(size_t in_size, size_t in_align) {
        size_align sa;
        sa.size = in_size;
        sa.alignment = in_align;
        return sa;
    }
};

struct tag_info {
    uint16_t tag_id;                        // The tag ID.
    const uint8_t *info_ptr;                // Pointer to the info table.
    uint16_t variant_count;                 // Number of variants in the tag.
    const uint8_t *largest_variants_ptr;    // Ptr to largest variants table.
    size_align tag_sa;                      // Size and align of this tag.
    uint16_t n_params;                      // Number of type parameters.
    const type_param *params;               // Array of type parameters.
};


// Contexts

// The base context, an abstract class. We use the curiously recurring
// template pattern here to avoid virtual dispatch.
template<typename T>
class ctxt {
public:
    const uint8_t *sp;                  // shape pointer
    const type_param *params;           // shapes of type parameters
    const rust_shape_tables *tables;
    rust_task *task;

    template<typename U>
    ctxt(const ctxt<U> &other,
         const uint8_t *in_sp = NULL,
         const type_param *in_params = NULL,
         const rust_shape_tables *in_tables = NULL)
    : sp(in_sp ? in_sp : other.sp),
      params(in_params ? in_params : other.params),
      tables(in_tables ? in_tables : other.tables),
      task(other.task) {}

    void walk(bool align);

protected:
    static inline uint16_t get_u16(const uint8_t *addr);
    static inline uint16_t get_u16_bump(const uint8_t *&addr);
    inline size_align get_size_align(const uint8_t *&addr);

private:
    void walk_evec(bool align);
    void walk_ivec(bool align);
    void walk_tag(bool align);
    void walk_box(bool align);
    void walk_struct(bool align);
    void walk_res(bool align);
    void walk_var(bool align);
};


struct rust_fn {
    void (*code)(uint8_t *rv, rust_task *task, void *env, ...);
    void *env;
};

struct rust_closure {
    type_desc *tydesc;
    uint32_t target_0;
    uint32_t target_1;
    uint32_t bindings[0];

    uint8_t *get_bindings() const { return (uint8_t *)bindings; }
};

struct rust_obj_box {
    type_desc *tydesc;

    uint8_t *get_bindings() const { return (uint8_t *)this; }
};

struct rust_vtable {
    CDECL void (*dtor)(void *rv, rust_task *task, rust_obj obj);
};

struct rust_obj {
    rust_vtable *vtable;
    void *box;
};


// Arenas; these functions must execute very quickly, so we use an arena
// instead of malloc or new.

class arena {
    uint8_t *ptr;
    uint8_t data[ARENA_SIZE];

public:
    arena() : ptr(data) {}

    template<typename T>
    inline T *alloc(size_t count = 1) {
        // FIXME: align
        size_t sz = count * sizeof(T);
        //DPRINT("size is %lu\n", sz);
        T *rv = (T *)ptr;
        ptr += sz;
        if (ptr > &data[ARENA_SIZE]) {
            fprintf(stderr, "Arena space exhausted, sorry\n");
            abort();
        }
        return rv;
    }
};


// Type parameters

struct type_param {
    const uint8_t *shape;
    const rust_shape_tables *tables;
    const struct type_param *params;    // subparameters

    template<typename T>
    inline void set(ctxt<T> *cx) {
        shape = cx->sp;
        tables = cx->tables;
        params = cx->params;
    }

    static type_param *make(const type_desc *tydesc, arena &arena) {
        uint32_t n_params = tydesc->n_params;
        if (!n_params)
            return NULL;

        type_param *ptrs = arena.alloc<type_param>(n_params);
        for (uint32_t i = 0; i < n_params; i++) {
            const type_desc *subtydesc = tydesc->first_param[i];
            ptrs[i].shape = subtydesc->shape;
            ptrs[i].tables = subtydesc->shape_tables;
            ptrs[i].params = make(subtydesc, arena);
        }
        return ptrs;
    }
};


// Traversals

#define WALK_NUMBER(c_type) \
    static_cast<T *>(this)->template walk_number<c_type>(align)
#define WALK_SIMPLE(method) static_cast<T *>(this)->method(align)

template<typename T>
void
ctxt<T>::walk(bool align) {
    switch (*sp++) {
    case SHAPE_U8:      WALK_NUMBER(uint8_t);   break;
    case SHAPE_U16:     WALK_NUMBER(uint16_t);  break;
    case SHAPE_U32:     WALK_NUMBER(uint32_t);  break;
    case SHAPE_U64:     WALK_NUMBER(uint64_t);  break;
    case SHAPE_I8:      WALK_NUMBER(int8_t);    break;
    case SHAPE_I16:     WALK_NUMBER(int16_t);   break;
    case SHAPE_I32:     WALK_NUMBER(int32_t);   break;
    case SHAPE_I64:     WALK_NUMBER(int64_t);   break;
    case SHAPE_F32:     WALK_NUMBER(float);     break;
    case SHAPE_F64:     WALK_NUMBER(double);    break;
    case SHAPE_EVEC:    walk_evec(align);       break;
    case SHAPE_IVEC:    walk_ivec(align);       break;
    case SHAPE_TAG:     walk_tag(align);        break;
    case SHAPE_BOX:     walk_box(align);        break;
    case SHAPE_PORT:    WALK_SIMPLE(walk_port); break;
    case SHAPE_CHAN:    WALK_SIMPLE(walk_chan); break;
    case SHAPE_TASK:    WALK_SIMPLE(walk_task); break;
    case SHAPE_STRUCT:  walk_struct(align);     break;
    case SHAPE_FN:      WALK_SIMPLE(walk_fn);   break;
    case SHAPE_OBJ:     WALK_SIMPLE(walk_obj);  break;
    case SHAPE_RES:     walk_res(align);        break;
    case SHAPE_VAR:     walk_var(align);        break;
    default:            abort();
    }
}

template<typename T>
uint16_t
ctxt<T>::get_u16(const uint8_t *addr) {
    return *reinterpret_cast<const uint16_t *>(addr);
}

template<typename T>
uint16_t
ctxt<T>::get_u16_bump(const uint8_t *&addr) {
    uint16_t result = get_u16(addr);
    addr += sizeof(uint16_t);
    return result;
}

template<typename T>
size_align
ctxt<T>::get_size_align(const uint8_t *&addr) {
    size_align result;
    result.size = get_u16_bump(addr);
    result.alignment = *addr++;
    return result;
}

template<typename T>
void
ctxt<T>::walk_evec(bool align) {
    bool is_pod = *sp++;

    uint16_t sp_size = get_u16_bump(sp);
    const uint8_t *end_sp = sp + sp_size;

    static_cast<T *>(this)->walk_evec(align, is_pod, sp_size);

    sp = end_sp;
}

template<typename T>
void
ctxt<T>::walk_ivec(bool align) {
    bool is_pod = *sp++;
    size_align elem_sa = get_size_align(sp);

    uint16_t sp_size = get_u16_bump(sp);
    const uint8_t *end_sp = sp + sp_size;

    // FIXME: Hack to work around our incorrect alignment in some cases.
    if (elem_sa.alignment == 8)
        elem_sa.alignment = 4;

    static_cast<T *>(this)->walk_ivec(align, is_pod, elem_sa);

    sp = end_sp;
}

template<typename T>
void
ctxt<T>::walk_tag(bool align) {
    tag_info tinfo;
    tinfo.tag_id = get_u16_bump(sp);

    // Determine the info pointer.
    uint16_t info_offset = get_u16(tables->tags +
                                   tinfo.tag_id * sizeof(uint16_t));
    tinfo.info_ptr = tables->tags + info_offset;

    tinfo.variant_count = get_u16_bump(tinfo.info_ptr);

    // Determine the largest-variants pointer.
    uint16_t largest_variants_offset = get_u16_bump(tinfo.info_ptr);
    tinfo.largest_variants_ptr = tables->tags + largest_variants_offset;

    // Determine the size and alignment.
    tinfo.tag_sa = get_size_align(tinfo.info_ptr);

    // Determine the number of parameters.
    tinfo.n_params = get_u16_bump(sp);

    // Read in the tag type parameters.
    type_param params[tinfo.n_params];
    for (uint16_t i = 0; i < tinfo.n_params; i++) {
        uint16_t len = get_u16_bump(sp);
        params[i].set(this);
        sp += len;
    }

    tinfo.params = params;

    // Call to the implementation.
    static_cast<T *>(this)->walk_tag(align, tinfo);
}

template<typename T>
void
ctxt<T>::walk_box(bool align) {
    uint16_t sp_size = get_u16_bump(sp);
    const uint8_t *end_sp = sp + sp_size;

    static_cast<T *>(this)->walk_box(align);

    sp = end_sp;
}

template<typename T>
void
ctxt<T>::walk_struct(bool align) {
    uint16_t sp_size = get_u16_bump(sp);
    const uint8_t *end_sp = sp + sp_size;

    static_cast<T *>(this)->walk_struct(align, end_sp);

    sp = end_sp;
}

template<typename T>
void
ctxt<T>::walk_res(bool align) {
    uint16_t dtor_offset = get_u16_bump(sp);
    const rust_fn **resources =
        reinterpret_cast<const rust_fn **>(tables->resources);
    const rust_fn *dtor = resources[dtor_offset];

    uint16_t n_ty_params = get_u16_bump(sp);

    uint16_t ty_params_size = get_u16_bump(sp);
    const uint8_t *ty_params_sp = sp;
    sp += ty_params_size;

    uint16_t sp_size = get_u16_bump(sp);
    const uint8_t *end_sp = sp + sp_size;

    static_cast<T *>(this)->walk_res(align, dtor, n_ty_params, ty_params_sp);

    sp = end_sp;
}

template<typename T>
void
ctxt<T>::walk_var(bool align) {
    uint8_t param = *sp++;
    static_cast<T *>(this)->walk_var(align, param);
}


// A shape printer, useful for debugging

class print : public ctxt<print> {
public:
    template<typename T>
    print(const ctxt<T> &other,
          const uint8_t *in_sp = NULL,
          const type_param *in_params = NULL,
          const rust_shape_tables *in_tables = NULL)
    : ctxt<print>(other, in_sp, in_params, in_tables) {}

    void walk_tag(bool align, tag_info &tinfo);
    void walk_struct(bool align, const uint8_t *end_sp);
    void walk_res(bool align, const rust_fn *dtor, uint16_t n_ty_params,
                  const uint8_t *ty_params_sp);
    void walk_var(bool align, uint8_t param);

    void walk_evec(bool align, bool is_pod, uint16_t sp_size) {
        DPRINT("evec<"); walk(align); DPRINT(">");
    }
    void walk_ivec(bool align, bool is_pod, size_align &elem_sa) {
        DPRINT("ivec<"); walk(align); DPRINT(">");
    }
    void walk_box(bool align) {
        DPRINT("box<"); walk(align); DPRINT(">");
    }

    void walk_port(bool align)                  { DPRINT("port"); }
    void walk_chan(bool align)                  { DPRINT("chan"); }
    void walk_task(bool align)                  { DPRINT("task"); }
    void walk_fn(bool align)                    { DPRINT("fn");   }
    void walk_obj(bool align)                   { DPRINT("obj");  }

    template<typename T>
    void walk_number(bool align) {}

    template<typename T>
    static void print_cx(const T *cx) {
        print self(*cx);
        self.walk(false);
    }
};

void
print::walk_tag(bool align, tag_info &tinfo) {
    DPRINT("tag%u", tinfo.tag_id);
    if (!tinfo.n_params)
        return;

    DPRINT("<");

    bool first = true;
    for (uint16_t i = 0; i < tinfo.n_params; i++) {
        if (!first)
            DPRINT(",");
        first = false;

        ctxt<print> sub(*this, tinfo.params[i].shape);
        sub.walk(align);
    }

    DPRINT(">");
}

void
print::walk_struct(bool align, const uint8_t *end_sp) {
    DPRINT("(");

    bool first = true;
    while (sp != end_sp) {
        if (!first)
            DPRINT(",");
        first = false;

        walk(align);
    }

    DPRINT(")");
}

void
print::walk_res(bool align, const rust_fn *dtor, uint16_t n_ty_params,
                const uint8_t *ty_params_sp) {
    DPRINT("res@%p", dtor);
    if (!n_ty_params)
        return;

    DPRINT("<");

    bool first = true;
    for (uint16_t i = 0; i < n_ty_params; i++) {
        if (!first)
            DPRINT(",");
        first = false;
        get_u16_bump(sp);   // Skip over the size.
        walk(align);
    }

    DPRINT(">");
}

void
print::walk_var(bool align, uint8_t param_index) {
    DPRINT("%c=", 'T' + param_index);

    const type_param *param = &params[param_index];
    print sub(*this, param->shape, param->params, param->tables);
    sub.walk(align);
}

template<>
void print::walk_number<uint8_t>(bool align)    { DPRINT("u8"); }
template<>
void print::walk_number<uint16_t>(bool align)   { DPRINT("u16"); }
template<>
void print::walk_number<uint32_t>(bool align)   { DPRINT("u32"); }
template<>
void print::walk_number<uint64_t>(bool align)   { DPRINT("u64"); }
template<>
void print::walk_number<int8_t>(bool align)     { DPRINT("i8"); }
template<>
void print::walk_number<int16_t>(bool align)    { DPRINT("i16"); }
template<>
void print::walk_number<int32_t>(bool align)    { DPRINT("i32"); }
template<>
void print::walk_number<int64_t>(bool align)    { DPRINT("i64"); }
template<>
void print::walk_number<float>(bool align)      { DPRINT("f32"); }
template<>
void print::walk_number<double>(bool align)     { DPRINT("f64"); }


//
// Size-of (which also computes alignment). Be warned: this is an expensive
// operation.
//
// TODO: Maybe dynamic_size_of() should call into this somehow?
//

class size_of : public ctxt<size_of> {
private:
    size_align sa;

public:
    size_of(const size_of &other,
            const uint8_t *in_sp,
            const type_param *in_params,
            const rust_shape_tables *in_tables)
    : ctxt<size_of>(other, in_sp, in_params, in_tables) {}

    void walk_tag(bool align, tag_info &tinfo);
    void walk_struct(bool align, const uint8_t *end_sp);
    void walk_ivec(bool align, bool is_pod, size_align &elem_sa);

    void walk_box(bool align)   { sa.set(sizeof(void *),   sizeof(void *)); }
    void walk_port(bool align)  { sa.set(sizeof(void *),   sizeof(void *)); }
    void walk_chan(bool align)  { sa.set(sizeof(void *),   sizeof(void *)); }
    void walk_task(bool align)  { sa.set(sizeof(void *),   sizeof(void *)); }
    void walk_fn(bool align)    { sa.set(sizeof(void *)*2, sizeof(void *)); }
    void walk_obj(bool align)   { sa.set(sizeof(void *)*2, sizeof(void *)); }

    void walk_evec(bool align, bool is_pod, uint16_t sp_size) {
        sa.set(sizeof(void *), sizeof(void *));
    }

    void walk_var(bool align, uint8_t param_index) {
        const type_param *param = &params[param_index];
        size_of sub(*this, param->shape, param->params, param->tables);
        sub.walk(align);
        sa = sub.sa;
    }

    void walk_res(bool align, const rust_fn *dtor, uint16_t n_ty_params,
                  const uint8_t *ty_params_sp) {
        abort();    // TODO
    }

    template<typename T>
    void walk_number(bool align) { sa.set(sizeof(T), ALIGNOF(T)); }

    template<typename T>
    static size_align get(const ctxt<T> &other_cx, unsigned back_up = 0) {
        size_of cx(*other_cx, other_cx->sp - back_up);
        cx.walk(false);
        assert(cx.sa.alignment > 0);
        return cx.sa;
    }
};

void
size_of::walk_tag(bool align, tag_info &tinfo) {
    // If the precalculated size and alignment are good, use them.
    if (tinfo.tag_sa.is_set()) {
        sa = tinfo.tag_sa;
        return;
    }

    uint16_t n_largest_variants = get_u16_bump(tinfo.largest_variants_ptr);
    sa.set(0, 0);
    for (uint16_t i = 0; i < n_largest_variants; i++) {
        uint16_t variant_id = get_u16_bump(tinfo.largest_variants_ptr);
        uint16_t variant_offset = get_u16(tinfo.info_ptr +
                                          variant_id * sizeof(uint16_t));
        const uint8_t *variant_ptr = tables->tags + variant_offset;

        uint16_t variant_len = get_u16_bump(variant_ptr);
        const uint8_t *variant_end = variant_ptr + variant_len;

        size_of sub(*this, variant_ptr, params, NULL);

        // Compute the size of this variant.
        size_align variant_sa;
        bool first = true;
        while (sub.sp != variant_end) {
            if (!first)
                variant_sa.size = round_up(variant_sa.size, sub.sa.alignment);
            sub.walk(!first);
            first = false;

            variant_sa.add(sub.sa.size, sub.sa.alignment);
        }

        if (sa.size < variant_sa.size)
            sa = variant_sa;
    }

    if (tinfo.variant_count == 1) {
        if (!sa.size)
            sa.set(1, 1);
    } else {
        // Add in space for the tag.
        sa.add(sizeof(uint32_t), ALIGNOF(uint32_t));
    }
}

void
size_of::walk_struct(bool align, const uint8_t *end_sp) {
    size_align struct_sa(0, 1);

    bool first = true;
    while (sp != end_sp) {
        if (!first)
            struct_sa.size = round_up(struct_sa.size, sa.alignment);
        walk(!first);
        first = false;

        struct_sa.add(sa);
    }

    sa = struct_sa;
}

void
size_of::walk_ivec(bool align, bool is_pod, size_align &elem_sa) {
    if (!elem_sa.is_set())
        walk(align);    // Determine the size the slow way.
    else
        sa = elem_sa;   // Use the size hint.

    sa.set(sizeof(rust_ivec) - sizeof(uintptr_t) + sa.size * 4,
           max(sa.alignment, sizeof(uintptr_t)));
}


#if 0

// An abstract class (again using the curiously recurring template pattern)
// for methods that actually manipulate the data involved.

#define DATA_SIMPLE(ty, call) \
    if (align) dp.align(sizeof(ty)); \
    static_cast<T *>(this)->call; \
    dp += sizeof(ty);

template<typename T,typename U>
class data : public ctxt<data> {
private:
    U dp;

public:
    void walk_tag(bool align, uint16_t tag_id, const uint8_t *info_ptr,
                  uint16_t variant_count, const uint8_t *largest_variants_ptr,
                  size_align &tag_sa, uint16_t n_params,
                  const type_param *params);
    void walk_ivec(bool align, bool is_pod, size_align &elem_sa);

    void walk_struct(bool align, const uint8_t *end_sp) {
        while (sp != end_sp) {
            // TODO: Allow subclasses to optimize for POD if they want to.
            walk(align);
            align = true;
        }
    }

    void walk_evec(bool align, bool is_pod, uint16_t sp_size) {
        DATA_SIMPLE(void *, walk_evec(align, is_pod, sp_size));
    }

    void walk_box(bool align)   { DATA_SIMPLE(void *, walk_box(align)); }
    void walk_port(bool align)  { DATA_SIMPLE(void *, walk_port(align)); }
    void walk_chan(bool align)  { DATA_SIMPLE(void *, walk_chan(align)); }
    void walk_task(bool align)  { DATA_SIMPLE(void *, walk_task(align)); }

    void walk_fn(bool align) {
        if (align) dp.align(sizeof(void *));
        static_cast<T *>(this)->walk_fn(args);
        dp += sizeof(void *) * 2;
    }

    void walk_obj(bool align) {
        if (align) dp.align(sizeof(void *));
        static_cast<T *>(this)->walk_obj(args);
        dp += sizeof(void *) * 2;
    }

    void walk_var(bool align, uint8_t param_index) {
        static_cast<T *>(this)->walk_var(align, param_index);
    }

    template<typename W>
    void walk_number(bool align) {
        DATA_SIMPLE(W, walk_number<W>(align));
    }
};

template<typename T,typename U>
void
data<T,U>::walk_ivec(bool align, bool is_pod, size_align &elem_sa) {
    if (!elem_sa.is_set())
        elem_sa = size_of::get(*this);
    else if (elem_sa.alignment == 8)
        elem_sa.alignment = 4;  // FIXME: This is an awful hack.

    // Get a pointer to the interior vector, and skip over it.
    if (align) dp.align(ALIGNOF(rust_ivec *));
    U end_dp = dp + sizeof(rust_ivec) - sizeof(uintptr_t) + elem_sa.size * 4;

    // Call to the implementation.
    static_cast<T *>(this)->walk_ivec(align, is_pod, elem_sa);

    dp = end_dp;
}

template<typename T,typename U>
void
data<T,U>::walk_tag(bool align, uint16_t tag_id, const uint8_t *info_ptr,
                    uint16_t variant_count,
                    const uint8_t *largest_variants_ptr, size_align &tag_sa,
                    uint16_t n_params, const type_param *params) {
    uint32_t tag_variant;
    U end_dp;
    if (variant_count > 1) {
        if (align) dp.align(ALIGNOF(uint32_t));
        process_tag_variant_ids(
        U::data<uint32_t> tag_variant =
}

#endif


// Copy constructors

class copy : public ctxt<copy> {
    // TODO
};

} // end namespace shape

