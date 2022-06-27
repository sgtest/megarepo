use crate::abi::Endian;
use crate::spec::{LinkerFlavor, Target, TargetOptions};

pub fn target() -> Target {
    let mut base = super::netbsd_base::opts();
    base.add_pre_link_args(LinkerFlavor::Gcc, &["-m32"]);
    base.max_atomic_width = Some(32);

    Target {
        llvm_target: "powerpc-unknown-netbsd".into(),
        pointer_width: 32,
        data_layout: "E-m:e-p:32:32-i64:64-n32".into(),
        arch: "powerpc".into(),
        options: TargetOptions { endian: Endian::Big, mcount: "__mcount".into(), ..base },
    }
}
