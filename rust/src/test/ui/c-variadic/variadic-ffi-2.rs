// ignore-arm stdcall isn't supported
#![feature(extended_varargs_abi_support)]
#![feature(abi_efiapi)]

fn baz(f: extern "stdcall" fn(usize, ...)) {
    //~^ ERROR: C-variadic function must have a compatible calling convention,
    // like C, cdecl, win64, sysv64 or efiapi
    f(22, 44);
}

fn sysv(f: extern "sysv64" fn(usize, ...)) {
    f(22, 44);
}
fn win(f: extern "win64" fn(usize, ...)) {
    f(22, 44);
}
fn efiapi(f: extern "efiapi" fn(usize, ...)) {
    f(22, 44);
}

fn main() {}
