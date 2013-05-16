// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::to_bytes;

#[deriving(Eq)]
pub enum Abi {
    // NB: This ordering MUST match the AbiDatas array below.
    // (This is ensured by the test indices_are_correct().)

    // Single platform ABIs come first (`for_arch()` relies on this)
    Cdecl,
    Stdcall,
    Fastcall,
    Aapcs,

    // Multiplatform ABIs second
    Rust,
    C,
    RustIntrinsic,
}

#[deriving(Eq)]
pub enum Architecture {
    // NB. You cannot change the ordering of these
    // constants without adjusting IntelBits below.
    // (This is ensured by the test indices_are_correct().)
    X86,
    X86_64,
    Arm,
    Mips
}

static IntelBits: u32 = (1 << (X86 as uint)) | (1 << (X86_64 as uint));
static ArmBits: u32 = (1 << (Arm as uint));

struct AbiData {
    abi: Abi,

    // Name of this ABI as we like it called.
    name: &'static str,

    // Is it specific to a platform? If so, which one?  Also, what is
    // the name that LLVM gives it (in case we disagree)
    abi_arch: AbiArchitecture
}

enum AbiArchitecture {
    RustArch,   // Not a real ABI (e.g., intrinsic)
    AllArch,    // An ABI that specifies cross-platform defaults (e.g., "C")
    Archs(u32)  // Multiple architectures (bitset)
}

#[deriving(Eq, Encodable, Decodable)]
pub struct AbiSet {
    priv bits: u32   // each bit represents one of the abis below
}

static AbiDatas: &'static [AbiData] = &[
    // Platform-specific ABIs
    AbiData {abi: Cdecl, name: "cdecl", abi_arch: Archs(IntelBits)},
    AbiData {abi: Stdcall, name: "stdcall", abi_arch: Archs(IntelBits)},
    AbiData {abi: Fastcall, name:"fastcall", abi_arch: Archs(IntelBits)},
    AbiData {abi: Aapcs, name: "aapcs", abi_arch: Archs(ArmBits)},

    // Cross-platform ABIs
    //
    // NB: Do not adjust this ordering without
    // adjusting the indices below.
    AbiData {abi: Rust, name: "Rust", abi_arch: RustArch},
    AbiData {abi: C, name: "C", abi_arch: AllArch},
    AbiData {abi: RustIntrinsic, name: "rust-intrinsic", abi_arch: RustArch},
];

#[cfg(stage0)]
fn each_abi(op: &fn(abi: Abi) -> bool) {
    /*!
     *
     * Iterates through each of the defined ABIs.
     */

    for AbiDatas.each |abi_data| {
        if !op(abi_data.abi) {
            return;
        }
    }
}
#[cfg(not(stage0))]
fn each_abi(op: &fn(abi: Abi) -> bool) -> bool {
    /*!
     *
     * Iterates through each of the defined ABIs.
     */

    AbiDatas.each(|abi_data| op(abi_data.abi))
}

pub fn lookup(name: &str) -> Option<Abi> {
    /*!
     *
     * Returns the ABI with the given name (if any).
     */

    for each_abi |abi| {
        if name == abi.data().name {
            return Some(abi);
        }
    }
    return None;
}

pub fn all_names() -> ~[&'static str] {
    AbiDatas.map(|d| d.name)
}

pub impl Abi {
    #[inline]
    fn index(&self) -> uint {
        *self as uint
    }

    #[inline]
    fn data(&self) -> &'static AbiData {
        &AbiDatas[self.index()]
    }

    fn name(&self) -> &'static str {
        self.data().name
    }
}

impl Architecture {
    fn bit(&self) -> u32 {
        1 << (*self as u32)
    }
}

pub impl AbiSet {
    fn from(abi: Abi) -> AbiSet {
        AbiSet { bits: (1 << abi.index()) }
    }

    #[inline]
    fn Rust() -> AbiSet {
        AbiSet::from(Rust)
    }

    #[inline]
    fn C() -> AbiSet {
        AbiSet::from(C)
    }

