use crate::spec::{Target, TargetOptions};

pub fn target() -> Target {
    Target {
        llvm_target: "armv6-unknown-freebsd-gnueabihf".to_string(),
        pointer_width: 32,
        data_layout: "e-m:e-p:32:32-Fi8-i64:64-v128:64:128-a:0:32-n32-S64".to_string(),
        arch: "arm".to_string(),
        options: TargetOptions {
            abi: "eabihf".to_string(),
            // FIXME: change env to "gnu" when cfg_target_abi becomes stable
            env: "gnueabihf".to_string(),
            features: "+v6,+vfp2,-d32".to_string(),
            max_atomic_width: Some(64),
            mcount: "\u{1}__gnu_mcount_nc".to_string(),
            ..super::freebsd_base::opts()
        },
    }
}
