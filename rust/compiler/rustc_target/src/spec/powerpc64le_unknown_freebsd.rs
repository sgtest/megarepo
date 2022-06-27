use crate::spec::{LinkerFlavor, Target, TargetOptions};

pub fn target() -> Target {
    let mut base = super::freebsd_base::opts();
    base.cpu = "ppc64le".into();
    base.add_pre_link_args(LinkerFlavor::Gcc, &["-m64"]);
    base.max_atomic_width = Some(64);

    Target {
        llvm_target: "powerpc64le-unknown-freebsd".into(),
        pointer_width: 64,
        data_layout: "e-m:e-i64:64-n32:64".into(),
        arch: "powerpc64".into(),
        options: TargetOptions { mcount: "_mcount".into(), ..base },
    }
}
