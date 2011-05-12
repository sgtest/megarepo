native "rust" mod rustrt {
  fn rust_file_is_dir(str path) -> int;
}

fn path_sep() -> str {
    ret _str::from_char(os_fs::path_sep);
}

type path = str;

fn dirname(path p) -> path {
    let int i = _str::rindex(p, os_fs::path_sep as u8);
    if (i == -1) {
        i = _str::rindex(p, os_fs::alt_path_sep as u8);
        if (i == -1) {
            ret p;
        }
    }
    ret _str::substr(p, 0u, i as uint);
}

fn connect(path pre, path post) -> path {
    auto len = _str::byte_len(pre);
    if (pre.(len - 1u) == (os_fs::path_sep as u8)) { // Trailing '/'?
        ret pre + post;
    }
    ret pre + path_sep() + post;
}

fn file_is_dir(path p) -> bool {
  ret rustrt::rust_file_is_dir(p) != 0;
}

fn list_dir(path p) -> vec[str] {
  auto pl = _str::byte_len(p);
  if (pl == 0u || p.(pl - 1u) as char != os_fs::path_sep) {
    p += path_sep();
  }
  let vec[str] full_paths = vec();
  for (str filename in os_fs::list_dir(p)) {
    if (!_str::eq(filename, ".")) {if (!_str::eq(filename, "..")) {
      _vec::push[str](full_paths, p + filename);
    }}
  }
  ret full_paths;
}


// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
