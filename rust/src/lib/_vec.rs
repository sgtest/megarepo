import option::none;
import option::some;
import util::orb;

type vbuf = rustrt::vbuf;

type operator2[T,U,V] = fn(&T, &U) -> V;

type array[T] = vec[mutable? T];

native "rust" mod rustrt {
    type vbuf;

    fn vec_buf[T](vec[T] v, uint offset) -> vbuf;

    fn vec_len[T](vec[T] v) -> uint;
    /**
     * Sometimes we modify the vec internal data via vec_buf and need to
     * update the vec's fill length accordingly.
     */
    fn vec_len_set[T](vec[T] v, uint n);

    /**
     * The T in vec_alloc[T, U] is the type of the vec to allocate.  The
     * U is the type of an element in the vec.  So to allocate a vec[U] we
     * want to invoke this as vec_alloc[vec[U], U].
     */
    fn vec_alloc[T, U](uint n_elts) -> vec[U];
    fn vec_alloc_mut[T, U](uint n_elts) -> vec[mutable U];

    fn refcount[T](vec[T] v) -> uint;

    fn vec_print_debug_info[T](vec[T] v);

    fn vec_from_vbuf[T](vbuf v, uint n_elts) -> vec[T];

    fn unsafe_vec_to_mut[T](vec[T] v) -> vec[mutable T];
}

fn alloc[T](uint n_elts) -> vec[T] {
    ret rustrt::vec_alloc[vec[T], T](n_elts);
}

fn alloc_mut[T](uint n_elts) -> vec[mutable T] {
    ret rustrt::vec_alloc_mut[vec[mutable T], T](n_elts);
}

fn refcount[T](array[T] v) -> uint {
    auto r = rustrt::refcount[T](v);
    if (r == dbg::const_refcount) {
        ret r;
    } else {
        // -1 because calling this function incremented the refcount.
        ret  r - 1u;
    }
}

fn vec_from_vbuf[T](vbuf v, uint n_elts) -> vec[T] {
    ret rustrt::vec_from_vbuf[T](v, n_elts);
}

// FIXME: Remove me; this is a botch to get around rustboot's bad typechecker.
fn empty[T]() -> vec[T] {
    ret alloc[T](0u);
}

// FIXME: Remove me; this is a botch to get around rustboot's bad typechecker.
fn empty_mut[T]() -> vec[mutable T] {
    ret alloc_mut[T](0u);
}

type init_op[T] = fn(uint i) -> T;

fn init_fn[T](&init_op[T] op, uint n_elts) -> vec[T] {
    let vec[T] v = alloc[T](n_elts);
    let uint i = 0u;
    while (i < n_elts) {
        v += vec(op(i));
        i += 1u;
    }
    ret v;
}

fn init_fn_mut[T](&init_op[T] op, uint n_elts) -> vec[mutable T] {
    let vec[mutable T] v = alloc_mut[T](n_elts);
    let uint i = 0u;
    while (i < n_elts) {
        v += vec(mutable op(i));
        i += 1u;
    }
    ret v;
}

fn init_elt[T](&T t, uint n_elts) -> vec[T] {
    /**
     * FIXME (issue #81): should be:
     *
     * fn elt_op[T](&T x, uint i) -> T { ret x; }
     * let init_op[T] inner = bind elt_op[T](t, _);
     * ret init_fn[T](inner, n_elts);
     */
    let vec[T] v = alloc[T](n_elts);
    let uint i = n_elts;
    while (i > 0u) {
        i -= 1u;
        v += vec(t);
    }
    ret v;
}

fn init_elt_mut[T](&T t, uint n_elts) -> vec[mutable T] {
    let vec[mutable T] v = alloc_mut[T](n_elts);
    let uint i = n_elts;
    while (i > 0u) {
        i -= 1u;
        v += vec(mutable t);
    }
    ret v;
}

fn buf[T](array[T] v) -> vbuf {
    ret rustrt::vec_buf[T](v, 0u);
}

fn len[T](array[T] v) -> uint {
    ret rustrt::vec_len[T](v);
}

fn len_set[T](array[T] v, uint n) {
    rustrt::vec_len_set[T](v, n);
}

fn buf_off[T](array[T] v, uint offset) -> vbuf {
     assert (offset < len[T](v));
    ret rustrt::vec_buf[T](v, offset);
}

fn print_debug_info[T](array[T] v) {
    rustrt::vec_print_debug_info[T](v);
}

