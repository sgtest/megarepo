// xfail-boot
fn main() {
  auto pi = 3.1415927;

  log(-pi * (pi + (2.0/pi)) - (pi * 5.0));
  if(pi == 5.0 || pi < 10.0 || pi <= 2.0 ||
     pi != 22.0/7.0 || pi >= 10.0 || pi > 1.0) {
    log("yes");
  }
}