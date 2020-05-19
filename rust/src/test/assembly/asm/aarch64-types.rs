// no-system-llvm
// assembly-output: emit-asm
// compile-flags: --target aarch64-unknown-linux-gnu

#![feature(no_core, lang_items, rustc_attrs, repr_simd)]
#![crate_type = "rlib"]
#![no_core]
#![allow(asm_sub_register, non_camel_case_types)]

#[rustc_builtin_macro]
macro_rules! asm {
    () => {};
}
#[rustc_builtin_macro]
macro_rules! concat {
    () => {};
}
#[rustc_builtin_macro]
macro_rules! stringify {
    () => {};
}

#[lang = "sized"]
trait Sized {}
#[lang = "copy"]
trait Copy {}

type ptr = *mut u8;

#[repr(simd)]
pub struct i8x8(i8, i8, i8, i8, i8, i8, i8, i8);
#[repr(simd)]
pub struct i16x4(i16, i16, i16, i16);
#[repr(simd)]
pub struct i32x2(i32, i32);
#[repr(simd)]
pub struct i64x1(i64);
#[repr(simd)]
pub struct f32x2(f32, f32);
#[repr(simd)]
pub struct f64x1(f64);
#[repr(simd)]
pub struct i8x16(i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8, i8);
#[repr(simd)]
pub struct i16x8(i16, i16, i16, i16, i16, i16, i16, i16);
#[repr(simd)]
pub struct i32x4(i32, i32, i32, i32);
#[repr(simd)]
pub struct i64x2(i64, i64);
#[repr(simd)]
pub struct f32x4(f32, f32, f32, f32);
#[repr(simd)]
pub struct f64x2(f64, f64);

impl Copy for i8 {}
impl Copy for i16 {}
impl Copy for i32 {}
impl Copy for f32 {}
impl Copy for i64 {}
impl Copy for f64 {}
impl Copy for ptr {}
impl Copy for i8x8 {}
impl Copy for i16x4 {}
impl Copy for i32x2 {}
impl Copy for i64x1 {}
impl Copy for f32x2 {}
impl Copy for f64x1 {}
impl Copy for i8x16 {}
impl Copy for i16x8 {}
impl Copy for i32x4 {}
impl Copy for i64x2 {}
impl Copy for f32x4 {}
impl Copy for f64x2 {}

extern "C" {
    fn extern_func();
    static extern_static: u8;
}

// CHECK-LABEL: sym_fn:
// CHECK: //APP
// CHECK: bl extern_func
// CHECK: //NO_APP
#[no_mangle]
pub unsafe fn sym_fn() {
    asm!("bl {}", sym extern_func);
}

// CHECK-LABEL: sym_static:
// CHECK: //APP
// CHECK: adr x0, extern_static
// CHECK: //NO_APP
#[no_mangle]
pub unsafe fn sym_static() {
    asm!("adr x0, {}", sym extern_static);
}

macro_rules! check {
    ($func:ident $ty:ident $class:ident $mov:literal $modifier:literal) => {
        #[no_mangle]
        pub unsafe fn $func(x: $ty) -> $ty {
            // Hack to avoid function merging
            extern "Rust" {
                fn dont_merge(s: &str);
            }
            dont_merge(stringify!($func));

            let y;
            asm!(
                concat!($mov, " {:", $modifier, "}, {:", $modifier, "}"),
                out($class) y,
                in($class) x
            );
            y
        }
    };
}

// CHECK-LABEL: reg_i8:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_i8 i8 reg "mov" "");

// CHECK-LABEL: reg_i16:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_i16 i16 reg "mov" "");

// CHECK-LABEL: reg_i32:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_i32 i32 reg "mov" "");

// CHECK-LABEL: reg_f32:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_f32 f32 reg "mov" "");

// CHECK-LABEL: reg_i64:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_i64 i64 reg "mov" "");

// CHECK-LABEL: reg_f64:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_f64 f64 reg "mov" "");

// CHECK-LABEL: reg_ptr:
// CHECK: //APP
// CHECK: mov x{{[0-9]+}}, x{{[0-9]+}}
// CHECK: //NO_APP
check!(reg_ptr ptr reg "mov" "");

// CHECK-LABEL: vreg_i8:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i8 i8 vreg "fmov" "s");

// CHECK-LABEL: vreg_i16:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i16 i16 vreg "fmov" "s");

// CHECK-LABEL: vreg_i32:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i32 i32 vreg "fmov" "s");

// CHECK-LABEL: vreg_f32:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_f32 f32 vreg "fmov" "s");

// CHECK-LABEL: vreg_i64:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i64 i64 vreg "fmov" "s");

// CHECK-LABEL: vreg_f64:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_f64 f64 vreg "fmov" "s");

// CHECK-LABEL: vreg_ptr:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_ptr ptr vreg "fmov" "s");

// CHECK-LABEL: vreg_i8x8:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i8x8 i8x8 vreg "fmov" "s");

// CHECK-LABEL: vreg_i16x4:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i16x4 i16x4 vreg "fmov" "s");

// CHECK-LABEL: vreg_i32x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i32x2 i32x2 vreg "fmov" "s");

// CHECK-LABEL: vreg_i64x1:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i64x1 i64x1 vreg "fmov" "s");

// CHECK-LABEL: vreg_f32x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_f32x2 f32x2 vreg "fmov" "s");

// CHECK-LABEL: vreg_f64x1:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_f64x1 f64x1 vreg "fmov" "s");

// CHECK-LABEL: vreg_i8x16:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i8x16 i8x16 vreg "fmov" "s");

// CHECK-LABEL: vreg_i16x8:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i16x8 i16x8 vreg "fmov" "s");

// CHECK-LABEL: vreg_i32x4:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i32x4 i32x4 vreg "fmov" "s");

// CHECK-LABEL: vreg_i64x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_i64x2 i64x2 vreg "fmov" "s");

// CHECK-LABEL: vreg_f32x4:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_f32x4 f32x4 vreg "fmov" "s");

// CHECK-LABEL: vreg_f64x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_f64x2 f64x2 vreg "fmov" "s");

// CHECK-LABEL: vreg_low16_i8:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i8 i8 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i16:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i16 i16 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_f32:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_f32 f32 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i64:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i64 i64 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_f64:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_f64 f64 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_ptr:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_ptr ptr vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i8x8:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i8x8 i8x8 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i16x4:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i16x4 i16x4 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i32x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i32x2 i32x2 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i64x1:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i64x1 i64x1 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_f32x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_f32x2 f32x2 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_f64x1:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_f64x1 f64x1 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i8x16:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i8x16 i8x16 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i16x8:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i16x8 i16x8 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i32x4:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i32x4 i32x4 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_i64x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_i64x2 i64x2 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_f32x4:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_f32x4 f32x4 vreg_low16 "fmov" "s");

// CHECK-LABEL: vreg_low16_f64x2:
// CHECK: //APP
// CHECK: fmov s{{[0-9]+}}, s{{[0-9]+}}
// CHECK: //NO_APP
check!(vreg_low16_f64x2 f64x2 vreg_low16 "fmov" "s");
