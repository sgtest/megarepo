// xfail-boot
// xfail-stage0
// error-pattern: unresolved modulename
import main::bar;

fn main(vec[str] args) {
  log "foo";
}
