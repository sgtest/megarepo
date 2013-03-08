// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use core::libc::{c_char, c_int, c_uint, c_longlong, c_ulonglong};
use core::option;
use core::ptr;
use core::str;
use core::vec;
use std::oldmap::HashMap;

pub type Opcode = u32;
pub type Bool = c_uint;

pub const True: Bool = 1 as Bool;
pub const False: Bool = 0 as Bool;

// Consts for the LLVM CallConv type, pre-cast to uint.

pub enum CallConv {
    CCallConv = 0,
    FastCallConv = 8,
    ColdCallConv = 9,
    X86StdcallCallConv = 64,
    X86FastcallCallConv = 65,
}

pub enum Visibility {
    LLVMDefaultVisibility = 0,
    HiddenVisibility = 1,
    ProtectedVisibility = 2,
}

pub enum Linkage {
    ExternalLinkage = 0,
    AvailableExternallyLinkage = 1,
    LinkOnceAnyLinkage = 2,
    LinkOnceODRLinkage = 3,
    LinkOnceODRAutoHideLinkage = 4,
    WeakAnyLinkage = 5,
    WeakODRLinkage = 6,
    AppendingLinkage = 7,
    InternalLinkage = 8,
    PrivateLinkage = 9,
    DLLImportLinkage = 10,
    DLLExportLinkage = 11,
    ExternalWeakLinkage = 12,
    GhostLinkage = 13,
    CommonLinkage = 14,
    LinkerPrivateLinkage = 15,
    LinkerPrivateWeakLinkage = 16,
}

pub enum Attribute {
    ZExtAttribute = 1,
    SExtAttribute = 2,
    NoReturnAttribute = 4,
    InRegAttribute = 8,
    StructRetAttribute = 16,
    NoUnwindAttribute = 32,
    NoAliasAttribute = 64,
    ByValAttribute = 128,
    NestAttribute = 256,
    ReadNoneAttribute = 512,
    ReadOnlyAttribute = 1024,
    NoInlineAttribute = 2048,
    AlwaysInlineAttribute = 4096,
    OptimizeForSizeAttribute = 8192,
    StackProtectAttribute = 16384,
    StackProtectReqAttribute = 32768,
    // 31 << 16
    AlignmentAttribute = 2031616,
    NoCaptureAttribute = 2097152,
    NoRedZoneAttribute = 4194304,
    NoImplicitFloatAttribute = 8388608,
    NakedAttribute = 16777216,
    InlineHintAttribute = 33554432,
    // 7 << 26
    StackAttribute = 469762048,
    ReturnsTwiceAttribute = 536870912,
    // 1 << 30
    UWTableAttribute = 1073741824,
    NonLazyBindAttribute = 2147483648,
}

// enum for the LLVM IntPredicate type
pub enum IntPredicate {
    IntEQ = 32,
    IntNE = 33,
    IntUGT = 34,
    IntUGE = 35,
    IntULT = 36,
    IntULE = 37,
    IntSGT = 38,
    IntSGE = 39,
    IntSLT = 40,
    IntSLE = 41,
}

// enum for the LLVM RealPredicate type
pub enum RealPredicate {
    RealPredicateFalse = 0,
    RealOEQ = 1,
    RealOGT = 2,
    RealOGE = 3,
    RealOLT = 4,
    RealOLE = 5,
    RealONE = 6,
    RealORD = 7,
    RealUNO = 8,
    RealUEQ = 9,
    RealUGT = 10,
    RealUGE = 11,
    RealULT = 12,
    RealULE = 13,
    RealUNE = 14,
    RealPredicateTrue = 15,
}

// enum for the LLVM TypeKind type - must stay in sync with the def of
// LLVMTypeKind in llvm/include/llvm-c/Core.h
#[deriving_eq]
pub enum TypeKind {
    Void      = 0,
    Half      = 1,
    Float     = 2,
    Double    = 3,
    X86_FP80  = 4,
    FP128     = 5,
    PPC_FP128 = 6,
    Label     = 7,
    Integer   = 8,
    Function  = 9,
    Struct    = 10,
    Array     = 11,
    Pointer   = 12,
    Vector    = 13,
    Metadata  = 14,
    X86_MMX   = 15
}

pub enum AtomicBinOp {
    Xchg = 0,
    Add  = 1,
    Sub  = 2,
    And  = 3,
    Nand = 4,
    Or   = 5,
    Xor  = 6,
    Max  = 7,
    Min  = 8,
    UMax = 9,
    UMin = 10,
}

pub enum AtomicOrdering {
    NotAtomic = 0,
    Unordered = 1,
    Monotonic = 2,
    // Consume = 3,  // Not specified yet.
    Acquire = 4,
    Release = 5,
    AcquireRelease = 6,
    SequentiallyConsistent = 7
}

// FIXME: Not used right now, but will be once #2334 is fixed
// Consts for the LLVMCodeGenFileType type (in include/llvm/c/TargetMachine.h)
pub enum FileType {
    AssemblyFile = 0,
    ObjectFile = 1
}

pub enum Metadata {
    MD_dbg = 0,
    MD_tbaa = 1,
    MD_prof = 2,
    MD_fpmath = 3,
    MD_range = 4,
    MD_tbaa_struct = 5
}

// Opaque pointer types
pub enum Module_opaque {}
pub type ModuleRef = *Module_opaque;
pub enum Context_opaque {}
pub type ContextRef = *Context_opaque;
pub enum Type_opaque {}
pub type TypeRef = *Type_opaque;
pub enum Value_opaque {}
pub type ValueRef = *Value_opaque;
pub enum BasicBlock_opaque {}
pub type BasicBlockRef = *BasicBlock_opaque;
pub enum Builder_opaque {}
pub type BuilderRef = *Builder_opaque;
pub enum MemoryBuffer_opaque {}
pub type MemoryBufferRef = *MemoryBuffer_opaque;
pub enum PassManager_opaque {}
pub type PassManagerRef = *PassManager_opaque;
pub enum PassManagerBuilder_opaque {}
pub type PassManagerBuilderRef = *PassManagerBuilder_opaque;
pub enum Use_opaque {}
pub type UseRef = *Use_opaque;
pub enum TargetData_opaque {}
pub type TargetDataRef = *TargetData_opaque;
pub enum ObjectFile_opaque {}
pub type ObjectFileRef = *ObjectFile_opaque;
pub enum SectionIterator_opaque {}
pub type SectionIteratorRef = *SectionIterator_opaque;

pub mod llvm {
    use super::{AtomicBinOp, AtomicOrdering, BasicBlockRef, Bool, BuilderRef};
    use super::{ContextRef, MemoryBufferRef, ModuleRef, ObjectFileRef};
    use super::{Opcode, PassManagerRef, PassManagerBuilderRef};
    use super::{SectionIteratorRef, TargetDataRef, TypeKind, TypeRef, UseRef};
    use super::{ValueRef};

    use core::libc::{c_char, c_int, c_longlong, c_uint, c_ulonglong};

