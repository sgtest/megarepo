use crate::spec::{LinkerFlavor, LldFlavor, SplitDebuginfo, TargetOptions};

pub fn opts() -> TargetOptions {
    // Suppress the verbose logo and authorship debugging output, which would needlessly
    // clog any log files.
    let pre_link_args = TargetOptions::link_args(LinkerFlavor::Msvc, &["/NOLOGO"]);

    TargetOptions {
        linker_flavor: LinkerFlavor::Msvc,
        is_like_windows: true,
        is_like_msvc: true,
        lld_flavor: LldFlavor::Link,
        linker_is_gnu: false,
        pre_link_args,
        abi_return_struct_as_int: true,
        emit_debug_gdb_scripts: false,

        // Currently this is the only supported method of debuginfo on MSVC
        // where `*.pdb` files show up next to the final artifact.
        split_debuginfo: SplitDebuginfo::Packed,

        ..Default::default()
    }
}
