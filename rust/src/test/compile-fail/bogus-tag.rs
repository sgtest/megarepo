// -*- rust -*-

// error-pattern: unresolved

enum color { rgb(int, int, int), rgba(int, int, int, int), }

fn main() {
    let red: color = rgb(255, 0, 0);
    match red {
      rgb(r, g, b) => { debug!("rgb"); }
      hsl(h, s, l) => { debug!("hsl"); }
    }
}

