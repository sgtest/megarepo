use rustc_middle::mir::interpret::{alloc_range, AllocRange, ConstValue, Pointer};

use crate::{
    rustc_smir::{Stable, Tables},
    stable_mir::mir::Mutability,
    stable_mir::ty::{Allocation, ProvenanceMap},
};

/// Creates new empty `Allocation` from given `Align`.
fn new_empty_allocation(align: rustc_target::abi::Align) -> Allocation {
    Allocation {
        bytes: Vec::new(),
        provenance: ProvenanceMap { ptrs: Vec::new() },
        align: align.bytes(),
        mutability: Mutability::Not,
    }
}

// We need this method instead of a Stable implementation
// because we need to get `Ty` of the const we are trying to create, to do that
// we need to have access to `ConstantKind` but we can't access that inside Stable impl.
#[allow(rustc::usage_of_qualified_ty)]
pub fn new_allocation<'tcx>(
    ty: rustc_middle::ty::Ty<'tcx>,
    const_value: ConstValue<'tcx>,
    tables: &mut Tables<'tcx>,
) -> Allocation {
    match const_value {
        ConstValue::Scalar(scalar) => {
            let size = scalar.size();
            let align = tables
                .tcx
                .layout_of(rustc_middle::ty::ParamEnv::reveal_all().and(ty))
                .unwrap()
                .align;
            let mut allocation = rustc_middle::mir::interpret::Allocation::uninit(size, align.abi);
            allocation
                .write_scalar(&tables.tcx, alloc_range(rustc_target::abi::Size::ZERO, size), scalar)
                .unwrap();
            allocation.stable(tables)
        }
        ConstValue::ZeroSized => {
            let align =
                tables.tcx.layout_of(rustc_middle::ty::ParamEnv::empty().and(ty)).unwrap().align;
            new_empty_allocation(align.abi)
        }
        ConstValue::Slice { data, start, end } => {
            let alloc_id = tables.tcx.create_memory_alloc(data);
            let ptr = Pointer::new(alloc_id, rustc_target::abi::Size::from_bytes(start));
            let scalar_ptr = rustc_middle::mir::interpret::Scalar::from_pointer(ptr, &tables.tcx);
            let scalar_len = rustc_middle::mir::interpret::Scalar::from_target_usize(
                (end - start) as u64,
                &tables.tcx,
            );
            let layout =
                tables.tcx.layout_of(rustc_middle::ty::ParamEnv::reveal_all().and(ty)).unwrap();
            let mut allocation =
                rustc_middle::mir::interpret::Allocation::uninit(layout.size, layout.align.abi);
            allocation
                .write_scalar(
                    &tables.tcx,
                    alloc_range(rustc_target::abi::Size::ZERO, tables.tcx.data_layout.pointer_size),
                    scalar_ptr,
                )
                .unwrap();
            allocation
                .write_scalar(
                    &tables.tcx,
                    alloc_range(tables.tcx.data_layout.pointer_size, scalar_len.size()),
                    scalar_len,
                )
                .unwrap();
            allocation.stable(tables)
        }
        ConstValue::ByRef { alloc, offset } => {
            let ty_size = tables
                .tcx
                .layout_of(rustc_middle::ty::ParamEnv::reveal_all().and(ty))
                .unwrap()
                .size;
            allocation_filter(&alloc.0, alloc_range(offset, ty_size), tables)
        }
    }
}

/// Creates an `Allocation` only from information within the `AllocRange`.
pub(super) fn allocation_filter<'tcx>(
    alloc: &rustc_middle::mir::interpret::Allocation,
    alloc_range: AllocRange,
    tables: &mut Tables<'tcx>,
) -> Allocation {
    let mut bytes: Vec<Option<u8>> = alloc
        .inspect_with_uninit_and_ptr_outside_interpreter(
            alloc_range.start.bytes_usize()..alloc_range.end().bytes_usize(),
        )
        .iter()
        .copied()
        .map(Some)
        .collect();
    for (i, b) in bytes.iter_mut().enumerate() {
        if !alloc
            .init_mask()
            .get(rustc_target::abi::Size::from_bytes(i + alloc_range.start.bytes_usize()))
        {
            *b = None;
        }
    }
    let mut ptrs = Vec::new();
    for (offset, prov) in alloc
        .provenance()
        .ptrs()
        .iter()
        .filter(|a| a.0 >= alloc_range.start && a.0 <= alloc_range.end())
    {
        ptrs.push((offset.bytes_usize() - alloc_range.start.bytes_usize(), tables.prov(*prov)));
    }
    Allocation {
        bytes: bytes,
        provenance: ProvenanceMap { ptrs },
        align: alloc.align.bytes(),
        mutability: alloc.mutability.stable(tables),
    }
}
