fn main(vec[str] args) {
  let vec[str] vs = vec("hi", "there", "this", "is", "a", "vec");
  let vec[vec[str]] vvs = vec(args, vs);
  for (vec[str] vs in vvs) {
    for (str s in vs) {
      log s;
    }
  }
}
