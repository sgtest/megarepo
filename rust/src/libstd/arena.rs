// Dynamic arenas.

export arena, arena_with_size;

import list;

type chunk = {data: [u8], mut fill: uint};
type arena = {mut chunks: list::list<@chunk>};

fn chunk(size: uint) -> @chunk {
    @{ data: vec::from_elem(size, 0u8), mut fill: 0u }
}

fn arena_with_size(initial_size: uint) -> arena {
    ret {mut chunks: list::cons(chunk(initial_size), @list::nil)};
}

fn arena() -> arena {
    arena_with_size(32u)
}

impl arena for arena {
    fn alloc(n_bytes: uint, align: uint) -> *() {
        let alignm1 = align - 1u;
        let mut head = list::head(self.chunks);

        let mut start = head.fill;
        start = (start + alignm1) & !alignm1;
        let mut end = start + n_bytes;

        if end > vec::len(head.data) {
            // Allocate a new chunk.
            let new_min_chunk_size = uint::max(n_bytes, vec::len(head.data));
            head = chunk(uint::next_power_of_two(new_min_chunk_size));
            self.chunks = list::cons(head, @self.chunks);
            start = 0u;
            end = n_bytes;
        }

        let p = ptr::offset(ptr::addr_of(head.fill), start);
        head.fill = end;
        unsafe { ret unsafe::reinterpret_cast(p); }
    }
}

