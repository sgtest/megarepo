//! Set and unset common attributes on LLVM values.

use std::ffi::CString;

use cstr::cstr;
use rustc_codegen_ssa::traits::*;
use rustc_data_structures::small_c_str::SmallCStr;
use rustc_hir::def_id::DefId;
use rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags;
use rustc_middle::ty::{self, TyCtxt};
use rustc_session::config::OptLevel;
use rustc_target::spec::abi::Abi;
use rustc_target::spec::{FramePointer, SanitizerSet, StackProbeType, StackProtector};
use smallvec::SmallVec;

use crate::attributes;
use crate::llvm::AttributePlace::Function;
use crate::llvm::{self, Attribute, AttributeKind, AttributePlace};
use crate::llvm_util;
pub use rustc_attr::{InlineAttr, InstructionSetAttr, OptimizeAttr};

use crate::context::CodegenCx;
use crate::value::Value;

pub fn apply_to_llfn(llfn: &Value, idx: AttributePlace, attrs: &[&Attribute]) {
    if !attrs.is_empty() {
        llvm::AddFunctionAttributes(llfn, idx, attrs);
    }
}

pub fn remove_from_llfn(llfn: &Value, idx: AttributePlace, attrs: &[AttributeKind]) {
    if !attrs.is_empty() {
        llvm::RemoveFunctionAttributes(llfn, idx, attrs);
    }
}

pub fn apply_to_callsite(callsite: &Value, idx: AttributePlace, attrs: &[&Attribute]) {
    if !attrs.is_empty() {
        llvm::AddCallSiteAttributes(callsite, idx, attrs);
    }
}

/// Get LLVM attribute for the provided inline heuristic.
#[inline]
fn inline_attr<'ll>(cx: &CodegenCx<'ll, '_>, inline: InlineAttr) -> Option<&'ll Attribute> {
    match inline {
        InlineAttr::Hint => Some(AttributeKind::InlineHint.create_attr(cx.llcx)),
        InlineAttr::Always => Some(AttributeKind::AlwaysInline.create_attr(cx.llcx)),
        InlineAttr::Never => {
            if cx.sess().target.arch != "amdgpu" {
                Some(AttributeKind::NoInline.create_attr(cx.llcx))
            } else {
                None
            }
        }
        InlineAttr::None => None,
    }
}

/// Get LLVM sanitize attributes.
#[inline]
pub fn sanitize_attrs<'ll>(
    cx: &CodegenCx<'ll, '_>,
    no_sanitize: SanitizerSet,
) -> SmallVec<[&'ll Attribute; 4]> {
    let mut attrs = SmallVec::new();
    let enabled = cx.tcx.sess.opts.debugging_opts.sanitizer - no_sanitize;
    if enabled.contains(SanitizerSet::ADDRESS) {
        attrs.push(llvm::AttributeKind::SanitizeAddress.create_attr(cx.llcx));
    }
    if enabled.contains(SanitizerSet::MEMORY) {
        attrs.push(llvm::AttributeKind::SanitizeMemory.create_attr(cx.llcx));
    }
    if enabled.contains(SanitizerSet::THREAD) {
        attrs.push(llvm::AttributeKind::SanitizeThread.create_attr(cx.llcx));
    }
    if enabled.contains(SanitizerSet::HWADDRESS) {
        attrs.push(llvm::AttributeKind::SanitizeHWAddress.create_attr(cx.llcx));
    }
    if enabled.contains(SanitizerSet::MEMTAG) {
        // Check to make sure the mte target feature is actually enabled.
        let features = cx.tcx.global_backend_features(());
        let mte_feature =
            features.iter().map(|s| &s[..]).rfind(|n| ["+mte", "-mte"].contains(&&n[..]));
        if let None | Some("-mte") = mte_feature {
            cx.tcx.sess.err("`-Zsanitizer=memtag` requires `-Ctarget-feature=+mte`");
        }

        attrs.push(llvm::AttributeKind::SanitizeMemTag.create_attr(cx.llcx));
    }
    attrs
}

/// Tell LLVM to emit or not emit the information necessary to unwind the stack for the function.
#[inline]
pub fn uwtable_attr(llcx: &llvm::Context) -> &Attribute {
    // NOTE: We should determine if we even need async unwind tables, as they
    // take have more overhead and if we can use sync unwind tables we
    // probably should.
    llvm::CreateUWTableAttr(llcx, true)
}

