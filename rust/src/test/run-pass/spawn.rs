// xfail-stage0
// -*- rust -*-

fn main() {
  auto t = spawn child(10);
}

fn child(int i) {
   log_err i;
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
