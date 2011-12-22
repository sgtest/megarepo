


// -*- rust -*-
tag color {
    rgb(int, int, int);
    rgba(int, int, int, int);
    hsl(int, int, int);
}

fn process(c: color) -> int {
    let x: int;
    alt c {
      rgb(r, _, _) { #debug("rgb"); log_full(core::debug, r); x = r; }
      rgba(_, _, _, a) { #debug("rgba"); log_full(core::debug, a); x = a; }
      hsl(_, s, _) { #debug("hsl"); log_full(core::debug, s); x = s; }
    }
    ret x;
}

fn main() {
    let gray: color = rgb(127, 127, 127);
    let clear: color = rgba(50, 150, 250, 0);
    let red: color = hsl(0, 255, 255);
    assert (process(gray) == 127);
    assert (process(clear) == 0);
    assert (process(red) == 255);
}