pub fn frame_pointer_type_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> Option<&'ll Attribute> {
    let mut fp = cx.sess().target.frame_pointer;
    // "mcount" function relies on stack pointer.
    // See <https://sourceware.org/binutils/docs/gprof/Implementation.html>.
    if cx.sess().instrument_mcount() || matches!(cx.sess().opts.cg.force_frame_pointers, Some(true))
    {
        fp = FramePointer::Always;
    }
    let attr_value = match fp {
        FramePointer::Always => cstr!("all"),
        FramePointer::NonLeaf => cstr!("non-leaf"),
        FramePointer::MayOmit => return None,
    };
    Some(llvm::CreateAttrStringValue(cx.llcx, cstr!("frame-pointer"), attr_value))
}

/// Tell LLVM what instrument function to insert.
#[inline]
fn instrument_function_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> Option<&'ll Attribute> {
    if cx.sess().instrument_mcount() {
        // Similar to `clang -pg` behavior. Handled by the
        // `post-inline-ee-instrument` LLVM pass.

        // The function name varies on platforms.
        // See test/CodeGen/mcount.c in clang.
        let mcount_name = CString::new(cx.sess().target.mcount.as_str().as_bytes()).unwrap();

        Some(llvm::CreateAttrStringValue(
            cx.llcx,
            cstr!("instrument-function-entry-inlined"),
            &mcount_name,
        ))
    } else {
        None
    }
}

fn probestack_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> Option<&'ll Attribute> {
    // Currently stack probes seem somewhat incompatible with the address
    // sanitizer and thread sanitizer. With asan we're already protected from
    // stack overflow anyway so we don't really need stack probes regardless.
    if cx
        .sess()
        .opts
        .debugging_opts
        .sanitizer
        .intersects(SanitizerSet::ADDRESS | SanitizerSet::THREAD)
    {
        return None;
    }

    // probestack doesn't play nice either with `-C profile-generate`.
    if cx.sess().opts.cg.profile_generate.enabled() {
        return None;
    }

    // probestack doesn't play nice either with gcov profiling.
    if cx.sess().opts.debugging_opts.profile {
        return None;
    }

    let attr_value = match cx.sess().target.stack_probes {
        StackProbeType::None => return None,
        // Request LLVM to generate the probes inline. If the given LLVM version does not support
        // this, no probe is generated at all (even if the attribute is specified).
        StackProbeType::Inline => cstr!("inline-asm"),
        // Flag our internal `__rust_probestack` function as the stack probe symbol.
        // This is defined in the `compiler-builtins` crate for each architecture.
        StackProbeType::Call => cstr!("__rust_probestack"),
        // Pick from the two above based on the LLVM version.
        StackProbeType::InlineOrCall { min_llvm_version_for_inline } => {
            if llvm_util::get_version() < min_llvm_version_for_inline {
                cstr!("__rust_probestack")
            } else {
                cstr!("inline-asm")
            }
        }
    };
    Some(llvm::CreateAttrStringValue(cx.llcx, cstr!("probe-stack"), attr_value))
}

fn stackprotector_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> Option<&'ll Attribute> {
    let sspattr = match cx.sess().stack_protector() {
        StackProtector::None => return None,
        StackProtector::All => AttributeKind::StackProtectReq,
        StackProtector::Strong => AttributeKind::StackProtectStrong,
        StackProtector::Basic => AttributeKind::StackProtect,
    };

    Some(sspattr.create_attr(cx.llcx))
}

pub fn target_cpu_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> &'ll Attribute {
    let target_cpu = SmallCStr::new(llvm_util::target_cpu(cx.tcx.sess));
    llvm::CreateAttrStringValue(cx.llcx, cstr!("target-cpu"), target_cpu.as_c_str())
}

pub fn tune_cpu_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> Option<&'ll Attribute> {
    llvm_util::tune_cpu(cx.tcx.sess).map(|tune| {
        let tune_cpu = SmallCStr::new(tune);
        llvm::CreateAttrStringValue(cx.llcx, cstr!("tune-cpu"), tune_cpu.as_c_str())
    })
}

/// Get the `NonLazyBind` LLVM attribute,
/// if the codegen options allow skipping the PLT.
pub fn non_lazy_bind_attr<'ll>(cx: &CodegenCx<'ll, '_>) -> Option<&'ll Attribute> {
    // Don't generate calls through PLT if it's not necessary
    if !cx.sess().needs_plt() {
        Some(AttributeKind::NonLazyBind.create_attr(cx.llcx))
    } else {
        None
    }
}

