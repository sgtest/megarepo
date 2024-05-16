// Verify that our intrinsics generate the correct LLVM calls for f16

#![crate_type = "lib"]
#![feature(f128)]
#![feature(f16)]
#![feature(core_intrinsics)]

/* arithmetic */

// CHECK-LABEL: i1 @f16_eq(
#[no_mangle]
pub fn f16_eq(a: f16, b: f16) -> bool {
    // CHECK: fcmp oeq half %{{.+}}, %{{.+}}
    a == b
}

// CHECK-LABEL: i1 @f16_ne(
#[no_mangle]
pub fn f16_ne(a: f16, b: f16) -> bool {
    // CHECK: fcmp une half %{{.+}}, %{{.+}}
    a != b
}

// CHECK-LABEL: i1 @f16_gt(
#[no_mangle]
pub fn f16_gt(a: f16, b: f16) -> bool {
    // CHECK: fcmp ogt half %{{.+}}, %{{.+}}
    a > b
}

// CHECK-LABEL: i1 @f16_ge(
#[no_mangle]
pub fn f16_ge(a: f16, b: f16) -> bool {
    // CHECK: fcmp oge half %{{.+}}, %{{.+}}
    a >= b
}

// CHECK-LABEL: i1 @f16_lt(
#[no_mangle]
pub fn f16_lt(a: f16, b: f16) -> bool {
    // CHECK: fcmp olt half %{{.+}}, %{{.+}}
    a < b
}

// CHECK-LABEL: i1 @f16_le(
#[no_mangle]
pub fn f16_le(a: f16, b: f16) -> bool {
    // CHECK: fcmp ole half %{{.+}}, %{{.+}}
    a <= b
}

// CHECK-LABEL: half @f16_neg(
#[no_mangle]
pub fn f16_neg(a: f16) -> f16 {
    // CHECK: fneg half %{{.+}}
    -a
}

// CHECK-LABEL: half @f16_add(
#[no_mangle]
pub fn f16_add(a: f16, b: f16) -> f16 {
    // CHECK: fadd half %{{.+}}, %{{.+}}
    a + b
}

// CHECK-LABEL: half @f16_sub(
#[no_mangle]
pub fn f16_sub(a: f16, b: f16) -> f16 {
    // CHECK: fsub half %{{.+}}, %{{.+}}
    a - b
}

// CHECK-LABEL: half @f16_mul(
#[no_mangle]
pub fn f16_mul(a: f16, b: f16) -> f16 {
    // CHECK: fmul half %{{.+}}, %{{.+}}
    a * b
}

// CHECK-LABEL: half @f16_div(
#[no_mangle]
pub fn f16_div(a: f16, b: f16) -> f16 {
    // CHECK: fdiv half %{{.+}}, %{{.+}}
    a / b
}

// CHECK-LABEL: half @f16_rem(
#[no_mangle]
pub fn f16_rem(a: f16, b: f16) -> f16 {
    // CHECK: frem half %{{.+}}, %{{.+}}
    a % b
}

// CHECK-LABEL: void @f16_add_assign(
#[no_mangle]
pub fn f16_add_assign(a: &mut f16, b: f16) {
    // CHECK: fadd half %{{.+}}, %{{.+}}
    // CHECK-NEXT: store half %{{.+}}, ptr %{{.+}}
    *a += b;
}

// CHECK-LABEL: void @f16_sub_assign(
#[no_mangle]
pub fn f16_sub_assign(a: &mut f16, b: f16) {
    // CHECK: fsub half %{{.+}}, %{{.+}}
    // CHECK-NEXT: store half %{{.+}}, ptr %{{.+}}
    *a -= b;
}

// CHECK-LABEL: void @f16_mul_assign(
#[no_mangle]
pub fn f16_mul_assign(a: &mut f16, b: f16) {
    // CHECK: fmul half %{{.+}}, %{{.+}}
    // CHECK-NEXT: store half %{{.+}}, ptr %{{.+}}
    *a *= b;
}

// CHECK-LABEL: void @f16_div_assign(
#[no_mangle]
pub fn f16_div_assign(a: &mut f16, b: f16) {
    // CHECK: fdiv half %{{.+}}, %{{.+}}
    // CHECK-NEXT: store half %{{.+}}, ptr %{{.+}}
    *a /= b;
}

// CHECK-LABEL: void @f16_rem_assign(
#[no_mangle]
pub fn f16_rem_assign(a: &mut f16, b: f16) {
    // CHECK: frem half %{{.+}}, %{{.+}}
    // CHECK-NEXT: store half %{{.+}}, ptr %{{.+}}
    *a %= b;
}

/* float to float conversions */

// CHECK-LABEL: half @f16_as_self(
#[no_mangle]
pub fn f16_as_self(a: f16) -> f16 {
    // CHECK: ret half %{{.+}}
    a as f16
}

// CHECK-LABEL: float @f16_as_f32(
#[no_mangle]
pub fn f16_as_f32(a: f16) -> f32 {
    // CHECK: fpext half %{{.+}} to float
    a as f32
}

