// check-fail

#![feature(ptr_from_ref)]

extern "C" {
    // N.B., mutability can be easily incorrect in FFI calls -- as
    // in C, the default is mutable pointers.
    fn ffi(c: *mut u8);
    fn int_ffi(c: *mut i32);
}

unsafe fn ref_to_mut() {
    let num = &3i32;

    let _num = &mut *(num as *const i32 as *mut i32);
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior
    let _num = &mut *(num as *const i32).cast_mut();
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior
    let _num = &mut *std::ptr::from_ref(num).cast_mut();
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior
    let _num = &mut *std::ptr::from_ref({ num }).cast_mut();
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior
    let _num = &mut *{ std::ptr::from_ref(num) }.cast_mut();
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior
    let _num = &mut *(std::ptr::from_ref({ num }) as *mut i32);
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior

    let deferred = num as *const i32 as *mut i32;
    let _num = &mut *deferred;
    //~^ ERROR casting `&T` to `&mut T` is undefined behavior
}

unsafe fn assign_to_ref() {
    let s = String::from("Hello");
    let a = &s;
    let num = &3i32;

    *(a as *const _ as *mut _) = String::from("Replaced");
    //~^ ERROR assigning to `&T` is undefined behavior
    *(a as *const _ as *mut String) += " world";
    //~^ ERROR assigning to `&T` is undefined behavior
    *std::ptr::from_ref(num).cast_mut() += 1;
    //~^ ERROR assigning to `&T` is undefined behavior
    *std::ptr::from_ref({ num }).cast_mut() += 1;
    //~^ ERROR assigning to `&T` is undefined behavior
    *{ std::ptr::from_ref(num) }.cast_mut() += 1;
    //~^ ERROR assigning to `&T` is undefined behavior
    *(std::ptr::from_ref({ num }) as *mut i32) += 1;
    //~^ ERROR assigning to `&T` is undefined behavior
    let value = num as *const i32 as *mut i32;
    *value = 1;
    //~^ ERROR assigning to `&T` is undefined behavior
}

unsafe fn no_warn() {
    let num = &3i32;
    let mut_num = &mut 3i32;
    let a = &String::from("ffi");

    *(num as *const i32 as *mut i32);
    println!("{}", *(num as *const _ as *const i16));
    println!("{}", *(mut_num as *mut _ as *mut i16));
    ffi(a.as_ptr() as *mut _);
    int_ffi(num as *const _ as *mut _);
    int_ffi(&3 as *const _ as *mut _);
    let mut value = 3;
    let value: *const i32 = &mut value;
    *(value as *const i16 as *mut i16) = 42;
}

fn main() {}
