fn main () {
  let mut line = "";
  let mut i = 0;
  do {
    line = if i == 9 { "exit" } else { "notexit" };
    i += 1;
  } while line != "exit";
}
