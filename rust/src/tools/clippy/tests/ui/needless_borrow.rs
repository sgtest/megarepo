// run-rustfix

#[warn(clippy::all, clippy::needless_borrow)]
#[allow(unused_variables, clippy::unnecessary_mut_passed)]
fn main() {
    let a = 5;
    let ref_a = &a;
    let _ = x(&a); // no warning
    let _ = x(&&a); // warn

    let mut b = 5;
    mut_ref(&mut b); // no warning
    mut_ref(&mut &mut b); // warn

    let s = &String::from("hi");
    let s_ident = f(&s); // should not error, because `&String` implements Copy, but `String` does not
    let g_val = g(&Vec::new()); // should not error, because `&Vec<T>` derefs to `&[T]`
    let vec = Vec::new();
    let vec_val = g(&vec); // should not error, because `&Vec<T>` derefs to `&[T]`
    h(&"foo"); // should not error, because the `&&str` is required, due to `&Trait`
    let garbl = match 42 {
        44 => &a,
        45 => {
            println!("foo");
            &&a
        },
        46 => &&a,
        47 => {
            println!("foo");
            loop {
                println!("{}", a);
                if a == 25 {
                    break &ref_a;
                }
            }
        },
        _ => panic!(),
    };

    let _ = x(&&&a);
    let _ = x(&mut &&a);
    let _ = x(&&&mut b);
    let _ = x(&&ref_a);
    {
        let b = &mut b;
        x(&b);
    }

    // Issue #8191
    let mut x = 5;
    let mut x = &mut x;

    mut_ref(&mut x);
    mut_ref(&mut &mut x);
    let y: &mut i32 = &mut x;
    let y: &mut i32 = &mut &mut x;

    let y = match 0 {
        // Don't lint. Removing the borrow would move 'x'
        0 => &mut x,
        _ => &mut *x,
    };

    *x = 5;

    let s = String::new();
    // let _ = (&s).len();
    // let _ = (&s).capacity();
    // let _ = (&&s).capacity();

    let x = (1, 2);
    let _ = (&x).0;
    let x = &x as *const (i32, i32);
    let _ = unsafe { (&*x).0 };
}

#[allow(clippy::needless_borrowed_reference)]
fn x(y: &i32) -> i32 {
    *y
}

fn mut_ref(y: &mut i32) {
    *y = 5;
}

fn f<T: Copy>(y: &T) -> T {
    *y
}

fn g(y: &[u8]) -> u8 {
    y[0]
}

trait Trait {}

impl<'a> Trait for &'a str {}

fn h(_: &dyn Trait) {}
