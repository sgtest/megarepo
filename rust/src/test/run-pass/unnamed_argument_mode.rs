fn good(_a: &int) {
}

// unnamed argument &int is now parse x: &int

fn called(_f: |&int|) {
}

pub fn main() {
    called(good);
}
