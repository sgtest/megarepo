


// -*- rust -*-
tag clam[T] { a(T, int); b; }

fn uhoh[T](v: vec[clam[T]]) {
    alt v.(1) {
      a[T](t, u) { log "incorrect"; log u; fail; }
      b[T]. { log "correct"; }
    }
}

fn main() {
    let v: vec[clam[int]] = [b[int], b[int], a[int](42, 17)];
    uhoh[int](v);
}