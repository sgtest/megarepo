// xfail-stage0
// xfail-stage1
// error-pattern: attempted dynamic environment-capture
obj foo(int x) {
    fn mth() {
        fn bar() {
            log x;
        }
    }
}

fn main() {
  foo(2);
}