    #[inline]
    fn Intrinsic() -> AbiSet {
        AbiSet::from(RustIntrinsic)
    }

    fn default() -> AbiSet {
        AbiSet::C()
    }

    fn empty() -> AbiSet {
        AbiSet { bits: 0 }
    }

    #[inline]
    fn is_rust(&self) -> bool {
        self.bits == 1 << Rust.index()
    }

    #[inline]
    fn is_c(&self) -> bool {
        self.bits == 1 << C.index()
    }

    #[inline]
    fn is_intrinsic(&self) -> bool {
        self.bits == 1 << RustIntrinsic.index()
    }

    fn contains(&self, abi: Abi) -> bool {
        (self.bits & (1 << abi.index())) != 0
    }

    fn subset_of(&self, other_abi_set: AbiSet) -> bool {
        (self.bits & other_abi_set.bits) == self.bits
    }

    fn add(&mut self, abi: Abi) {
        self.bits |= (1 << abi.index());
    }

    #[cfg(stage0)]
    fn each(&self, op: &fn(abi: Abi) -> bool) {
        for each_abi |abi| {
            if self.contains(abi) {
                if !op(abi) {
                    return;
                }
            }
        }
    }
    #[cfg(not(stage0))]
    fn each(&self, op: &fn(abi: Abi) -> bool) -> bool {
        each_abi(|abi| !self.contains(abi) || op(abi))
    }

    fn is_empty(&self) -> bool {
        self.bits == 0
    }

    fn for_arch(&self, arch: Architecture) -> Option<Abi> {
        // NB---Single platform ABIs come first
        for self.each |abi| {
            let data = abi.data();
            match data.abi_arch {
                Archs(a) if (a & arch.bit()) != 0 => { return Some(abi); }
                Archs(_) => { }
                RustArch | AllArch => { return Some(abi); }
            }
        }

        None
    }

    fn check_valid(&self) -> Option<(Abi, Abi)> {
        let mut abis = ~[];
        for self.each |abi| { abis.push(abi); }

        for abis.eachi |i, abi| {
            let data = abi.data();
            for abis.slice(0, i).each |other_abi| {
                let other_data = other_abi.data();
                debug!("abis=(%?,%?) datas=(%?,%?)",
                       abi, data.abi_arch,
                       other_abi, other_data.abi_arch);
                match (&data.abi_arch, &other_data.abi_arch) {
                    (&AllArch, &AllArch) => {
                        // Two cross-architecture ABIs
                        return Some((*abi, *other_abi));
                    }
                    (_, &RustArch) |
                    (&RustArch, _) => {
                        // Cannot combine Rust or Rust-Intrinsic with
                        // anything else.
                        return Some((*abi, *other_abi));
                    }
                    (&Archs(is), &Archs(js)) if (is & js) != 0 => {
                        // Two ABIs for same architecture
                        return Some((*abi, *other_abi));
                    }
                    _ => {}
                }
            }
        }

        return None;
    }
}

#[cfg(stage0)]
impl to_bytes::IterBytes for Abi {
    fn iter_bytes(&self, lsb0: bool, f: to_bytes::Cb) {
        self.index().iter_bytes(lsb0, f)
    }
}
#[cfg(not(stage0))]
impl to_bytes::IterBytes for Abi {
    fn iter_bytes(&self, lsb0: bool, f: to_bytes::Cb) -> bool {
        self.index().iter_bytes(lsb0, f)
    }
}

#[cfg(stage0)]
impl to_bytes::IterBytes for AbiSet {
    fn iter_bytes(&self, lsb0: bool, f: to_bytes::Cb) {
        self.bits.iter_bytes(lsb0, f)
    }
}
#[cfg(not(stage0))]
impl to_bytes::IterBytes for AbiSet {
    fn iter_bytes(&self, lsb0: bool, f: to_bytes::Cb) -> bool {
        self.bits.iter_bytes(lsb0, f)
    }
}

