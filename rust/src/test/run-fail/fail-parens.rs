// xfail-test
// Fail statements without arguments need to be disambiguated in
// certain positions
// error-pattern:explicit-failure

fn bigfail() {
    do { while (fail) { if (fail) {
        match (fail) { _ {
        }}
    }}} while fail;
}

fn main() { bigfail(); }