/// Returns attributes to remove and to add, respectively,
/// to set the default optimizations attrs on a function.
#[inline]
pub(crate) fn default_optimisation_attrs<'ll>(
    cx: &CodegenCx<'ll, '_>,
) -> (
    // Attributes to remove
    SmallVec<[AttributeKind; 3]>,
    // Attributes to add
    SmallVec<[&'ll Attribute; 2]>,
) {
    let mut to_remove = SmallVec::new();
    let mut to_add = SmallVec::new();
    match cx.sess().opts.optimize {
        OptLevel::Size => {
            to_remove.push(llvm::AttributeKind::MinSize);
            to_add.push(llvm::AttributeKind::OptimizeForSize.create_attr(cx.llcx));
            to_remove.push(llvm::AttributeKind::OptimizeNone);
        }
        OptLevel::SizeMin => {
            to_add.push(llvm::AttributeKind::MinSize.create_attr(cx.llcx));
            to_add.push(llvm::AttributeKind::OptimizeForSize.create_attr(cx.llcx));
            to_remove.push(llvm::AttributeKind::OptimizeNone);
        }
        OptLevel::No => {
            to_remove.push(llvm::AttributeKind::MinSize);
            to_remove.push(llvm::AttributeKind::OptimizeForSize);
            to_remove.push(llvm::AttributeKind::OptimizeNone);
        }
        _ => {}
    }
    (to_remove, to_add)
}

/// Composite function which sets LLVM attributes for function depending on its AST (`#[attribute]`)
/// attributes.
pub fn from_fn_attrs<'ll, 'tcx>(
    cx: &CodegenCx<'ll, 'tcx>,
    llfn: &'ll Value,
    instance: ty::Instance<'tcx>,
) {
    let codegen_fn_attrs = cx.tcx.codegen_fn_attrs(instance.def_id());

    let mut to_remove = SmallVec::<[_; 4]>::new();
    let mut to_add = SmallVec::<[_; 16]>::new();

    match codegen_fn_attrs.optimize {
        OptimizeAttr::None => {
            let (to_remove_opt, to_add_opt) = default_optimisation_attrs(cx);
            to_remove.extend(to_remove_opt);
            to_add.extend(to_add_opt);
        }
        OptimizeAttr::Speed => {
            to_remove.push(llvm::AttributeKind::MinSize);
            to_remove.push(llvm::AttributeKind::OptimizeForSize);
            to_remove.push(llvm::AttributeKind::OptimizeNone);
        }
        OptimizeAttr::Size => {
            to_add.push(llvm::AttributeKind::MinSize.create_attr(cx.llcx));
            to_add.push(llvm::AttributeKind::OptimizeForSize.create_attr(cx.llcx));
            to_remove.push(llvm::AttributeKind::OptimizeNone);
        }
    }

    let inline = if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::NAKED) {
        InlineAttr::Never
    } else if codegen_fn_attrs.inline == InlineAttr::None && instance.def.requires_inline(cx.tcx) {
        InlineAttr::Hint
    } else {
        codegen_fn_attrs.inline
    };
    to_add.extend(inline_attr(cx, inline));

    // The `uwtable` attribute according to LLVM is:
    //
    //     This attribute indicates that the ABI being targeted requires that an
    //     unwind table entry be produced for this function even if we can show
    //     that no exceptions passes by it. This is normally the case for the
    //     ELF x86-64 abi, but it can be disabled for some compilation units.
    //
    // Typically when we're compiling with `-C panic=abort` (which implies this
    // `no_landing_pads` check) we don't need `uwtable` because we can't
    // generate any exceptions! On Windows, however, exceptions include other
    // events such as illegal instructions, segfaults, etc. This means that on
    // Windows we end up still needing the `uwtable` attribute even if the `-C
    // panic=abort` flag is passed.
    //
    // You can also find more info on why Windows always requires uwtables here:
    //      https://bugzilla.mozilla.org/show_bug.cgi?id=1302078
    if cx.sess().must_emit_unwind_tables() {
        to_add.push(uwtable_attr(cx.llcx));
    }

    if cx.sess().opts.debugging_opts.profile_sample_use.is_some() {
        to_add.push(llvm::CreateAttrString(cx.llcx, cstr!("use-sample-profile")));
    }

    // FIXME: none of these three functions interact with source level attributes.
    to_add.extend(frame_pointer_type_attr(cx));
    to_add.extend(instrument_function_attr(cx));
    to_add.extend(probestack_attr(cx));
    to_add.extend(stackprotector_attr(cx));

    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::COLD) {
        to_add.push(AttributeKind::Cold.create_attr(cx.llcx));
    }
    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::FFI_RETURNS_TWICE) {
        to_add.push(AttributeKind::ReturnsTwice.create_attr(cx.llcx));
    }
    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::FFI_PURE) {
        to_add.push(AttributeKind::ReadOnly.create_attr(cx.llcx));
    }
    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::FFI_CONST) {
        to_add.push(AttributeKind::ReadNone.create_attr(cx.llcx));
    }
    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::NAKED) {
        to_add.push(AttributeKind::Naked.create_attr(cx.llcx));
    }
    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::ALLOCATOR) {
        // apply to return place instead of function (unlike all other attributes applied in this function)
        let no_alias = AttributeKind::NoAlias.create_attr(cx.llcx);
        attributes::apply_to_llfn(llfn, AttributePlace::ReturnValue, &[no_alias]);
    }
    if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::CMSE_NONSECURE_ENTRY) {
        to_add.push(llvm::CreateAttrString(cx.llcx, cstr!("cmse_nonsecure_entry")));
    }
    if let Some(align) = codegen_fn_attrs.alignment {
        llvm::set_alignment(llfn, align as usize);
    }
    to_add.extend(sanitize_attrs(cx, codegen_fn_attrs.no_sanitize));

    // Always annotate functions with the target-cpu they are compiled for.
    // Without this, ThinLTO won't inline Rust functions into Clang generated
    // functions (because Clang annotates functions this way too).
    to_add.push(target_cpu_attr(cx));
    // tune-cpu is only conveyed through the attribute for our purpose.
    // The target doesn't care; the subtarget reads our attribute.
    to_add.extend(tune_cpu_attr(cx));

    let function_features =
        codegen_fn_attrs.target_features.iter().map(|f| f.as_str()).collect::<Vec<&str>>();

    if let Some(f) = llvm_util::check_tied_features(
        cx.tcx.sess,
        &function_features.iter().map(|f| (*f, true)).collect(),
    ) {
        let span = cx
            .tcx
            .get_attrs(instance.def_id())
            .iter()
            .find(|a| a.has_name(rustc_span::sym::target_feature))
            .map_or_else(|| cx.tcx.def_span(instance.def_id()), |a| a.span);
        let msg = format!(
            "the target features {} must all be either enabled or disabled together",
            f.join(", ")
        );
        let mut err = cx.tcx.sess.struct_span_err(span, &msg);
        err.help("add the missing features in a `target_feature` attribute");
        err.emit();
        return;
    }

    let mut function_features = function_features
        .iter()
        .flat_map(|feat| {
            llvm_util::to_llvm_features(cx.tcx.sess, feat).into_iter().map(|f| format!("+{}", f))
        })
        .chain(codegen_fn_attrs.instruction_set.iter().map(|x| match x {
            InstructionSetAttr::ArmA32 => "-thumb-mode".to_string(),
            InstructionSetAttr::ArmT32 => "+thumb-mode".to_string(),
        }))
        .collect::<Vec<String>>();

    if cx.tcx.sess.target.is_like_wasm {
        // If this function is an import from the environment but the wasm
        // import has a specific module/name, apply them here.
        if let Some(module) = wasm_import_module(cx.tcx, instance.def_id()) {
            to_add.push(llvm::CreateAttrStringValue(cx.llcx, cstr!("wasm-import-module"), &module));

            let name =
                codegen_fn_attrs.link_name.unwrap_or_else(|| cx.tcx.item_name(instance.def_id()));
            let name = CString::new(name.as_str()).unwrap();
            to_add.push(llvm::CreateAttrStringValue(cx.llcx, cstr!("wasm-import-name"), &name));
        }

        // The `"wasm"` abi on wasm targets automatically enables the
        // `+multivalue` feature because the purpose of the wasm abi is to match
        // the WebAssembly specification, which has this feature. This won't be
        // needed when LLVM enables this `multivalue` feature by default.
        if !cx.tcx.is_closure(instance.def_id()) {
            let abi = cx.tcx.fn_sig(instance.def_id()).abi();
            if abi == Abi::Wasm {
                function_features.push("+multivalue".to_string());
            }
        }
    }

    if !function_features.is_empty() {
        let global_features = cx.tcx.global_backend_features(()).iter().map(|s| &s[..]);
        let val = global_features
            .chain(function_features.iter().map(|s| &s[..]))
            .intersperse(",")
            .collect::<SmallCStr>();
        to_add.push(llvm::CreateAttrStringValue(cx.llcx, cstr!("target-features"), &val));
    }

    attributes::remove_from_llfn(llfn, Function, &to_remove);
    attributes::apply_to_llfn(llfn, Function, &to_add);
}

fn wasm_import_module(tcx: TyCtxt<'_>, id: DefId) -> Option<CString> {
    tcx.wasm_import_module_map(id.krate).get(&id).map(|s| CString::new(&s[..]).unwrap())
}