// Returns the last element of v.
fn last[T](array[T] v) -> option::t[T] {
    auto l = len[T](v);
    if (l == 0u) {
        ret none[T];
    }
    ret some[T](v.(l - 1u));
}

// Returns elements from [start..end) from v.

fn slice[T](array[T] v, uint start, uint end) -> vec[T] {
    assert (start <= end);
    assert (end <= len[T](v));
    auto result = alloc[T](end - start);
    let uint i = start;
    while (i < end) {
        result += vec(v.(i));
        i += 1u;
    }
    ret result;
}

fn shift[T](&mutable array[T] v) -> T {
    auto ln = len[T](v);
    assert (ln > 0u);
    auto e = v.(0);
    v = slice[T](v, 1u, ln);
    ret e;
}

fn pop[T](&mutable array[T] v) -> T {
    auto ln = len[T](v);
    assert (ln > 0u);
    ln -= 1u;
    auto e = v.(ln);
    v = slice[T](v, 0u, ln);
    ret e;
}

fn push[T](&mutable array[T] v, &T t) {
    v += vec(t);
}

fn unshift[T](&mutable array[T] v, &T t) {
    auto res = alloc[T](len[T](v) + 1u);
    res += vec(t);
    res += v;
    v = res;
}

fn grow[T](&array[T] v, uint n, &T initval) {
    let uint i = n;
    while (i > 0u) {
        i -= 1u;
        v += vec(initval);
    }
}

fn grow_set[T](&vec[mutable T] v, uint index, &T initval, &T val) {
    auto length = _vec::len(v);
    if (index >= length) {
        grow(v, index - length + 1u, initval);
    }
    v.(index) = val;
}

fn map[T, U](&option::operator[T,U] f, &array[T] v) -> vec[U] {
    let vec[U] u = alloc[U](len[T](v));
    for (T ve in v) {
        u += vec(f(ve));
    }
    ret u;
}

fn map2[T,U,V](&operator2[T,U,V] f, &array[T] v0, &array[U] v1) -> vec[V] {
    auto v0_len = len[T](v0);
    if (v0_len != len[U](v1)) {
        fail;
    }

    let vec[V] u = alloc[V](v0_len);
    auto i = 0u;
    while (i < v0_len) {
        u += vec(f(v0.(i), v1.(i)));
        i += 1u;
    }

    ret u;
}

fn find[T](fn (&T) -> bool f, &array[T] v) -> option::t[T] {
    for (T elt in v) {
        if (f(elt)) {
            ret some[T](elt);
        }
    }

    ret none[T];
}

fn foldl[T, U](fn (&U, &T) -> U p, &U z, &vec[T] v) -> U {
    auto sz = len[T](v);

    if (sz == 0u) {
        ret z;
    }
    else {
        auto rest = slice[T](v, 1u, sz);

        ret (p(foldl[T,U](p, z, rest), v.(0)));
    }
}

fn unzip[T, U](&vec[tup(T, U)] v) -> tup(vec[T], vec[U]) {
    auto sz = len[tup(T, U)](v);

    if (sz == 0u) {
        ret tup(alloc[T](0u), alloc[U](0u));
    }
    else {
        auto rest = slice[tup(T, U)](v, 1u, sz);
        auto tl   = unzip[T, U](rest);
        auto a    = vec(v.(0)._0);
        auto b    = vec(v.(0)._1);
        ret tup(a + tl._0, b + tl._1);
    }
}

fn or(&vec[bool] v) -> bool {
    auto f = orb;
    ret _vec::foldl[bool, bool](f, false, v);
}

fn clone[T](&vec[T] v) -> vec[T] {
    ret slice[T](v, 0u, len[T](v));
}

fn plus_option[T](&vec[T] v, &option::t[T] o) -> () {
    alt (o) {
        case (none[T]) {}
        case (some[T](?x)) { v += vec(x); }
    }
}

fn cat_options[T](&vec[option::t[T]] v) -> vec[T] {
    let vec[T] res = vec();

    for (option::t[T] o in v) {
        alt (o) {
            case (none[T]) { }
            case (some[T](?t)) {
                res += vec(t);
            }
        }
    }

    ret res;
}

// TODO: Remove in favor of built-in "freeze" operation when it's implemented.
fn freeze[T](vec[mutable T] v) -> vec[T] {
    let vec[T] result = vec();
    for (T elem in v) {
        result += vec(elem);
    }
    ret result;
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
