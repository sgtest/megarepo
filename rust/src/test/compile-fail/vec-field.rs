// xfail-stage0
// error-pattern:attempted field access on type vec\[int\]
// issue #367

fn f() {
  auto v = [1];
  log v.some_field_name; //type error
}

fn main() {}