import comm::port;
import comm::chan;
import comm::send;
import comm::recv;
import future::future;

export map, mapi, alli, any;

#[doc="The maximum number of tasks this module will spawn for a single
operationg."]
const max_tasks : uint = 32u;

#[doc="The minimum number of elements each task will process."]
const min_granularity : uint = 1024u;

#[doc="An internal helper to map a function over a large vector and
return the intermediate results.

This is used to build most of the other parallel vector functions,
like map or alli."]
fn map_slices<A: copy send, B: copy send>(xs: [A],
                                          f: fn~(uint, [const A]/&) -> B)
    -> [B] {

    let len = xs.len();
    if len < min_granularity {
        log(info, "small slice");
        // This is a small vector, fall back on the normal map.
        [f(0u, xs)]
    }
    else {
        let num_tasks = uint::min(max_tasks, len / min_granularity);

        let items_per_task = len / num_tasks;

        let mut futures = [];
        let mut base = 0u;
        log(info, "spawning tasks");
        while base < len {
            let end = uint::min(len, base + items_per_task);
            // FIXME: why is the ::<A, ()> annotation required here?
            vec::unpack_slice::<A, ()>(xs) {|p, _len|
                let f = ptr::addr_of(f);
                futures += [future::spawn() {|copy base|
                    unsafe {
                        let len = end - base;
                        let slice = (ptr::offset(p, base),
                                     len * sys::size_of::<A>());
                        log(info, #fmt("pre-slice: %?", (base, slice)));
                        let slice : [const A]/& =
                            unsafe::reinterpret_cast(slice);
                        log(info, #fmt("slice: %?",
                                       (base, vec::len(slice), end - base)));
                        assert(vec::len(slice) == end - base);
                        (*f)(base, slice)
                    }
                }];
            };
            base += items_per_task;
        }
        log(info, "tasks spawned");

        log(info, #fmt("num_tasks: %?", (num_tasks, futures.len())));
        assert(num_tasks == futures.len());

        let r = futures.map() {|ys|
            ys.get()
        };
        assert(r.len() == futures.len());
        r
    }
}

#[doc="A parallel version of map."]
fn map<A: copy send, B: copy send>(xs: [A], f: fn~(A) -> B) -> [B] {
    vec::concat(map_slices(xs) {|_base, slice|
        vec::map(slice, f)
    })
}

#[doc="A parallel version of mapi."]
fn mapi<A: copy send, B: copy send>(xs: [A], f: fn~(uint, A) -> B) -> [B] {
    let slices = map_slices(xs) {|base, slice|
        vec::mapi(slice) {|i, x|
            f(i + base, x)
        }
    };
    let r = vec::concat(slices);
    log(info, (r.len(), xs.len()));
    assert(r.len() == xs.len());
    r
}

#[doc="Returns true if the function holds for all elements in the vector."]
fn alli<A: copy send>(xs: [A], f: fn~(uint, A) -> bool) -> bool {
    vec::all(map_slices(xs) {|base, slice|
        vec::alli(slice) {|i, x|
            f(i + base, x)
        }
    }) {|x| x }
}

#[doc="Returns true if the function holds for any elements in the vector."]
fn any<A: copy send>(xs: [A], f: fn~(A) -> bool) -> bool {
    vec::any(map_slices(xs) {|_base, slice|
        vec::any(slice, f)
    }) {|x| x }
}
