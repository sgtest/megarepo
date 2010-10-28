
import util.option;
import util.some;
import util.none;

// FIXME: It would probably be more appealing to define this as
// type list[T] = rec(T hd, option[@list[T]] tl), but at the moment
// our recursion rules do not permit that.

tag list[T] {
    cons(T, @list[T]);
    nil;
}

fn foldl[T,U](&list[T] ls, U u, fn(&T t, U u) -> U f) -> U {
  alt(ls) {
    case (cons[T](?hd, ?tl)) {
      auto u_ = f(hd, u);
      // FIXME: should use 'be' here, not 'ret'. But parametric
      // tail calls currently don't work.
      ret foldl[T,U](*tl, u_, f);
    }
    case (nil[T]) {
      ret u;
    }
  }
}

fn find[T,U](&list[T] ls,
             (fn(&T) -> option[U]) f) -> option[U] {
  alt(ls) {
    case (cons[T](?hd, ?tl)) {
        alt (f(hd)) {
            case (none[U]) {
                // FIXME: should use 'be' here, not 'ret'. But parametric tail
                // calls currently don't work.
                ret find[T,U](*tl, f);
            }
            case (some[U](?res)) {
                ret some[U](res);
            }
        }
    }
    case (nil[T]) {
        ret none[U];
    }
  }
}

fn length[T](&list[T] ls) -> uint {
  fn count[T](&T t, uint u) -> uint {
    ret u + 1u;
  }
  ret foldl[T,uint](ls, 0u, bind count[T](_, _));
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