    #[link_args = "-Lrustllvm -lrustllvm"]
    #[link_name = "rustllvm"]
    #[abi = "cdecl"]
    pub extern {
        /* Create and destroy contexts. */
        pub unsafe fn LLVMContextCreate() -> ContextRef;
        pub unsafe fn LLVMGetGlobalContext() -> ContextRef;
        pub unsafe fn LLVMContextDispose(C: ContextRef);
        pub unsafe fn LLVMGetMDKindIDInContext(C: ContextRef,
                                           Name: *c_char,
                                           SLen: c_uint)
                                        -> c_uint;
        pub unsafe fn LLVMGetMDKindID(Name: *c_char, SLen: c_uint) -> c_uint;

        /* Create and destroy modules. */
        pub unsafe fn LLVMModuleCreateWithNameInContext(ModuleID: *c_char,
                                                    C: ContextRef)
                                                 -> ModuleRef;
        pub unsafe fn LLVMDisposeModule(M: ModuleRef);

        /** Data layout. See Module::getDataLayout. */
        pub unsafe fn LLVMGetDataLayout(M: ModuleRef) -> *c_char;
        pub unsafe fn LLVMSetDataLayout(M: ModuleRef, Triple: *c_char);

        /** Target triple. See Module::getTargetTriple. */
        pub unsafe fn LLVMGetTarget(M: ModuleRef) -> *c_char;
        pub unsafe fn LLVMSetTarget(M: ModuleRef, Triple: *c_char);

        /** See Module::dump. */
        pub unsafe fn LLVMDumpModule(M: ModuleRef);

        /** See Module::setModuleInlineAsm. */
        pub unsafe fn LLVMSetModuleInlineAsm(M: ModuleRef, Asm: *c_char);

        /** See llvm::LLVMTypeKind::getTypeID. */
        pub unsafe fn LLVMGetTypeKind(Ty: TypeRef) -> TypeKind;

        /** See llvm::LLVMType::getContext. */
        pub unsafe fn LLVMGetTypeContext(Ty: TypeRef) -> ContextRef;

        /* Operations on integer types */
        pub unsafe fn LLVMInt1TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMInt8TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMInt16TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMInt32TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMInt64TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMIntTypeInContext(C: ContextRef,
                                           NumBits: c_uint) -> TypeRef;

        pub unsafe fn LLVMInt1Type() -> TypeRef;
        pub unsafe fn LLVMInt8Type() -> TypeRef;
        pub unsafe fn LLVMInt16Type() -> TypeRef;
        pub unsafe fn LLVMInt32Type() -> TypeRef;
        pub unsafe fn LLVMInt64Type() -> TypeRef;
        pub unsafe fn LLVMIntType(NumBits: c_uint) -> TypeRef;
        pub unsafe fn LLVMGetIntTypeWidth(IntegerTy: TypeRef) -> c_uint;

        /* Operations on real types */
        pub unsafe fn LLVMFloatTypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMDoubleTypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMX86FP80TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMFP128TypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMPPCFP128TypeInContext(C: ContextRef) -> TypeRef;

        pub unsafe fn LLVMFloatType() -> TypeRef;
        pub unsafe fn LLVMDoubleType() -> TypeRef;
        pub unsafe fn LLVMX86FP80Type() -> TypeRef;
        pub unsafe fn LLVMFP128Type() -> TypeRef;
        pub unsafe fn LLVMPPCFP128Type() -> TypeRef;

        /* Operations on function types */
        pub unsafe fn LLVMFunctionType(ReturnType: TypeRef,
                                       ParamTypes: *TypeRef,
                                       ParamCount: c_uint,
                                       IsVarArg: Bool)
                                    -> TypeRef;
        pub unsafe fn LLVMIsFunctionVarArg(FunctionTy: TypeRef) -> Bool;
        pub unsafe fn LLVMGetReturnType(FunctionTy: TypeRef) -> TypeRef;
        pub unsafe fn LLVMCountParamTypes(FunctionTy: TypeRef) -> c_uint;
        pub unsafe fn LLVMGetParamTypes(FunctionTy: TypeRef, Dest: *TypeRef);

        /* Operations on struct types */
        pub unsafe fn LLVMStructTypeInContext(C: ContextRef,
                                              ElementTypes: *TypeRef,
                                              ElementCount: c_uint,
                                              Packed: Bool) -> TypeRef;
        pub unsafe fn LLVMStructType(ElementTypes: *TypeRef,
                                     ElementCount: c_uint,
                                     Packed: Bool)
                                  -> TypeRef;
        pub unsafe fn LLVMCountStructElementTypes(StructTy: TypeRef)
                                               -> c_uint;
        pub unsafe fn LLVMGetStructElementTypes(StructTy: TypeRef,
                                            Dest: *mut TypeRef);
        pub unsafe fn LLVMIsPackedStruct(StructTy: TypeRef) -> Bool;

        /* Operations on array, pointer, and vector types (sequence types) */
        pub unsafe fn LLVMArrayType(ElementType: TypeRef,
                         ElementCount: c_uint) -> TypeRef;
        pub unsafe fn LLVMPointerType(ElementType: TypeRef,
                           AddressSpace: c_uint) -> TypeRef;
        pub unsafe fn LLVMVectorType(ElementType: TypeRef,
                          ElementCount: c_uint) -> TypeRef;

        pub unsafe fn LLVMGetElementType(Ty: TypeRef) -> TypeRef;
        pub unsafe fn LLVMGetArrayLength(ArrayTy: TypeRef) -> c_uint;
        pub unsafe fn LLVMGetPointerAddressSpace(PointerTy: TypeRef)
                                              -> c_uint;
        pub unsafe fn LLVMGetVectorSize(VectorTy: TypeRef) -> c_uint;

        /* Operations on other types */
        pub unsafe fn LLVMVoidTypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMLabelTypeInContext(C: ContextRef) -> TypeRef;
        pub unsafe fn LLVMMetadataTypeInContext(C: ContextRef) -> TypeRef;

        pub unsafe fn LLVMVoidType() -> TypeRef;
        pub unsafe fn LLVMLabelType() -> TypeRef;
        pub unsafe fn LLVMMetadataType() -> TypeRef;

        /* Operations on all values */
        pub unsafe fn LLVMTypeOf(Val: ValueRef) -> TypeRef;
        pub unsafe fn LLVMGetValueName(Val: ValueRef) -> *c_char;
        pub unsafe fn LLVMSetValueName(Val: ValueRef, Name: *c_char);
        pub unsafe fn LLVMDumpValue(Val: ValueRef);
        pub unsafe fn LLVMReplaceAllUsesWith(OldVal: ValueRef,
                                             NewVal: ValueRef);
        pub unsafe fn LLVMHasMetadata(Val: ValueRef) -> c_int;
        pub unsafe fn LLVMGetMetadata(Val: ValueRef, KindID: c_uint)
                                   -> ValueRef;
        pub unsafe fn LLVMSetMetadata(Val: ValueRef,
                                      KindID: c_uint,
                                      Node: ValueRef);

        /* Operations on Uses */
        pub unsafe fn LLVMGetFirstUse(Val: ValueRef) -> UseRef;
        pub unsafe fn LLVMGetNextUse(U: UseRef) -> UseRef;
        pub unsafe fn LLVMGetUser(U: UseRef) -> ValueRef;
        pub unsafe fn LLVMGetUsedValue(U: UseRef) -> ValueRef;

        /* Operations on Users */
        pub unsafe fn LLVMGetNumOperands(Val: ValueRef) -> c_int;
        pub unsafe fn LLVMGetOperand(Val: ValueRef, Index: c_uint)
                                  -> ValueRef;
        pub unsafe fn LLVMSetOperand(Val: ValueRef,
                                     Index: c_uint,
                                     Op: ValueRef);

        /* Operations on constants of any type */
        pub unsafe fn LLVMConstNull(Ty: TypeRef) -> ValueRef;
        /* all zeroes */
        pub unsafe fn LLVMConstAllOnes(Ty: TypeRef) -> ValueRef;
        /* only for int/vector */
        pub unsafe fn LLVMGetUndef(Ty: TypeRef) -> ValueRef;
        pub unsafe fn LLVMIsConstant(Val: ValueRef) -> Bool;
        pub unsafe fn LLVMIsNull(Val: ValueRef) -> Bool;
        pub unsafe fn LLVMIsUndef(Val: ValueRef) -> Bool;
        pub unsafe fn LLVMConstPointerNull(Ty: TypeRef) -> ValueRef;

        /* Operations on metadata */
        pub unsafe fn LLVMMDStringInContext(C: ContextRef,
                                        Str: *c_char,
                                        SLen: c_uint)
                                     -> ValueRef;
        pub unsafe fn LLVMMDString(Str: *c_char, SLen: c_uint) -> ValueRef;
        pub unsafe fn LLVMMDNodeInContext(C: ContextRef,
                                      Vals: *ValueRef,
                                      Count: c_uint)
                                   -> ValueRef;
        pub unsafe fn LLVMMDNode(Vals: *ValueRef, Count: c_uint) -> ValueRef;
        pub unsafe fn LLVMAddNamedMetadataOperand(M: ModuleRef, Str: *c_char,
                                       Val: ValueRef);

        /* Operations on scalar constants */
        pub unsafe fn LLVMConstInt(IntTy: TypeRef,
                               N: c_ulonglong,
                               SignExtend: Bool)
                            -> ValueRef;
        pub unsafe fn LLVMConstIntOfString(IntTy: TypeRef,
                                       Text: *c_char,
                                       Radix: u8)
                                    -> ValueRef;
        pub unsafe fn LLVMConstIntOfStringAndSize(IntTy: TypeRef,
                                                  Text: *c_char,
                                                  SLen: c_uint,
                                                  Radix: u8)
                                               -> ValueRef;
        pub unsafe fn LLVMConstReal(RealTy: TypeRef, N: f64) -> ValueRef;
        pub unsafe fn LLVMConstRealOfString(RealTy: TypeRef,
                                        Text: *c_char)
                                     -> ValueRef;
        pub unsafe fn LLVMConstRealOfStringAndSize(RealTy: TypeRef,
                                                   Text: *c_char,
                                                   SLen: c_uint)
                                                -> ValueRef;
        pub unsafe fn LLVMConstIntGetZExtValue(ConstantVal: ValueRef)
                                            -> c_ulonglong;
        pub unsafe fn LLVMConstIntGetSExtValue(ConstantVal: ValueRef)
                                            -> c_longlong;


        /* Operations on composite constants */
        pub unsafe fn LLVMConstStringInContext(C: ContextRef,
                                           Str: *c_char,
                                           Length: c_uint,
                                           DontNullTerminate: Bool)
                                        -> ValueRef;
        pub unsafe fn LLVMConstStructInContext(C: ContextRef,
                                               ConstantVals: *ValueRef,
                                               Count: c_uint,
                                               Packed: Bool) -> ValueRef;

        pub unsafe fn LLVMConstString(Str: *c_char,
                                      Length: c_uint,
                                      DontNullTerminate: Bool)
                                   -> ValueRef;
        pub unsafe fn LLVMConstArray(ElementTy: TypeRef,
                                     ConstantVals: *ValueRef,
                                     Length: c_uint)
                                  -> ValueRef;
        pub unsafe fn LLVMConstStruct(ConstantVals: *ValueRef,
                                      Count: c_uint,
                                      Packed: Bool) -> ValueRef;
        pub unsafe fn LLVMConstVector(ScalarConstantVals: *ValueRef,
                                      Size: c_uint) -> ValueRef;

        /* Constant expressions */
        pub unsafe fn LLVMAlignOf(Ty: TypeRef) -> ValueRef;
        pub unsafe fn LLVMSizeOf(Ty: TypeRef) -> ValueRef;
        pub unsafe fn LLVMConstNeg(ConstantVal: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstNSWNeg(ConstantVal: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstNUWNeg(ConstantVal: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstFNeg(ConstantVal: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstNot(ConstantVal: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstAdd(LHSConstant: ValueRef,
                                   RHSConstant: ValueRef)
                                -> ValueRef;
        pub unsafe fn LLVMConstNSWAdd(LHSConstant: ValueRef,
                                  RHSConstant: ValueRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstNUWAdd(LHSConstant: ValueRef,
                                  RHSConstant: ValueRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstFAdd(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstSub(LHSConstant: ValueRef,
                                   RHSConstant: ValueRef)
                                -> ValueRef;
        pub unsafe fn LLVMConstNSWSub(LHSConstant: ValueRef,
                                      RHSConstant: ValueRef)
                                   -> ValueRef;
        pub unsafe fn LLVMConstNUWSub(LHSConstant: ValueRef,
                                      RHSConstant: ValueRef)
                                   -> ValueRef;
        pub unsafe fn LLVMConstFSub(LHSConstant: ValueRef,
                                    RHSConstant: ValueRef)
                                 -> ValueRef;
        pub unsafe fn LLVMConstMul(LHSConstant: ValueRef,
                               RHSConstant: ValueRef)
                            -> ValueRef;
        pub unsafe fn LLVMConstNSWMul(LHSConstant: ValueRef,
                                  RHSConstant: ValueRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstNUWMul(LHSConstant: ValueRef,
                                  RHSConstant: ValueRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstFMul(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstUDiv(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstSDiv(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstExactSDiv(LHSConstant: ValueRef,
                                     RHSConstant: ValueRef)
                                  -> ValueRef;
        pub unsafe fn LLVMConstFDiv(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstURem(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstSRem(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstFRem(LHSConstant: ValueRef,
                                RHSConstant: ValueRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstAnd(LHSConstant: ValueRef,
                               RHSConstant: ValueRef)
                            -> ValueRef;
        pub unsafe fn LLVMConstOr(LHSConstant: ValueRef,
                              RHSConstant: ValueRef)
                           -> ValueRef;
        pub unsafe fn LLVMConstXor(LHSConstant: ValueRef,
                               RHSConstant: ValueRef)
                            -> ValueRef;
        pub unsafe fn LLVMConstShl(LHSConstant: ValueRef,
                               RHSConstant: ValueRef)
                            -> ValueRef;
        pub unsafe fn LLVMConstLShr(LHSConstant: ValueRef,
                                    RHSConstant: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstAShr(LHSConstant: ValueRef,
                                    RHSConstant: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstGEP(ConstantVal: ValueRef,
                        ConstantIndices: *ValueRef,
                        NumIndices: c_uint) -> ValueRef;
        pub unsafe fn LLVMConstInBoundsGEP(ConstantVal: ValueRef,
                                       ConstantIndices: *ValueRef,
                                       NumIndices: c_uint)
                                    -> ValueRef;
        pub unsafe fn LLVMConstTrunc(ConstantVal: ValueRef,
                                 ToType: TypeRef)
                              -> ValueRef;
        pub unsafe fn LLVMConstSExt(ConstantVal: ValueRef,
                                ToType: TypeRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstZExt(ConstantVal: ValueRef,
                                ToType: TypeRef)
                             -> ValueRef;
        pub unsafe fn LLVMConstFPTrunc(ConstantVal: ValueRef,
                                   ToType: TypeRef)
                                -> ValueRef;
        pub unsafe fn LLVMConstFPExt(ConstantVal: ValueRef,
                                 ToType: TypeRef)
                              -> ValueRef;
        pub unsafe fn LLVMConstUIToFP(ConstantVal: ValueRef,
                                  ToType: TypeRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstSIToFP(ConstantVal: ValueRef,
                                  ToType: TypeRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstFPToUI(ConstantVal: ValueRef,
                                  ToType: TypeRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstFPToSI(ConstantVal: ValueRef,
                                  ToType: TypeRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstPtrToInt(ConstantVal: ValueRef,
                                    ToType: TypeRef)
                                 -> ValueRef;
        pub unsafe fn LLVMConstIntToPtr(ConstantVal: ValueRef,
                                    ToType: TypeRef)
                                 -> ValueRef;
        pub unsafe fn LLVMConstBitCast(ConstantVal: ValueRef,
                                   ToType: TypeRef)
                                -> ValueRef;
        pub unsafe fn LLVMConstZExtOrBitCast(ConstantVal: ValueRef,
                                         ToType: TypeRef)
                                      -> ValueRef;
        pub unsafe fn LLVMConstSExtOrBitCast(ConstantVal: ValueRef,
                                         ToType: TypeRef)
                                      -> ValueRef;
        pub unsafe fn LLVMConstTruncOrBitCast(ConstantVal: ValueRef,
                                          ToType: TypeRef)
                                       -> ValueRef;
        pub unsafe fn LLVMConstPointerCast(ConstantVal: ValueRef,
                                       ToType: TypeRef)
                                    -> ValueRef;
        pub unsafe fn LLVMConstIntCast(ConstantVal: ValueRef,
                                       ToType: TypeRef,
                                       isSigned: Bool)
                                    -> ValueRef;
        pub unsafe fn LLVMConstFPCast(ConstantVal: ValueRef,
                                  ToType: TypeRef)
                               -> ValueRef;
        pub unsafe fn LLVMConstSelect(ConstantCondition: ValueRef,
                                      ConstantIfTrue: ValueRef,
                                      ConstantIfFalse: ValueRef)
                                   -> ValueRef;
        pub unsafe fn LLVMConstExtractElement(VectorConstant: ValueRef,
                                   IndexConstant: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstInsertElement(VectorConstant: ValueRef,
                                  ElementValueConstant: ValueRef,
                                  IndexConstant: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstShuffleVector(VectorAConstant: ValueRef,
                                  VectorBConstant: ValueRef,
                                  MaskConstant: ValueRef) -> ValueRef;
        pub unsafe fn LLVMConstExtractValue(AggConstant: ValueRef,
                                            IdxList: *c_uint,
                                            NumIdx: c_uint) -> ValueRef;
        pub unsafe fn LLVMConstInsertValue(AggConstant: ValueRef,
                                           ElementValueConstant: ValueRef,
                                           IdxList: *c_uint,
                                           NumIdx: c_uint)
                                        -> ValueRef;
        pub unsafe fn LLVMConstInlineAsm(Ty: TypeRef, AsmString: *c_char,
                              Constraints: *c_char, HasSideEffects: Bool,
                              IsAlignStack: Bool) -> ValueRef;
        pub unsafe fn LLVMBlockAddress(F: ValueRef, BB: BasicBlockRef)
                                    -> ValueRef;



        /* Operations on global variables, functions, and aliases (globals) */
        pub unsafe fn LLVMGetGlobalParent(Global: ValueRef) -> ModuleRef;
        pub unsafe fn LLVMIsDeclaration(Global: ValueRef) -> Bool;
        pub unsafe fn LLVMGetLinkage(Global: ValueRef) -> c_uint;
        pub unsafe fn LLVMSetLinkage(Global: ValueRef, Link: c_uint);
        pub unsafe fn LLVMGetSection(Global: ValueRef) -> *c_char;
        pub unsafe fn LLVMSetSection(Global: ValueRef, Section: *c_char);
        pub unsafe fn LLVMGetVisibility(Global: ValueRef) -> c_uint;
        pub unsafe fn LLVMSetVisibility(Global: ValueRef, Viz: c_uint);
        pub unsafe fn LLVMGetAlignment(Global: ValueRef) -> c_uint;
        pub unsafe fn LLVMSetAlignment(Global: ValueRef, Bytes: c_uint);


        /* Operations on global variables */
        pub unsafe fn LLVMAddGlobal(M: ModuleRef,
                                Ty: TypeRef,
                                Name: *c_char)
                             -> ValueRef;
        pub unsafe fn LLVMAddGlobalInAddressSpace(M: ModuleRef,
                                              Ty: TypeRef,
                                              Name: *c_char,
                                              AddressSpace: c_uint)
                                           -> ValueRef;
        pub unsafe fn LLVMGetNamedGlobal(M: ModuleRef, Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMGetFirstGlobal(M: ModuleRef) -> ValueRef;
        pub unsafe fn LLVMGetLastGlobal(M: ModuleRef) -> ValueRef;
        pub unsafe fn LLVMGetNextGlobal(GlobalVar: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetPreviousGlobal(GlobalVar: ValueRef) -> ValueRef;
        pub unsafe fn LLVMDeleteGlobal(GlobalVar: ValueRef);
        pub unsafe fn LLVMGetInitializer(GlobalVar: ValueRef) -> ValueRef;
        pub unsafe fn LLVMSetInitializer(GlobalVar: ValueRef,
                                         ConstantVal: ValueRef);
        pub unsafe fn LLVMIsThreadLocal(GlobalVar: ValueRef) -> Bool;
        pub unsafe fn LLVMSetThreadLocal(GlobalVar: ValueRef,
                                         IsThreadLocal: Bool);
        pub unsafe fn LLVMIsGlobalConstant(GlobalVar: ValueRef) -> Bool;
        pub unsafe fn LLVMSetGlobalConstant(GlobalVar: ValueRef,
                                            IsConstant: Bool);

        /* Operations on aliases */
        pub unsafe fn LLVMAddAlias(M: ModuleRef,
                                   Ty: TypeRef,
                                   Aliasee: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;

        /* Operations on functions */
        pub unsafe fn LLVMAddFunction(M: ModuleRef,
                                  Name: *c_char,
                                  FunctionTy: TypeRef)
                               -> ValueRef;
        pub unsafe fn LLVMGetNamedFunction(M: ModuleRef,
                                           Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMGetFirstFunction(M: ModuleRef) -> ValueRef;
        pub unsafe fn LLVMGetLastFunction(M: ModuleRef) -> ValueRef;
        pub unsafe fn LLVMGetNextFunction(Fn: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetPreviousFunction(Fn: ValueRef) -> ValueRef;
        pub unsafe fn LLVMDeleteFunction(Fn: ValueRef);
        pub unsafe fn LLVMGetOrInsertFunction(M: ModuleRef, Name: *c_char,
                                   FunctionTy: TypeRef) -> ValueRef;
        pub unsafe fn LLVMGetIntrinsicID(Fn: ValueRef) -> c_uint;
        pub unsafe fn LLVMGetFunctionCallConv(Fn: ValueRef) -> c_uint;
        pub unsafe fn LLVMSetFunctionCallConv(Fn: ValueRef, CC: c_uint);
        pub unsafe fn LLVMGetGC(Fn: ValueRef) -> *c_char;
        pub unsafe fn LLVMSetGC(Fn: ValueRef, Name: *c_char);
        pub unsafe fn LLVMAddFunctionAttr(Fn: ValueRef,
                                          PA: c_ulonglong,
                                          HighPA: c_ulonglong);
        pub unsafe fn LLVMGetFunctionAttr(Fn: ValueRef) -> c_ulonglong;
        pub unsafe fn LLVMRemoveFunctionAttr(Fn: ValueRef,
                                             PA: c_ulonglong,
                                             HighPA: c_ulonglong);

        /* Operations on parameters */
        pub unsafe fn LLVMCountParams(Fn: ValueRef) -> c_uint;
        pub unsafe fn LLVMGetParams(Fn: ValueRef, Params: *ValueRef);
        pub unsafe fn LLVMGetParam(Fn: ValueRef, Index: c_uint) -> ValueRef;
        pub unsafe fn LLVMGetParamParent(Inst: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetFirstParam(Fn: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetLastParam(Fn: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetNextParam(Arg: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetPreviousParam(Arg: ValueRef) -> ValueRef;
        pub unsafe fn LLVMAddAttribute(Arg: ValueRef, PA: c_uint);
        pub unsafe fn LLVMRemoveAttribute(Arg: ValueRef, PA: c_uint);
        pub unsafe fn LLVMGetAttribute(Arg: ValueRef) -> c_uint;
        pub unsafe fn LLVMSetParamAlignment(Arg: ValueRef, align: c_uint);

        /* Operations on basic blocks */
        pub unsafe fn LLVMBasicBlockAsValue(BB: BasicBlockRef) -> ValueRef;
        pub unsafe fn LLVMValueIsBasicBlock(Val: ValueRef) -> Bool;
        pub unsafe fn LLVMValueAsBasicBlock(Val: ValueRef) -> BasicBlockRef;
        pub unsafe fn LLVMGetBasicBlockParent(BB: BasicBlockRef) -> ValueRef;
        pub unsafe fn LLVMCountBasicBlocks(Fn: ValueRef) -> c_uint;
        pub unsafe fn LLVMGetBasicBlocks(Fn: ValueRef,
                                         BasicBlocks: *ValueRef);
        pub unsafe fn LLVMGetFirstBasicBlock(Fn: ValueRef) -> BasicBlockRef;
        pub unsafe fn LLVMGetLastBasicBlock(Fn: ValueRef) -> BasicBlockRef;
        pub unsafe fn LLVMGetNextBasicBlock(BB: BasicBlockRef)
                                         -> BasicBlockRef;
        pub unsafe fn LLVMGetPreviousBasicBlock(BB: BasicBlockRef)
                                             -> BasicBlockRef;
        pub unsafe fn LLVMGetEntryBasicBlock(Fn: ValueRef) -> BasicBlockRef;

        pub unsafe fn LLVMAppendBasicBlockInContext(C: ContextRef,
                                                    Fn: ValueRef,
                                                    Name: *c_char)
                                                 -> BasicBlockRef;
        pub unsafe fn LLVMInsertBasicBlockInContext(C: ContextRef,
                                                    BB: BasicBlockRef,
                                                    Name: *c_char)
                                                 -> BasicBlockRef;

        pub unsafe fn LLVMAppendBasicBlock(Fn: ValueRef,
                                       Name: *c_char)
                                    -> BasicBlockRef;
        pub unsafe fn LLVMInsertBasicBlock(InsertBeforeBB: BasicBlockRef,
                                       Name: *c_char)
                                    -> BasicBlockRef;
        pub unsafe fn LLVMDeleteBasicBlock(BB: BasicBlockRef);

        /* Operations on instructions */
        pub unsafe fn LLVMGetInstructionParent(Inst: ValueRef)
                                            -> BasicBlockRef;
        pub unsafe fn LLVMGetFirstInstruction(BB: BasicBlockRef) -> ValueRef;
        pub unsafe fn LLVMGetLastInstruction(BB: BasicBlockRef) -> ValueRef;
        pub unsafe fn LLVMGetNextInstruction(Inst: ValueRef) -> ValueRef;
        pub unsafe fn LLVMGetPreviousInstruction(Inst: ValueRef) -> ValueRef;

        /* Operations on call sites */
        pub unsafe fn LLVMSetInstructionCallConv(Instr: ValueRef, CC: c_uint);
        pub unsafe fn LLVMGetInstructionCallConv(Instr: ValueRef) -> c_uint;
        pub unsafe fn LLVMAddInstrAttribute(Instr: ValueRef,
                                            index: c_uint,
                                            IA: c_uint);
        pub unsafe fn LLVMRemoveInstrAttribute(Instr: ValueRef,
                                               index: c_uint,
                                               IA: c_uint);
        pub unsafe fn LLVMSetInstrParamAlignment(Instr: ValueRef,
                                                 index: c_uint,
                                                 align: c_uint);

        /* Operations on call instructions (only) */
        pub unsafe fn LLVMIsTailCall(CallInst: ValueRef) -> Bool;
        pub unsafe fn LLVMSetTailCall(CallInst: ValueRef, IsTailCall: Bool);

        /* Operations on phi nodes */
        pub unsafe fn LLVMAddIncoming(PhiNode: ValueRef,
                                      IncomingValues: *ValueRef,
                                      IncomingBlocks: *BasicBlockRef,
                                      Count: c_uint);
        pub unsafe fn LLVMCountIncoming(PhiNode: ValueRef) -> c_uint;
        pub unsafe fn LLVMGetIncomingValue(PhiNode: ValueRef,
                                       Index: c_uint)
                                    -> ValueRef;
        pub unsafe fn LLVMGetIncomingBlock(PhiNode: ValueRef,
                                Index: c_uint) -> BasicBlockRef;

        /* Instruction builders */
        pub unsafe fn LLVMCreateBuilderInContext(C: ContextRef) -> BuilderRef;
        pub unsafe fn LLVMCreateBuilder() -> BuilderRef;
        pub unsafe fn LLVMPositionBuilder(Builder: BuilderRef,
                                          Block: BasicBlockRef,
                                          Instr: ValueRef);
        pub unsafe fn LLVMPositionBuilderBefore(Builder: BuilderRef,
                                                Instr: ValueRef);
        pub unsafe fn LLVMPositionBuilderAtEnd(Builder: BuilderRef,
                                               Block: BasicBlockRef);
        pub unsafe fn LLVMGetInsertBlock(Builder: BuilderRef)
                                      -> BasicBlockRef;
        pub unsafe fn LLVMClearInsertionPosition(Builder: BuilderRef);
        pub unsafe fn LLVMInsertIntoBuilder(Builder: BuilderRef,
                                            Instr: ValueRef);
        pub unsafe fn LLVMInsertIntoBuilderWithName(Builder: BuilderRef,
                                                Instr: ValueRef,
                                                Name: *c_char);
        pub unsafe fn LLVMDisposeBuilder(Builder: BuilderRef);

        /* Metadata */
        pub unsafe fn LLVMSetCurrentDebugLocation(Builder: BuilderRef,
                                                  L: ValueRef);
        pub unsafe fn LLVMGetCurrentDebugLocation(Builder: BuilderRef)
                                               -> ValueRef;
        pub unsafe fn LLVMSetInstDebugLocation(Builder: BuilderRef,
                                               Inst: ValueRef);

        /* Terminators */
        pub unsafe fn LLVMBuildRetVoid(B: BuilderRef) -> ValueRef;
        pub unsafe fn LLVMBuildRet(B: BuilderRef, V: ValueRef) -> ValueRef;
        pub unsafe fn LLVMBuildAggregateRet(B: BuilderRef, RetVals: *ValueRef,
                                 N: c_uint) -> ValueRef;
        pub unsafe fn LLVMBuildBr(B: BuilderRef, Dest: BasicBlockRef)
                               -> ValueRef;
        pub unsafe fn LLVMBuildCondBr(B: BuilderRef,
                                  If: ValueRef,
                                  Then: BasicBlockRef,
                                  Else: BasicBlockRef)
                               -> ValueRef;
        pub unsafe fn LLVMBuildSwitch(B: BuilderRef, V: ValueRef,
                                      Else: BasicBlockRef, NumCases: c_uint)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildIndirectBr(B: BuilderRef, Addr: ValueRef,
                               NumDests: c_uint) -> ValueRef;
        pub unsafe fn LLVMBuildInvoke(B: BuilderRef,
                                      Fn: ValueRef,
                                      Args: *ValueRef,
                                      NumArgs: c_uint,
                                      Then: BasicBlockRef,
                                      Catch: BasicBlockRef,
                                      Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildLandingPad(B: BuilderRef,
                                      Ty: TypeRef,
                                      PersFn: ValueRef,
                                      NumClauses: c_uint,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildResume(B: BuilderRef, Exn: ValueRef)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildUnreachable(B: BuilderRef) -> ValueRef;

        /* Add a case to the switch instruction */
        pub unsafe fn LLVMAddCase(Switch: ValueRef,
                              OnVal: ValueRef,
                              Dest: BasicBlockRef);

        /* Add a destination to the indirectbr instruction */
        pub unsafe fn LLVMAddDestination(IndirectBr: ValueRef,
                                         Dest: BasicBlockRef);

        /* Add a clause to the landing pad instruction */
        pub unsafe fn LLVMAddClause(LandingPad: ValueRef,
                                    ClauseVal: ValueRef);

        /* Set the cleanup on a landing pad instruction */
        pub unsafe fn LLVMSetCleanup(LandingPad: ValueRef, Val: Bool);

        /* Arithmetic */
        pub unsafe fn LLVMBuildAdd(B: BuilderRef,
                                   LHS: ValueRef,
                                   RHS: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildNSWAdd(B: BuilderRef,
                                      LHS: ValueRef,
                                      RHS: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildNUWAdd(B: BuilderRef,
                                      LHS: ValueRef,
                                      RHS: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildFAdd(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildSub(B: BuilderRef,
                                   LHS: ValueRef,
                                   RHS: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildNSWSub(B: BuilderRef,
                                      LHS: ValueRef,
                                      RHS: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildNUWSub(B: BuilderRef,
                                      LHS: ValueRef,
                                      RHS: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildFSub(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildMul(B: BuilderRef,
                                   LHS: ValueRef,
                                   RHS: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildNSWMul(B: BuilderRef,
                                      LHS: ValueRef,
                                      RHS: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildNUWMul(B: BuilderRef,
                                      LHS: ValueRef,
                                      RHS: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildFMul(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildUDiv(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildSDiv(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildExactSDiv(B: BuilderRef,
                                         LHS: ValueRef,
                                         RHS: ValueRef,
                                         Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMBuildFDiv(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildURem(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildSRem(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildFRem(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildShl(B: BuilderRef,
                                   LHS: ValueRef,
                                   RHS: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildLShr(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildAShr(B: BuilderRef,
                                    LHS: ValueRef,
                                    RHS: ValueRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildAnd(B: BuilderRef,
                                   LHS: ValueRef,
                                   RHS: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildOr(B: BuilderRef,
                                  LHS: ValueRef,
                                  RHS: ValueRef,
                                  Name: *c_char)
                               -> ValueRef;
        pub unsafe fn LLVMBuildXor(B: BuilderRef,
                                   LHS: ValueRef,
                                   RHS: ValueRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildBinOp(B: BuilderRef,
                                 Op: Opcode,
                                 LHS: ValueRef,
                                 RHS: ValueRef,
                                 Name: *c_char)
                              -> ValueRef;
        pub unsafe fn LLVMBuildNeg(B: BuilderRef,
                               V: ValueRef,
                               Name: *c_char)
                            -> ValueRef;
        pub unsafe fn LLVMBuildNSWNeg(B: BuilderRef,
                                  V: ValueRef,
                                  Name: *c_char)
                               -> ValueRef;
        pub unsafe fn LLVMBuildNUWNeg(B: BuilderRef,
                                  V: ValueRef,
                                  Name: *c_char)
                               -> ValueRef;
        pub unsafe fn LLVMBuildFNeg(B: BuilderRef,
                                V: ValueRef,
                                Name: *c_char)
                             -> ValueRef;
        pub unsafe fn LLVMBuildNot(B: BuilderRef,
                               V: ValueRef,
                               Name: *c_char)
                            -> ValueRef;

        /* Memory */
        pub unsafe fn LLVMBuildMalloc(B: BuilderRef,
                                      Ty: TypeRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildArrayMalloc(B: BuilderRef,
                                           Ty: TypeRef,
                                           Val: ValueRef,
                                           Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildAlloca(B: BuilderRef,
                                  Ty: TypeRef,
                                  Name: *c_char)
                               -> ValueRef;
        pub unsafe fn LLVMBuildArrayAlloca(B: BuilderRef,
                                           Ty: TypeRef,
                                           Val: ValueRef,
                                           Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildFree(B: BuilderRef,
                                    PointerVal: ValueRef) -> ValueRef;
        pub unsafe fn LLVMBuildLoad(B: BuilderRef,
                                PointerVal: ValueRef,
                                Name: *c_char)
                             -> ValueRef;
        pub unsafe fn LLVMBuildStore(B: BuilderRef,
                                     Val: ValueRef,
                                     Ptr: ValueRef) -> ValueRef;
        pub unsafe fn LLVMBuildGEP(B: BuilderRef,
                               Pointer: ValueRef,
                               Indices: *ValueRef,
                               NumIndices: c_uint,
                               Name: *c_char)
                            -> ValueRef;
        pub unsafe fn LLVMBuildInBoundsGEP(B: BuilderRef, Pointer: ValueRef,
                                Indices: *ValueRef, NumIndices: c_uint,
                                Name: *c_char)
           -> ValueRef;
        pub unsafe fn LLVMBuildStructGEP(B: BuilderRef,
                                     Pointer: ValueRef,
                                     Idx: c_uint,
                                     Name: *c_char)
                                  -> ValueRef;
        pub unsafe fn LLVMBuildGlobalString(B: BuilderRef,
                                        Str: *c_char,
                                        Name: *c_char)
                                     -> ValueRef;
        pub unsafe fn LLVMBuildGlobalStringPtr(B: BuilderRef,
                                           Str: *c_char,
                                           Name: *c_char)
                                        -> ValueRef;

        /* Casts */
        pub unsafe fn LLVMBuildTrunc(B: BuilderRef,
                                     Val: ValueRef,
                                     DestTy: TypeRef,
                                     Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildZExt(B: BuilderRef,
                                    Val: ValueRef,
                                    DestTy: TypeRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildSExt(B: BuilderRef,
                                    Val: ValueRef,
                                    DestTy: TypeRef,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildFPToUI(B: BuilderRef,
                                      Val: ValueRef,
                                      DestTy: TypeRef,
                                      Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildFPToSI(B: BuilderRef,
                                      Val: ValueRef,
                                      DestTy: TypeRef,
                                      Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildUIToFP(B: BuilderRef,
                                      Val: ValueRef,
                                      DestTy: TypeRef,
                                      Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildSIToFP(B: BuilderRef,
                                      Val: ValueRef,
                                      DestTy: TypeRef,
                                      Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildFPTrunc(B: BuilderRef,
                                       Val: ValueRef,
                                       DestTy: TypeRef,
                                       Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildFPExt(B: BuilderRef,
                                     Val: ValueRef,
                                     DestTy: TypeRef,
                                     Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildPtrToInt(B: BuilderRef,
                                        Val: ValueRef,
                                        DestTy: TypeRef,
                                        Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildIntToPtr(B: BuilderRef,
                                        Val: ValueRef,
                                        DestTy: TypeRef,
                                        Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildBitCast(B: BuilderRef,
                                       Val: ValueRef,
                                       DestTy: TypeRef,
                                       Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildZExtOrBitCast(B: BuilderRef,
                                         Val: ValueRef,
                                         DestTy: TypeRef,
                                         Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMBuildSExtOrBitCast(B: BuilderRef,
                                         Val: ValueRef,
                                         DestTy: TypeRef,
                                         Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMBuildTruncOrBitCast(B: BuilderRef,
                                          Val: ValueRef,
                                          DestTy: TypeRef,
                                          Name: *c_char)
                                       -> ValueRef;
        pub unsafe fn LLVMBuildCast(B: BuilderRef, Op: Opcode, Val: ValueRef,
                         DestTy: TypeRef, Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildPointerCast(B: BuilderRef,
                                       Val: ValueRef,
                                       DestTy: TypeRef,
                                       Name: *c_char)
                                    -> ValueRef;
        pub unsafe fn LLVMBuildIntCast(B: BuilderRef,
                                       Val: ValueRef,
                                       DestTy: TypeRef,
                                       Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildFPCast(B: BuilderRef,
                                      Val: ValueRef,
                                      DestTy: TypeRef,
                                      Name: *c_char) -> ValueRef;

        /* Comparisons */
        pub unsafe fn LLVMBuildICmp(B: BuilderRef, Op: c_uint, LHS: ValueRef,
                         RHS: ValueRef, Name: *c_char) -> ValueRef;
        pub unsafe fn LLVMBuildFCmp(B: BuilderRef, Op: c_uint, LHS: ValueRef,
                         RHS: ValueRef, Name: *c_char) -> ValueRef;

        /* Miscellaneous instructions */
        pub unsafe fn LLVMBuildPhi(B: BuilderRef,
                                   Ty: TypeRef,
                                   Name: *c_char)
                                -> ValueRef;
        pub unsafe fn LLVMBuildCall(B: BuilderRef,
                                    Fn: ValueRef,
                                    Args: *ValueRef,
                                    NumArgs: c_uint,
                                    Name: *c_char)
                                 -> ValueRef;
        pub unsafe fn LLVMBuildSelect(B: BuilderRef,
                                      If: ValueRef,
                                      Then: ValueRef,
                                      Else: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildVAArg(B: BuilderRef,
                                     list: ValueRef,
                                     Ty: TypeRef,
                                     Name: *c_char)
                                  -> ValueRef;
        pub unsafe fn LLVMBuildExtractElement(B: BuilderRef,
                                          VecVal: ValueRef,
                                          Index: ValueRef,
                                          Name: *c_char)
                                       -> ValueRef;
        pub unsafe fn LLVMBuildInsertElement(B: BuilderRef,
                                         VecVal: ValueRef,
                                         EltVal: ValueRef,
                                         Index: ValueRef,
                                         Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMBuildShuffleVector(B: BuilderRef,
                                         V1: ValueRef,
                                         V2: ValueRef,
                                         Mask: ValueRef,
                                         Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMBuildExtractValue(B: BuilderRef,
                                        AggVal: ValueRef,
                                        Index: c_uint,
                                        Name: *c_char)
                                     -> ValueRef;
        pub unsafe fn LLVMBuildInsertValue(B: BuilderRef,
                                       AggVal: ValueRef,
                                       EltVal: ValueRef,
                                       Index: c_uint,
                                       Name: *c_char)
                                    -> ValueRef;

        pub unsafe fn LLVMBuildIsNull(B: BuilderRef,
                                      Val: ValueRef,
                                      Name: *c_char)
                                   -> ValueRef;
        pub unsafe fn LLVMBuildIsNotNull(B: BuilderRef,
                                         Val: ValueRef,
                                         Name: *c_char)
                                      -> ValueRef;
        pub unsafe fn LLVMBuildPtrDiff(B: BuilderRef,
                                       LHS: ValueRef,
                                       RHS: ValueRef,
                                       Name: *c_char) -> ValueRef;

        /* Atomic Operations */
        pub unsafe fn LLVMBuildAtomicCmpXchg(B: BuilderRef, LHS: ValueRef,
                                  CMP: ValueRef, RHS: ValueRef,
                                  ++Order: AtomicOrdering) -> ValueRef;
        pub unsafe fn LLVMBuildAtomicRMW(B: BuilderRef, ++Op: AtomicBinOp,
                              LHS: ValueRef, RHS: ValueRef,
                              ++Order: AtomicOrdering) -> ValueRef;

        /* Selected entries from the downcasts. */
        pub unsafe fn LLVMIsATerminatorInst(Inst: ValueRef) -> ValueRef;

        /** Writes a module to the specified path. Returns 0 on success. */
        pub unsafe fn LLVMWriteBitcodeToFile(M: ModuleRef,
                                             Path: *c_char) -> c_int;

        /** Creates target data from a target layout string. */
        pub unsafe fn LLVMCreateTargetData(StringRep: *c_char)
                                        -> TargetDataRef;
        /** Adds the target data to the given pass manager. The pass manager
            references the target data only weakly. */
        pub unsafe fn LLVMAddTargetData(TD: TargetDataRef,
                                        PM: PassManagerRef);
        /** Number of bytes clobbered when doing a Store to *T. */
        pub unsafe fn LLVMStoreSizeOfType(TD: TargetDataRef, Ty: TypeRef)
            -> c_ulonglong;

        /** Number of bytes clobbered when doing a Store to *T. */
        pub unsafe fn LLVMSizeOfTypeInBits(TD: TargetDataRef, Ty: TypeRef)
            -> c_ulonglong;

        /** Distance between successive elements in an array of T.
        Includes ABI padding. */
        pub unsafe fn LLVMABISizeOfType(TD: TargetDataRef, Ty: TypeRef)
                                     -> c_uint;

        /** Returns the preferred alignment of a type. */
        pub unsafe fn LLVMPreferredAlignmentOfType(TD: TargetDataRef,
                                        Ty: TypeRef) -> c_uint;
        /** Returns the minimum alignment of a type. */
        pub unsafe fn LLVMABIAlignmentOfType(TD: TargetDataRef,
                                  Ty: TypeRef) -> c_uint;
        /**
         * Returns the minimum alignment of a type when part of a call frame.
         */
        pub unsafe fn LLVMCallFrameAlignmentOfType(TD: TargetDataRef,
                                                   Ty: TypeRef)
                                                -> c_uint;

        /** Disposes target data. */
        pub unsafe fn LLVMDisposeTargetData(TD: TargetDataRef);

        /** Creates a pass manager. */
        pub unsafe fn LLVMCreatePassManager() -> PassManagerRef;
        /** Disposes a pass manager. */
        pub unsafe fn LLVMDisposePassManager(PM: PassManagerRef);
        /** Runs a pass manager on a module. */
        pub unsafe fn LLVMRunPassManager(PM: PassManagerRef,
                                         M: ModuleRef) -> Bool;

        /** Adds a verification pass. */
        pub unsafe fn LLVMAddVerifierPass(PM: PassManagerRef);

        pub unsafe fn LLVMAddGlobalOptimizerPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddIPSCCPPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddDeadArgEliminationPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddInstructionCombiningPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddCFGSimplificationPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddFunctionInliningPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddFunctionAttrsPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddScalarReplAggregatesPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddScalarReplAggregatesPassSSA(PM: PassManagerRef);
        pub unsafe fn LLVMAddJumpThreadingPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddConstantPropagationPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddReassociatePass(PM: PassManagerRef);
        pub unsafe fn LLVMAddLoopRotatePass(PM: PassManagerRef);
        pub unsafe fn LLVMAddLICMPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddLoopUnswitchPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddLoopDeletionPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddLoopUnrollPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddGVNPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddMemCpyOptPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddSCCPPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddDeadStoreEliminationPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddStripDeadPrototypesPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddConstantMergePass(PM: PassManagerRef);
        pub unsafe fn LLVMAddArgumentPromotionPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddTailCallEliminationPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddIndVarSimplifyPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddAggressiveDCEPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddGlobalDCEPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddCorrelatedValuePropagationPass(PM:
                                                            PassManagerRef);
        pub unsafe fn LLVMAddPruneEHPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddSimplifyLibCallsPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddLoopIdiomPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddEarlyCSEPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddTypeBasedAliasAnalysisPass(PM: PassManagerRef);
        pub unsafe fn LLVMAddBasicAliasAnalysisPass(PM: PassManagerRef);

        pub unsafe fn LLVMPassManagerBuilderCreate() -> PassManagerBuilderRef;
        pub unsafe fn LLVMPassManagerBuilderDispose(PMB:
                                                    PassManagerBuilderRef);
        pub unsafe fn LLVMPassManagerBuilderSetOptLevel(
            PMB: PassManagerBuilderRef, OptimizationLevel: c_uint);
        pub unsafe fn LLVMPassManagerBuilderSetSizeLevel(
            PMB: PassManagerBuilderRef, Value: Bool);
        pub unsafe fn LLVMPassManagerBuilderSetDisableUnitAtATime(
            PMB: PassManagerBuilderRef, Value: Bool);
        pub unsafe fn LLVMPassManagerBuilderSetDisableUnrollLoops(
            PMB: PassManagerBuilderRef, Value: Bool);
        pub unsafe fn LLVMPassManagerBuilderSetDisableSimplifyLibCalls
            (PMB: PassManagerBuilderRef, Value: Bool);
        pub unsafe fn LLVMPassManagerBuilderUseInlinerWithThreshold
            (PMB: PassManagerBuilderRef, threshold: c_uint);
        pub unsafe fn LLVMPassManagerBuilderPopulateModulePassManager
            (PMB: PassManagerBuilderRef, PM: PassManagerRef);

        pub unsafe fn LLVMPassManagerBuilderPopulateFunctionPassManager
            (PMB: PassManagerBuilderRef, PM: PassManagerRef);

        /** Destroys a memory buffer. */
        pub unsafe fn LLVMDisposeMemoryBuffer(MemBuf: MemoryBufferRef);


        /* Stuff that's in rustllvm/ because it's not upstream yet. */

        /** Opens an object file. */
        pub unsafe fn LLVMCreateObjectFile(MemBuf: MemoryBufferRef)
                                        -> ObjectFileRef;
        /** Closes an object file. */
        pub unsafe fn LLVMDisposeObjectFile(ObjFile: ObjectFileRef);

        /** Enumerates the sections in an object file. */
        pub unsafe fn LLVMGetSections(ObjFile: ObjectFileRef)
                                   -> SectionIteratorRef;
        /** Destroys a section iterator. */
        pub unsafe fn LLVMDisposeSectionIterator(SI: SectionIteratorRef);
        /** Returns true if the section iterator is at the end of the section
            list: */
        pub unsafe fn LLVMIsSectionIteratorAtEnd(ObjFile: ObjectFileRef,
                                      SI: SectionIteratorRef) -> Bool;
        /** Moves the section iterator to point to the next section. */
        pub unsafe fn LLVMMoveToNextSection(SI: SectionIteratorRef);
        /** Returns the current section name. */
        pub unsafe fn LLVMGetSectionName(SI: SectionIteratorRef) -> *c_char;
        /** Returns the current section size. */
        pub unsafe fn LLVMGetSectionSize(SI: SectionIteratorRef)
                                      -> c_ulonglong;
        /** Returns the current section contents as a string buffer. */
        pub unsafe fn LLVMGetSectionContents(SI: SectionIteratorRef)
                                          -> *c_char;

        /** Reads the given file and returns it as a memory buffer. Use
            LLVMDisposeMemoryBuffer() to get rid of it. */
        pub unsafe fn LLVMRustCreateMemoryBufferWithContentsOfFile(
                Path: *c_char)
             -> MemoryBufferRef;

        pub unsafe fn LLVMRustWriteOutputFile(PM: PassManagerRef,
                                              M: ModuleRef,
                                              Triple: *c_char,
                                              // FIXME: When #2334 is fixed,
                                              // change c_uint to FileType
                                              Output: *c_char,
                                              FileType: c_uint,
                                              OptLevel: c_int,
                                              EnableSegmentedStacks: bool)
                                           -> bool;

        /** Returns a string describing the last error caused by an LLVMRust*
            call. */
        pub unsafe fn LLVMRustGetLastError() -> *c_char;

        /** Prepare the JIT. Returns a memory manager that can load crates. */
        pub unsafe fn LLVMRustPrepareJIT(__morestack: *()) -> *();

        /** Load a crate into the memory manager. */
        pub unsafe fn LLVMRustLoadCrate(MM: *(),
                                        Filename: *c_char)
                                     -> bool;

        /** Execute the JIT engine. */
        pub unsafe fn LLVMRustExecuteJIT(MM: *(),
                              PM: PassManagerRef,
                              M: ModuleRef,
                              OptLevel: c_int,
                              EnableSegmentedStacks: bool) -> *();

        /** Parses the bitcode in the given memory buffer. */
        pub unsafe fn LLVMRustParseBitcode(MemBuf: MemoryBufferRef)
                                        -> ModuleRef;

        /** Parses LLVM asm in the given file */
        pub unsafe fn LLVMRustParseAssemblyFile(Filename: *c_char)
                                             -> ModuleRef;

        pub unsafe fn LLVMRustAddPrintModulePass(PM: PassManagerRef,
                                                 M: ModuleRef,
                                                 Output: *c_char);

        /** Turn on LLVM pass-timing. */
        pub unsafe fn LLVMRustEnableTimePasses();

        /// Print the pass timings since static dtors aren't picking them up.
        pub unsafe fn LLVMRustPrintPassTimings();

        pub unsafe fn LLVMStructCreateNamed(C: ContextRef, Name: *c_char)
                                         -> TypeRef;

        pub unsafe fn LLVMStructSetBody(StructTy: TypeRef,
                                        ElementTypes: *TypeRef,
                                        ElementCount: c_uint,
                                        Packed: Bool);

        pub unsafe fn LLVMConstNamedStruct(S: TypeRef,
                                           ConstantVals: *ValueRef,
                                           Count: c_uint)
                                        -> ValueRef;

        /** Enables LLVM debug output. */
        pub unsafe fn LLVMSetDebug(Enabled: c_int);
    }
}

pub fn SetInstructionCallConv(Instr: ValueRef, CC: CallConv) {
    unsafe {
        llvm::LLVMSetInstructionCallConv(Instr, CC as c_uint);
    }
}
pub fn SetFunctionCallConv(Fn: ValueRef, CC: CallConv) {
    unsafe {
        llvm::LLVMSetFunctionCallConv(Fn, CC as c_uint);
    }
}
pub fn SetLinkage(Global: ValueRef, Link: Linkage) {
    unsafe {
        llvm::LLVMSetLinkage(Global, Link as c_uint);
    }
}

/* Memory-managed object interface to type handles. */

pub struct TypeNames {
    type_names: HashMap<TypeRef, @str>,
    named_types: HashMap<@str, TypeRef>
}

pub fn associate_type(tn: @TypeNames, s: @str, t: TypeRef) {
    fail_unless!(tn.type_names.insert(t, s));
    fail_unless!(tn.named_types.insert(s, t));
}

pub fn type_has_name(tn: @TypeNames, t: TypeRef) -> Option<@str> {
    return tn.type_names.find(&t);
}

pub fn name_has_type(tn: @TypeNames, s: @str) -> Option<TypeRef> {
    return tn.named_types.find(&s);
}

pub fn mk_type_names() -> @TypeNames {
    @TypeNames {
        type_names: HashMap(),
        named_types: HashMap()
    }
}

pub fn type_to_str(names: @TypeNames, ty: TypeRef) -> @str {
    return type_to_str_inner(names, [], ty);
}

pub fn type_to_str_inner(names: @TypeNames, +outer0: &[TypeRef], ty: TypeRef)
                      -> @str {
    unsafe {
        match type_has_name(names, ty) {
          option::Some(n) => return n,
          _ => {}
        }

        let outer = vec::append_one(outer0.to_vec(), ty);

        let kind = llvm::LLVMGetTypeKind(ty);

        fn tys_str(names: @TypeNames, outer: &[TypeRef],
                   tys: ~[TypeRef]) -> @str {
            let mut s = ~"";
            let mut first: bool = true;
            for tys.each |t| {
                if first { first = false; } else { s += ~", "; }
                s += type_to_str_inner(names, outer, *t).to_owned();
            }
            // [Note at-str] FIXME #2543: Could rewrite this without the copy,
            // but need better @str support.
            return s.to_managed();
        }

        match kind {
          Void => return @"Void",
          Half => return @"Half",
          Float => return @"Float",
          Double => return @"Double",
          X86_FP80 => return @"X86_FP80",
          FP128 => return @"FP128",
          PPC_FP128 => return @"PPC_FP128",
          Label => return @"Label",
          Integer => {
            // See [Note at-str]
            return fmt!("i%d", llvm::LLVMGetIntTypeWidth(ty)
                        as int).to_managed();
          }
          Function => {
            let out_ty: TypeRef = llvm::LLVMGetReturnType(ty);
            let n_args = llvm::LLVMCountParamTypes(ty) as uint;
            let args = vec::from_elem(n_args, 0 as TypeRef);
            unsafe {
                llvm::LLVMGetParamTypes(ty, vec::raw::to_ptr(args));
            }
            // See [Note at-str]
            return fmt!("fn(%s) -> %s",
                        tys_str(names, outer, args),
                        type_to_str_inner(names, outer, out_ty)).to_managed();
          }
          Struct => {
            let elts = struct_tys(ty);
            // See [Note at-str]
            return fmt!("{%s}", tys_str(names, outer, elts)).to_managed();
          }
          Array => {
            let el_ty = llvm::LLVMGetElementType(ty);
            // See [Note at-str]
            return fmt!("[%s@ x %u", type_to_str_inner(names, outer, el_ty),
                llvm::LLVMGetArrayLength(ty) as uint).to_managed();
          }
          Pointer => {
            let mut i = 0;
            for outer0.each |tout| {
                i += 1;
                if *tout as int == ty as int {
                    let n = outer0.len() - i;
                    // See [Note at-str]
                    return fmt!("*\\%d", n as int).to_managed();
                }
            }
            let addrstr = {
                let addrspace = llvm::LLVMGetPointerAddressSpace(ty) as uint;
                if addrspace == 0 {
                    ~""
                } else {
                    fmt!("addrspace(%u)", addrspace)
                }
            };
            // See [Note at-str]
            return fmt!("%s*%s", addrstr, type_to_str_inner(names,
                        outer,
                        llvm::LLVMGetElementType(ty))).to_managed();
          }
          Vector => return @"Vector",
          Metadata => return @"Metadata",
          X86_MMX => return @"X86_MMAX"
        }
    }
}

pub fn float_width(llt: TypeRef) -> uint {
    unsafe {
        return match llvm::LLVMGetTypeKind(llt) as int {
              1 => 32u,
              2 => 64u,
              3 => 80u,
              4 | 5 => 128u,
              _ => fail!(~"llvm_float_width called on a non-float type")
            };
    }
}

pub fn fn_ty_param_tys(fn_ty: TypeRef) -> ~[TypeRef] {
    unsafe {
        let args = vec::from_elem(llvm::LLVMCountParamTypes(fn_ty) as uint,
                                 0 as TypeRef);
        llvm::LLVMGetParamTypes(fn_ty, vec::raw::to_ptr(args));
        return args;
    }
}

pub fn struct_tys(struct_ty: TypeRef) -> ~[TypeRef] {
    unsafe {
        let n_elts = llvm::LLVMCountStructElementTypes(struct_ty) as uint;
        if n_elts == 0 {
            return ~[];
        }
        let mut elts = vec::from_elem(n_elts, ptr::null());
        llvm::LLVMGetStructElementTypes(
            struct_ty, ptr::to_mut_unsafe_ptr(&mut elts[0]));
        return elts;
    }
}


/* Memory-managed interface to target data. */

pub struct target_data_res {
    TD: TargetDataRef,
}

impl Drop for target_data_res {
    fn finalize(&self) {
        unsafe {
            llvm::LLVMDisposeTargetData(self.TD);
        }
    }
}

pub fn target_data_res(TD: TargetDataRef) -> target_data_res {
    target_data_res {
        TD: TD
    }
}

pub struct TargetData {
    lltd: TargetDataRef,
    dtor: @target_data_res
}

pub fn mk_target_data(string_rep: ~str) -> TargetData {
    let lltd =
        str::as_c_str(string_rep, |buf| unsafe {
            llvm::LLVMCreateTargetData(buf)
        });

    TargetData {
        lltd: lltd,
        dtor: @target_data_res(lltd)
    }
}

/* Memory-managed interface to pass managers. */

pub struct pass_manager_res {
    PM: PassManagerRef,
}

impl Drop for pass_manager_res {
    fn finalize(&self) {
        unsafe {
            llvm::LLVMDisposePassManager(self.PM);
        }
    }
}

pub fn pass_manager_res(PM: PassManagerRef) -> pass_manager_res {
    pass_manager_res {
        PM: PM
    }
}

pub struct PassManager {
    llpm: PassManagerRef,
    dtor: @pass_manager_res
}

pub fn mk_pass_manager() -> PassManager {
    unsafe {
        let llpm = llvm::LLVMCreatePassManager();

        PassManager {
            llpm: llpm,
            dtor: @pass_manager_res(llpm)
        }
    }
}

/* Memory-managed interface to object files. */

pub struct object_file_res {
    ObjectFile: ObjectFileRef,
}

impl Drop for object_file_res {
    fn finalize(&self) {
        unsafe {
            llvm::LLVMDisposeObjectFile(self.ObjectFile);
        }
    }
}

pub fn object_file_res(ObjFile: ObjectFileRef) -> object_file_res {
    object_file_res {
        ObjectFile: ObjFile
    }
}

pub struct ObjectFile {
    llof: ObjectFileRef,
    dtor: @object_file_res
}

pub fn mk_object_file(llmb: MemoryBufferRef) -> Option<ObjectFile> {
    unsafe {
        let llof = llvm::LLVMCreateObjectFile(llmb);
        if llof as int == 0 { return option::None::<ObjectFile>; }

        option::Some(ObjectFile {
            llof: llof,
            dtor: @object_file_res(llof)
        })
    }
}

/* Memory-managed interface to section iterators. */

pub struct section_iter_res {
    SI: SectionIteratorRef,
}

impl Drop for section_iter_res {
    fn finalize(&self) {
        unsafe {
            llvm::LLVMDisposeSectionIterator(self.SI);
        }
    }
}

pub fn section_iter_res(SI: SectionIteratorRef) -> section_iter_res {
    section_iter_res {
        SI: SI
    }
}

pub struct SectionIter {
    llsi: SectionIteratorRef,
    dtor: @section_iter_res
}

pub fn mk_section_iter(llof: ObjectFileRef) -> SectionIter {
    unsafe {
        let llsi = llvm::LLVMGetSections(llof);
        SectionIter {
            llsi: llsi,
            dtor: @section_iter_res(llsi)
        }
    }
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