// CHECK-LABEL: double @f16_as_f64(
#[no_mangle]
pub fn f16_as_f64(a: f16) -> f64 {
    // CHECK: fpext half %{{.+}} to double
    a as f64
}

// CHECK-LABEL: fp128 @f16_as_f128(
#[no_mangle]
pub fn f16_as_f128(a: f16) -> f128 {
    // CHECK: fpext half %{{.+}} to fp128
    a as f128
}

// CHECK-LABEL: half @f32_as_f16(
#[no_mangle]
pub fn f32_as_f16(a: f32) -> f16 {
    // CHECK: fptrunc float %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @f64_as_f16(
#[no_mangle]
pub fn f64_as_f16(a: f64) -> f16 {
    // CHECK: fptrunc double %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @f128_as_f16(
#[no_mangle]
pub fn f128_as_f16(a: f128) -> f16 {
    // CHECK: fptrunc fp128 %{{.+}} to half
    a as f16
}

/* float to int conversions */

// CHECK-LABEL: i8 @f16_as_u8(
#[no_mangle]
pub fn f16_as_u8(a: f16) -> u8 {
    // CHECK: call i8 @llvm.fptoui.sat.i8.f16(half %{{.+}})
    a as u8
}

#[no_mangle]
pub fn f16_as_u16(a: f16) -> u16 {
    // CHECK: call i16 @llvm.fptoui.sat.i16.f16(half %{{.+}})
    a as u16
}

// CHECK-LABEL: i32 @f16_as_u32(
#[no_mangle]
pub fn f16_as_u32(a: f16) -> u32 {
    // CHECK: call i32 @llvm.fptoui.sat.i32.f16(half %{{.+}})
    a as u32
}

// CHECK-LABEL: i64 @f16_as_u64(
#[no_mangle]
pub fn f16_as_u64(a: f16) -> u64 {
    // CHECK: call i64 @llvm.fptoui.sat.i64.f16(half %{{.+}})
    a as u64
}

// CHECK-LABEL: i128 @f16_as_u128(
#[no_mangle]
pub fn f16_as_u128(a: f16) -> u128 {
    // CHECK: call i128 @llvm.fptoui.sat.i128.f16(half %{{.+}})
    a as u128
}

// CHECK-LABEL: i8 @f16_as_i8(
#[no_mangle]
pub fn f16_as_i8(a: f16) -> i8 {
    // CHECK: call i8 @llvm.fptosi.sat.i8.f16(half %{{.+}})
    a as i8
}

// CHECK-LABEL: i16 @f16_as_i16(
#[no_mangle]
pub fn f16_as_i16(a: f16) -> i16 {
    // CHECK: call i16 @llvm.fptosi.sat.i16.f16(half %{{.+}})
    a as i16
}
// CHECK-LABEL: i32 @f16_as_i32(
#[no_mangle]
pub fn f16_as_i32(a: f16) -> i32 {
    // CHECK: call i32 @llvm.fptosi.sat.i32.f16(half %{{.+}})
    a as i32
}

// CHECK-LABEL: i64 @f16_as_i64(
#[no_mangle]
pub fn f16_as_i64(a: f16) -> i64 {
    // CHECK: call i64 @llvm.fptosi.sat.i64.f16(half %{{.+}})
    a as i64
}

// CHECK-LABEL: i128 @f16_as_i128(
#[no_mangle]
pub fn f16_as_i128(a: f16) -> i128 {
    // CHECK: call i128 @llvm.fptosi.sat.i128.f16(half %{{.+}})
    a as i128
}

/* int to float conversions */

// CHECK-LABEL: half @u8_as_f16(
#[no_mangle]
pub fn u8_as_f16(a: u8) -> f16 {
    // CHECK: uitofp i8 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @u16_as_f16(
#[no_mangle]
pub fn u16_as_f16(a: u16) -> f16 {
    // CHECK: uitofp i16 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @u32_as_f16(
#[no_mangle]
pub fn u32_as_f16(a: u32) -> f16 {
    // CHECK: uitofp i32 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @u64_as_f16(
#[no_mangle]
pub fn u64_as_f16(a: u64) -> f16 {
    // CHECK: uitofp i64 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @u128_as_f16(
#[no_mangle]
pub fn u128_as_f16(a: u128) -> f16 {
    // CHECK: uitofp i128 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @i8_as_f16(
#[no_mangle]
pub fn i8_as_f16(a: i8) -> f16 {
    // CHECK: sitofp i8 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @i16_as_f16(
#[no_mangle]
pub fn i16_as_f16(a: i16) -> f16 {
    // CHECK: sitofp i16 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @i32_as_f16(
#[no_mangle]
pub fn i32_as_f16(a: i32) -> f16 {
    // CHECK: sitofp i32 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @i64_as_f16(
#[no_mangle]
pub fn i64_as_f16(a: i64) -> f16 {
    // CHECK: sitofp i64 %{{.+}} to half
    a as f16
}

// CHECK-LABEL: half @i128_as_f16(
#[no_mangle]
pub fn i128_as_f16(a: i128) -> f16 {
    // CHECK: sitofp i128 %{{.+}} to half
    a as f16
}
