// Regresion test for issue #1448 and #1386

fn main() {
    debug!{"%u", 10i}; //~ ERROR mismatched types
}
