fn main() {
    let f: fn#() = fn# () {
        log "This is a bare function"
    };
    let g;
    g = f;
}