impl ToStr for Abi {
    fn to_str(&self) -> ~str {
        self.data().name.to_str()
    }
}

impl ToStr for AbiSet {
    fn to_str(&self) -> ~str {
        let mut strs = ~[];
        for self.each |abi| {
            strs.push(abi.data().name);
        }
        fmt!("\"%s\"", str::connect_slices(strs, " "))
    }
}

#[test]
fn lookup_Rust() {
    let abi = lookup("Rust");
    assert!(abi.is_some() && abi.get().data().name == "Rust");
}

#[test]
fn lookup_cdecl() {
    let abi = lookup("cdecl");
    assert!(abi.is_some() && abi.get().data().name == "cdecl");
}

#[test]
fn lookup_baz() {
    let abi = lookup("baz");
    assert!(abi.is_none());
}

#[cfg(test)]
fn cannot_combine(n: Abi, m: Abi) {
    let mut set = AbiSet::empty();
    set.add(n);
    set.add(m);
    match set.check_valid() {
        Some((a, b)) => {
            assert!((n == a && m == b) ||
                         (m == a && n == b));
        }
        None => {
            fail!("Invalid match not detected");
        }
    }
}

#[cfg(test)]
fn can_combine(n: Abi, m: Abi) {
    let mut set = AbiSet::empty();
    set.add(n);
    set.add(m);
    match set.check_valid() {
        Some((_, _)) => {
            fail!("Valid match declared invalid");
        }
        None => {}
    }
}

#[test]
fn cannot_combine_cdecl_and_stdcall() {
    cannot_combine(Cdecl, Stdcall);
}

#[test]
fn cannot_combine_c_and_rust() {
    cannot_combine(C, Rust);
}

#[test]
fn cannot_combine_rust_and_cdecl() {
    cannot_combine(Rust, Cdecl);
}

#[test]
fn cannot_combine_rust_intrinsic_and_cdecl() {
    cannot_combine(RustIntrinsic, Cdecl);
}

#[test]
fn can_combine_c_and_stdcall() {
    can_combine(C, Stdcall);
}

#[test]
fn can_combine_aapcs_and_stdcall() {
    can_combine(Aapcs, Stdcall);
}

#[test]
fn abi_to_str_stdcall_aaps() {
    let mut set = AbiSet::empty();
    set.add(Aapcs);
    set.add(Stdcall);
    assert!(set.to_str() == ~"\"stdcall aapcs\"");
}

#[test]
fn abi_to_str_c_aaps() {
    let mut set = AbiSet::empty();
    set.add(Aapcs);
    set.add(C);
    debug!("set = %s", set.to_str());
    assert!(set.to_str() == ~"\"aapcs C\"");
}

#[test]
fn abi_to_str_rust() {
    let mut set = AbiSet::empty();
    set.add(Rust);
    debug!("set = %s", set.to_str());
    assert!(set.to_str() == ~"\"Rust\"");
}

#[test]
fn indices_are_correct() {
    for AbiDatas.eachi |i, abi_data| {
        assert!(i == abi_data.abi.index());
    }

    let bits = 1 << (X86 as u32);
    let bits = bits | 1 << (X86_64 as u32);
    assert!(IntelBits == bits);

    let bits = 1 << (Arm as u32);
    assert!(ArmBits == bits);
}

#[cfg(test)]
fn check_arch(abis: &[Abi], arch: Architecture, expect: Option<Abi>) {
    let mut set = AbiSet::empty();
    for abis.each |&abi| {
        set.add(abi);
    }
    let r = set.for_arch(arch);
    assert!(r == expect);
}

#[test]
fn pick_multiplatform() {
    check_arch([C, Cdecl], X86, Some(Cdecl));
    check_arch([C, Cdecl], X86_64, Some(Cdecl));
    check_arch([C, Cdecl], Arm, Some(C));
}

#[test]
fn pick_uniplatform() {
    check_arch([Stdcall], X86, Some(Stdcall));
    check_arch([Stdcall], Arm, None);
}
