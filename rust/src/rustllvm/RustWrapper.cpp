// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#include "rustllvm.h"
#include "llvm/Object/Archive.h"
#include "llvm/Object/ObjectFile.h"
#include "llvm/IR/DiagnosticInfo.h"
#include "llvm/IR/DiagnosticPrinter.h"

#if LLVM_VERSION_MINOR >= 5
#include "llvm/IR/CallSite.h"
#else
#include "llvm/Support/CallSite.h"
#endif

//===----------------------------------------------------------------------===
//
// This file defines alternate interfaces to core functions that are more
// readily callable by Rust's FFI.
//
//===----------------------------------------------------------------------===

using namespace llvm;
using namespace llvm::sys;
using namespace llvm::object;

static char *LastError;

#if LLVM_VERSION_MINOR >= 5
extern "C" LLVMMemoryBufferRef
LLVMRustCreateMemoryBufferWithContentsOfFile(const char *Path) {
  ErrorOr<std::unique_ptr<MemoryBuffer>> buf_or = MemoryBuffer::getFile(Path,
                                                                        -1,
                                                                        false);
  if (!buf_or) {
      LLVMRustSetLastError(buf_or.getError().message().c_str());
      return nullptr;
  }
  return wrap(buf_or.get().release());
}
#else
extern "C" LLVMMemoryBufferRef
LLVMRustCreateMemoryBufferWithContentsOfFile(const char *Path) {
  OwningPtr<MemoryBuffer> buf;
  error_code err = MemoryBuffer::getFile(Path, buf, -1, false);
  if (err) {
      LLVMRustSetLastError(err.message().c_str());
      return NULL;
  }
  return wrap(buf.take());
}
#endif

extern "C" char *LLVMRustGetLastError(void) {
  char *ret = LastError;
  LastError = NULL;
  return ret;
}

void LLVMRustSetLastError(const char *err) {
  free((void*) LastError);
  LastError = strdup(err);
}

extern "C" void
LLVMRustSetNormalizedTarget(LLVMModuleRef M, const char *triple) {
    unwrap(M)->setTargetTriple(Triple::normalize(triple));
}

extern "C" LLVMValueRef LLVMRustConstSmallInt(LLVMTypeRef IntTy, unsigned N,
                                              LLVMBool SignExtend) {
  return LLVMConstInt(IntTy, (unsigned long long)N, SignExtend);
}

extern "C" LLVMValueRef LLVMRustConstInt(LLVMTypeRef IntTy,
           unsigned N_hi,
           unsigned N_lo,
           LLVMBool SignExtend) {
  unsigned long long N = N_hi;
  N <<= 32;
  N |= N_lo;
  return LLVMConstInt(IntTy, N, SignExtend);
}

extern "C" void LLVMRustPrintPassTimings() {
  raw_fd_ostream OS (2, false); // stderr.
  TimerGroup::printAll(OS);
}

extern "C" LLVMValueRef LLVMGetOrInsertFunction(LLVMModuleRef M,
                                                const char* Name,
                                                LLVMTypeRef FunctionTy) {
  return wrap(unwrap(M)->getOrInsertFunction(Name,
                                             unwrap<FunctionType>(FunctionTy)));
}

extern "C" LLVMTypeRef LLVMMetadataTypeInContext(LLVMContextRef C) {
  return wrap(Type::getMetadataTy(*unwrap(C)));
}

extern "C" void LLVMAddCallSiteAttribute(LLVMValueRef Instr, unsigned index, uint64_t Val) {
  CallSite Call = CallSite(unwrap<Instruction>(Instr));
  AttrBuilder B;
  B.addRawValue(Val);
  Call.setAttributes(
    Call.getAttributes().addAttributes(Call->getContext(), index,
                                       AttributeSet::get(Call->getContext(),
                                                         index, B)));
}


#if LLVM_VERSION_MINOR >= 5
extern "C" void LLVMAddDereferenceableCallSiteAttr(LLVMValueRef Instr, unsigned idx, uint64_t b) {
  CallSite Call = CallSite(unwrap<Instruction>(Instr));
  AttrBuilder B;
  B.addDereferenceableAttr(b);
  Call.setAttributes(
    Call.getAttributes().addAttributes(Call->getContext(), idx,
                                       AttributeSet::get(Call->getContext(),
                                                         idx, B)));
}
#else
extern "C" void LLVMAddDereferenceableCallSiteAttr(LLVMValueRef, unsigned, uint64_t) {}
#endif

extern "C" void LLVMAddFunctionAttribute(LLVMValueRef Fn, unsigned index, uint64_t Val) {
  Function *A = unwrap<Function>(Fn);
  AttrBuilder B;
  B.addRawValue(Val);
  A->addAttributes(index, AttributeSet::get(A->getContext(), index, B));
}

#if LLVM_VERSION_MINOR >= 5
extern "C" void LLVMAddDereferenceableAttr(LLVMValueRef Fn, unsigned index, uint64_t bytes) {
  Function *A = unwrap<Function>(Fn);
  AttrBuilder B;
  B.addDereferenceableAttr(bytes);
  A->addAttributes(index, AttributeSet::get(A->getContext(), index, B));
}
#else
extern "C" void LLVMAddDereferenceableAttr(LLVMValueRef, unsigned, uint64_t) {}
#endif

extern "C" void LLVMAddFunctionAttrString(LLVMValueRef Fn, unsigned index, const char *Name) {
  Function *F = unwrap<Function>(Fn);
  AttrBuilder B;
  B.addAttribute(Name);
  F->addAttributes(index, AttributeSet::get(F->getContext(), index, B));
}

extern "C" void LLVMRemoveFunctionAttrString(LLVMValueRef fn, unsigned index, const char *Name) {
  Function *f = unwrap<Function>(fn);
  LLVMContext &C = f->getContext();
  AttrBuilder B;
  B.addAttribute(Name);
  AttributeSet to_remove = AttributeSet::get(C, index, B);

  AttributeSet attrs = f->getAttributes();
  f->setAttributes(attrs.removeAttributes(f->getContext(),
                                          index,
                                          to_remove));
}

extern "C" LLVMValueRef LLVMBuildAtomicLoad(LLVMBuilderRef B,
                                            LLVMValueRef source,
                                            const char* Name,
                                            AtomicOrdering order,
                                            unsigned alignment) {
    LoadInst* li = new LoadInst(unwrap(source),0);
    li->setVolatile(true);
    li->setAtomic(order);
    li->setAlignment(alignment);
    return wrap(unwrap(B)->Insert(li, Name));
}

extern "C" LLVMValueRef LLVMBuildAtomicStore(LLVMBuilderRef B,
                                             LLVMValueRef val,
                                             LLVMValueRef target,
                                             AtomicOrdering order,
                                             unsigned alignment) {
    StoreInst* si = new StoreInst(unwrap(val),unwrap(target));
    si->setVolatile(true);
    si->setAtomic(order);
    si->setAlignment(alignment);
    return wrap(unwrap(B)->Insert(si));
}

extern "C" LLVMValueRef LLVMBuildAtomicCmpXchg(LLVMBuilderRef B,
                                               LLVMValueRef target,
                                               LLVMValueRef old,
                                               LLVMValueRef source,
                                               AtomicOrdering order,
                                               AtomicOrdering failure_order) {
    return wrap(unwrap(B)->CreateAtomicCmpXchg(unwrap(target), unwrap(old),
                                               unwrap(source), order
#if LLVM_VERSION_MINOR >= 5
                                               , failure_order
#endif
                                               ));
}
extern "C" LLVMValueRef LLVMBuildAtomicFence(LLVMBuilderRef B, AtomicOrdering order) {
    return wrap(unwrap(B)->CreateFence(order));
}

extern "C" void LLVMSetDebug(int Enabled) {
#ifndef NDEBUG
  DebugFlag = Enabled;
#endif
}

extern "C" LLVMValueRef LLVMInlineAsm(LLVMTypeRef Ty,
                                      char *AsmString,
                                      char *Constraints,
                                      LLVMBool HasSideEffects,
                                      LLVMBool IsAlignStack,
                                      unsigned Dialect) {
    return wrap(InlineAsm::get(unwrap<FunctionType>(Ty), AsmString,
                               Constraints, HasSideEffects,
                               IsAlignStack, (InlineAsm::AsmDialect) Dialect));
}

typedef DIBuilder* DIBuilderRef;

template<typename DIT>
DIT unwrapDI(LLVMValueRef ref) {
    return DIT(ref ? unwrap<MDNode>(ref) : NULL);
}

#if LLVM_VERSION_MINOR >= 5
extern "C" const uint32_t LLVMRustDebugMetadataVersion = DEBUG_METADATA_VERSION;
#else
extern "C" const uint32_t LLVMRustDebugMetadataVersion = 1;
#endif

extern "C" void LLVMRustAddModuleFlag(LLVMModuleRef M,
                                      const char *name,
                                      uint32_t value) {
    unwrap(M)->addModuleFlag(Module::Warning, name, value);
}

extern "C" DIBuilderRef LLVMDIBuilderCreate(LLVMModuleRef M) {
    return new DIBuilder(*unwrap(M));
}

extern "C" void LLVMDIBuilderDispose(DIBuilderRef Builder) {
    delete Builder;
}

extern "C" void LLVMDIBuilderFinalize(DIBuilderRef Builder) {
    Builder->finalize();
}

extern "C" LLVMValueRef LLVMDIBuilderCreateCompileUnit(
    DIBuilderRef Builder,
    unsigned Lang,
    const char* File,
    const char* Dir,
    const char* Producer,
    bool isOptimized,
    const char* Flags,
    unsigned RuntimeVer,
    const char* SplitName) {
    return wrap(Builder->createCompileUnit(Lang,
                                           File,
                                           Dir,
                                           Producer,
                                           isOptimized,
                                           Flags,
                                           RuntimeVer,
                                           SplitName));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateFile(
    DIBuilderRef Builder,
    const char* Filename,
    const char* Directory) {
    return wrap(Builder->createFile(Filename, Directory));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateSubroutineType(
    DIBuilderRef Builder,
    LLVMValueRef File,
    LLVMValueRef ParameterTypes) {
    return wrap(Builder->createSubroutineType(
        unwrapDI<DIFile>(File),
#if LLVM_VERSION_MINOR >= 6
        unwrapDI<DITypeArray>(ParameterTypes)));
#else
        unwrapDI<DIArray>(ParameterTypes)));
#endif
}

extern "C" LLVMValueRef LLVMDIBuilderCreateFunction(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    const char* LinkageName,
    LLVMValueRef File,
    unsigned LineNo,
    LLVMValueRef Ty,
    bool isLocalToUnit,
    bool isDefinition,
    unsigned ScopeLine,
    unsigned Flags,
    bool isOptimized,
    LLVMValueRef Fn,
    LLVMValueRef TParam,
    LLVMValueRef Decl) {
    return wrap(Builder->createFunction(
        unwrapDI<DIScope>(Scope), Name, LinkageName,
        unwrapDI<DIFile>(File), LineNo,
        unwrapDI<DICompositeType>(Ty), isLocalToUnit, isDefinition, ScopeLine,
        Flags, isOptimized,
        unwrap<Function>(Fn),
        unwrapDI<MDNode*>(TParam),
        unwrapDI<MDNode*>(Decl)));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateBasicType(
    DIBuilderRef Builder,
    const char* Name,
    uint64_t SizeInBits,
    uint64_t AlignInBits,
    unsigned Encoding) {
    return wrap(Builder->createBasicType(
        Name, SizeInBits,
        AlignInBits, Encoding));
}

extern "C" LLVMValueRef LLVMDIBuilderCreatePointerType(
    DIBuilderRef Builder,
    LLVMValueRef PointeeTy,
    uint64_t SizeInBits,
    uint64_t AlignInBits,
    const char* Name) {
    return wrap(Builder->createPointerType(
        unwrapDI<DIType>(PointeeTy), SizeInBits, AlignInBits, Name));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateStructType(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef File,
    unsigned LineNumber,
    uint64_t SizeInBits,
    uint64_t AlignInBits,
    unsigned Flags,
    LLVMValueRef DerivedFrom,
    LLVMValueRef Elements,
    unsigned RunTimeLang,
    LLVMValueRef VTableHolder,
    const char *UniqueId) {
    return wrap(Builder->createStructType(
        unwrapDI<DIDescriptor>(Scope),
        Name,
        unwrapDI<DIFile>(File),
        LineNumber,
        SizeInBits,
        AlignInBits,
        Flags,
        unwrapDI<DIType>(DerivedFrom),
        unwrapDI<DIArray>(Elements),
        RunTimeLang,
        unwrapDI<DIType>(VTableHolder)
#if LLVM_VERSION_MINOR >= 4
        ,UniqueId
#endif
        ));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateMemberType(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef File,
    unsigned LineNo,
    uint64_t SizeInBits,
    uint64_t AlignInBits,
    uint64_t OffsetInBits,
    unsigned Flags,
    LLVMValueRef Ty) {
    return wrap(Builder->createMemberType(
        unwrapDI<DIDescriptor>(Scope), Name,
        unwrapDI<DIFile>(File), LineNo,
        SizeInBits, AlignInBits, OffsetInBits, Flags,
        unwrapDI<DIType>(Ty)));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateLexicalBlock(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    LLVMValueRef File,
    unsigned Line,
    unsigned Col) {
    return wrap(Builder->createLexicalBlock(
        unwrapDI<DIDescriptor>(Scope),
        unwrapDI<DIFile>(File), Line, Col
#if LLVM_VERSION_MINOR == 5
        , 0
#endif
        ));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateStaticVariable(
    DIBuilderRef Builder,
    LLVMValueRef Context,
    const char* Name,
    const char* LinkageName,
    LLVMValueRef File,
    unsigned LineNo,
    LLVMValueRef Ty,
    bool isLocalToUnit,
    LLVMValueRef Val,
    LLVMValueRef Decl = NULL) {
#if LLVM_VERSION_MINOR == 6
    return wrap(Builder->createGlobalVariable(unwrapDI<DIDescriptor>(Context),
#else
    return wrap(Builder->createStaticVariable(unwrapDI<DIDescriptor>(Context),
#endif
        Name,
        LinkageName,
        unwrapDI<DIFile>(File),
        LineNo,
        unwrapDI<DIType>(Ty),
        isLocalToUnit,
        unwrap(Val),
        unwrapDI<MDNode*>(Decl)));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateLocalVariable(
    DIBuilderRef Builder,
    unsigned Tag,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef File,
    unsigned LineNo,
    LLVMValueRef Ty,
    bool AlwaysPreserve,
    unsigned Flags,
    unsigned ArgNo) {
    return wrap(Builder->createLocalVariable(Tag,
        unwrapDI<DIDescriptor>(Scope), Name,
        unwrapDI<DIFile>(File),
        LineNo,
        unwrapDI<DIType>(Ty), AlwaysPreserve, Flags, ArgNo));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateArrayType(
    DIBuilderRef Builder,
    uint64_t Size,
    uint64_t AlignInBits,
    LLVMValueRef Ty,
    LLVMValueRef Subscripts) {
    return wrap(Builder->createArrayType(Size, AlignInBits,
        unwrapDI<DIType>(Ty),
        unwrapDI<DIArray>(Subscripts)));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateVectorType(
    DIBuilderRef Builder,
    uint64_t Size,
    uint64_t AlignInBits,
    LLVMValueRef Ty,
    LLVMValueRef Subscripts) {
    return wrap(Builder->createVectorType(Size, AlignInBits,
        unwrapDI<DIType>(Ty),
        unwrapDI<DIArray>(Subscripts)));
}

extern "C" LLVMValueRef LLVMDIBuilderGetOrCreateSubrange(
    DIBuilderRef Builder,
    int64_t Lo,
    int64_t Count) {
    return wrap(Builder->getOrCreateSubrange(Lo, Count));
}

extern "C" LLVMValueRef LLVMDIBuilderGetOrCreateArray(
    DIBuilderRef Builder,
    LLVMValueRef* Ptr,
    unsigned Count) {
    return wrap(Builder->getOrCreateArray(
        ArrayRef<Value*>(reinterpret_cast<Value**>(Ptr), Count)));
}

extern "C" LLVMValueRef LLVMDIBuilderInsertDeclareAtEnd(
    DIBuilderRef Builder,
    LLVMValueRef Val,
    LLVMValueRef VarInfo,
    LLVMBasicBlockRef InsertAtEnd) {
    return wrap(Builder->insertDeclare(
        unwrap(Val),
        unwrapDI<DIVariable>(VarInfo),
        unwrap(InsertAtEnd)));
}

extern "C" LLVMValueRef LLVMDIBuilderInsertDeclareBefore(
    DIBuilderRef Builder,
    LLVMValueRef Val,
    LLVMValueRef VarInfo,
    LLVMValueRef InsertBefore) {
    return wrap(Builder->insertDeclare(
        unwrap(Val),
        unwrapDI<DIVariable>(VarInfo),
        unwrap<Instruction>(InsertBefore)));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateEnumerator(
    DIBuilderRef Builder,
    const char* Name,
    uint64_t Val)
{
    return wrap(Builder->createEnumerator(Name, Val));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateEnumerationType(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef File,
    unsigned LineNumber,
    uint64_t SizeInBits,
    uint64_t AlignInBits,
    LLVMValueRef Elements,
    LLVMValueRef ClassType)
{
    return wrap(Builder->createEnumerationType(
        unwrapDI<DIDescriptor>(Scope),
        Name,
        unwrapDI<DIFile>(File),
        LineNumber,
        SizeInBits,
        AlignInBits,
        unwrapDI<DIArray>(Elements),
        unwrapDI<DIType>(ClassType)));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateUnionType(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef File,
    unsigned LineNumber,
    uint64_t SizeInBits,
    uint64_t AlignInBits,
    unsigned Flags,
    LLVMValueRef Elements,
    unsigned RunTimeLang,
    const char* UniqueId)
{
    return wrap(Builder->createUnionType(
        unwrapDI<DIDescriptor>(Scope),
        Name,
        unwrapDI<DIFile>(File),
        LineNumber,
        SizeInBits,
        AlignInBits,
        Flags,
        unwrapDI<DIArray>(Elements),
        RunTimeLang
#if LLVM_VERSION_MINOR >= 4
        ,UniqueId
#endif
        ));
}

#if LLVM_VERSION_MINOR < 5
extern "C" void LLVMSetUnnamedAddr(LLVMValueRef Value, LLVMBool Unnamed) {
    unwrap<GlobalValue>(Value)->setUnnamedAddr(Unnamed);
}
#endif

extern "C" LLVMValueRef LLVMDIBuilderCreateTemplateTypeParameter(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef Ty,
    LLVMValueRef File,
    unsigned LineNo,
    unsigned ColumnNo)
{
    return wrap(Builder->createTemplateTypeParameter(
      unwrapDI<DIDescriptor>(Scope),
      Name,
      unwrapDI<DIType>(Ty),
      unwrapDI<MDNode*>(File),
      LineNo,
      ColumnNo));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateOpDeref(LLVMTypeRef IntTy)
{
    return LLVMConstInt(IntTy, DIBuilder::OpDeref, true);
}

extern "C" LLVMValueRef LLVMDIBuilderCreateOpPlus(LLVMTypeRef IntTy)
{
    return LLVMConstInt(IntTy, DIBuilder::OpPlus, true);
}

extern "C" LLVMValueRef LLVMDIBuilderCreateComplexVariable(
    DIBuilderRef Builder,
    unsigned Tag,
    LLVMValueRef Scope,
    const char *Name,
    LLVMValueRef File,
    unsigned LineNo,
    LLVMValueRef Ty,
    LLVMValueRef* AddrOps,
    unsigned AddrOpsCount,
    unsigned ArgNo)
{
    llvm::ArrayRef<llvm::Value*> addr_ops((llvm::Value**)AddrOps, AddrOpsCount);

    return wrap(Builder->createComplexVariable(
        Tag,
        unwrapDI<DIDescriptor>(Scope),
        Name,
        unwrapDI<DIFile>(File),
        LineNo,
        unwrapDI<DIType>(Ty),
        addr_ops,
        ArgNo
    ));
}

extern "C" LLVMValueRef LLVMDIBuilderCreateNameSpace(
    DIBuilderRef Builder,
    LLVMValueRef Scope,
    const char* Name,
    LLVMValueRef File,
    unsigned LineNo)
{
    return wrap(Builder->createNameSpace(
        unwrapDI<DIDescriptor>(Scope),
        Name,
        unwrapDI<DIFile>(File),
        LineNo));
}

extern "C" void LLVMDICompositeTypeSetTypeArray(
    LLVMValueRef CompositeType,
    LLVMValueRef TypeArray)
{
#if LLVM_VERSION_MINOR >= 6
    unwrapDI<DICompositeType>(CompositeType).setArrays(unwrapDI<DIArray>(TypeArray));
#else
    unwrapDI<DICompositeType>(CompositeType).setTypeArray(unwrapDI<DIArray>(TypeArray));
#endif
}

extern "C" void LLVMWriteTypeToString(LLVMTypeRef Type, RustStringRef str) {
    raw_rust_string_ostream os(str);
    unwrap<llvm::Type>(Type)->print(os);
}

extern "C" void LLVMWriteValueToString(LLVMValueRef Value, RustStringRef str) {
    raw_rust_string_ostream os(str);
    os << "(";
    unwrap<llvm::Value>(Value)->getType()->print(os);
    os << ":";
    unwrap<llvm::Value>(Value)->print(os);
    os << ")";
}

#if LLVM_VERSION_MINOR >= 5
extern "C" bool
LLVMRustLinkInExternalBitcode(LLVMModuleRef dst, char *bc, size_t len) {
    Module *Dst = unwrap(dst);
#if LLVM_VERSION_MINOR == 5
    MemoryBuffer* buf = MemoryBuffer::getMemBufferCopy(StringRef(bc, len));
    ErrorOr<Module *> Src = llvm::getLazyBitcodeModule(buf, Dst->getContext());
#else
    std::unique_ptr<MemoryBuffer> buf = MemoryBuffer::getMemBufferCopy(StringRef(bc, len));
    ErrorOr<Module *> Src = llvm::getLazyBitcodeModule(std::move(buf), Dst->getContext());
#endif
    if (!Src) {
        LLVMRustSetLastError(Src.getError().message().c_str());
#if LLVM_VERSION_MINOR == 5
        delete buf;
#endif
        return false;
    }

    std::string Err;
    if (Linker::LinkModules(Dst, *Src, Linker::DestroySource, &Err)) {
        LLVMRustSetLastError(Err.c_str());
        return false;
    }
    return true;
}
#else
extern "C" bool
LLVMRustLinkInExternalBitcode(LLVMModuleRef dst, char *bc, size_t len) {
    Module *Dst = unwrap(dst);
    MemoryBuffer* buf = MemoryBuffer::getMemBufferCopy(StringRef(bc, len));
    std::string Err;
    Module *Src = llvm::getLazyBitcodeModule(buf, Dst->getContext(), &Err);
    if (!Src) {
        LLVMRustSetLastError(Err.c_str());
        delete buf;
        return false;
    }

    if (Linker::LinkModules(Dst, Src, Linker::DestroySource, &Err)) {
        LLVMRustSetLastError(Err.c_str());
        return false;
    }
    return true;
}
#endif

#if LLVM_VERSION_MINOR >= 5
extern "C" void*
LLVMRustOpenArchive(char *path) {
    ErrorOr<std::unique_ptr<MemoryBuffer>> buf_or = MemoryBuffer::getFile(path,
                                                                          -1,
                                                                          false);
    if (!buf_or) {
        LLVMRustSetLastError(buf_or.getError().message().c_str());
        return nullptr;
    }

#if LLVM_VERSION_MINOR >= 6
    ErrorOr<std::unique_ptr<Archive>> archive_or =
        Archive::create(buf_or.get()->getMemBufferRef());

    if (!archive_or) {
        LLVMRustSetLastError(archive_or.getError().message().c_str());
        return nullptr;
    }

    OwningBinary<Archive> *ret = new OwningBinary<Archive>(
            std::move(archive_or.get()), std::move(buf_or.get()));
#else
    std::error_code err;
    Archive *ret = new Archive(std::move(buf_or.get()), err);
    if (err) {
        LLVMRustSetLastError(err.message().c_str());
        return nullptr;
    }
#endif

    return ret;
}
#else
extern "C" void*
LLVMRustOpenArchive(char *path) {
    OwningPtr<MemoryBuffer> buf;
    error_code err = MemoryBuffer::getFile(path, buf, -1, false);
    if (err) {
        LLVMRustSetLastError(err.message().c_str());
        return NULL;
    }
    Archive *ret = new Archive(buf.take(), err);
    if (err) {
        LLVMRustSetLastError(err.message().c_str());
        return NULL;
    }
    return ret;
}
#endif

extern "C" const char*
#if LLVM_VERSION_MINOR >= 6
LLVMRustArchiveReadSection(OwningBinary<Archive> *ob, char *name, size_t *size) {

    std::unique_ptr<Archive> &ar = ob->getBinary();
#else
LLVMRustArchiveReadSection(Archive *ar, char *name, size_t *size) {
#endif

#if LLVM_VERSION_MINOR >= 5
    Archive::child_iterator child = ar->child_begin(),
                              end = ar->child_end();
    for (; child != end; ++child) {
        ErrorOr<StringRef> name_or_err = child->getName();
        if (name_or_err.getError()) continue;
        StringRef sect_name = name_or_err.get();
#else
    Archive::child_iterator child = ar->begin_children(),
                              end = ar->end_children();
    for (; child != end; ++child) {
        StringRef sect_name;
        error_code err = child->getName(sect_name);
        if (err) continue;
#endif
        if (sect_name.trim(" ") == name) {
            StringRef buf = child->getBuffer();
            *size = buf.size();
            return buf.data();
        }
    }
    return NULL;
}

extern "C" void
#if LLVM_VERSION_MINOR >= 6
LLVMRustDestroyArchive(OwningBinary<Archive> *ar) {
#else
LLVMRustDestroyArchive(Archive *ar) {
#endif
    delete ar;
}

#if LLVM_VERSION_MINOR >= 5
extern "C" void
LLVMRustSetDLLExportStorageClass(LLVMValueRef Value) {
    GlobalValue *V = unwrap<GlobalValue>(Value);
    V->setDLLStorageClass(GlobalValue::DLLExportStorageClass);
}
#else
extern "C" void
LLVMRustSetDLLExportStorageClass(LLVMValueRef Value) {
    LLVMSetLinkage(Value, LLVMDLLExportLinkage);
}
#endif

extern "C" int
LLVMVersionMinor() {
    return LLVM_VERSION_MINOR;
}

extern "C" int
LLVMVersionMajor() {
    return LLVM_VERSION_MAJOR;
}

// Note that the two following functions look quite similar to the
// LLVMGetSectionName function. Sadly, it appears that this function only
// returns a char* pointer, which isn't guaranteed to be null-terminated. The
// function provided by LLVM doesn't return the length, so we've created our own
// function which returns the length as well as the data pointer.
//
// For an example of this not returning a null terminated string, see
// lib/Object/COFFObjectFile.cpp in the getSectionName function. One of the
// branches explicitly creates a StringRef without a null terminator, and then
// that's returned.

inline section_iterator *unwrap(LLVMSectionIteratorRef SI) {
    return reinterpret_cast<section_iterator*>(SI);
}

extern "C" int
LLVMRustGetSectionName(LLVMSectionIteratorRef SI, const char **ptr) {
    StringRef ret;
#if LLVM_VERSION_MINOR >= 5
    if (std::error_code ec = (*unwrap(SI))->getName(ret))
#else
    if (error_code ec = (*unwrap(SI))->getName(ret))
#endif
      report_fatal_error(ec.message());
    *ptr = ret.data();
    return ret.size();
}

// LLVMArrayType function does not support 64-bit ElementCount
extern "C" LLVMTypeRef
LLVMRustArrayType(LLVMTypeRef ElementType, uint64_t ElementCount) {
    return wrap(ArrayType::get(unwrap(ElementType), ElementCount));
}

DEFINE_SIMPLE_CONVERSION_FUNCTIONS(Twine, LLVMTwineRef)
DEFINE_SIMPLE_CONVERSION_FUNCTIONS(DebugLoc, LLVMDebugLocRef)

extern "C" void
LLVMWriteTwineToString(LLVMTwineRef T, RustStringRef str) {
    raw_rust_string_ostream os(str);
    unwrap(T)->print(os);
}

extern "C" void
LLVMUnpackOptimizationDiagnostic(
    LLVMDiagnosticInfoRef di,
    const char **pass_name_out,
    LLVMValueRef *function_out,
    LLVMDebugLocRef *debugloc_out,
    LLVMTwineRef *message_out)
{
    // Undefined to call this not on an optimization diagnostic!
    llvm::DiagnosticInfoOptimizationBase *opt
        = static_cast<llvm::DiagnosticInfoOptimizationBase*>(unwrap(di));

    *pass_name_out = opt->getPassName();
    *function_out = wrap(&opt->getFunction());
    *debugloc_out = wrap(&opt->getDebugLoc());
    *message_out = wrap(&opt->getMsg());
}

extern "C" void LLVMWriteDiagnosticInfoToString(LLVMDiagnosticInfoRef di, RustStringRef str) {
    raw_rust_string_ostream os(str);
    DiagnosticPrinterRawOStream dp(os);
    unwrap(di)->print(dp);
}

extern "C" int LLVMGetDiagInfoKind(LLVMDiagnosticInfoRef di) {
    return unwrap(di)->getKind();
}

extern "C" void LLVMWriteDebugLocToString(
    LLVMContextRef C,
    LLVMDebugLocRef dl,
    RustStringRef str)
{
    raw_rust_string_ostream os(str);
    unwrap(dl)->print(*unwrap(C), os);
}

DEFINE_SIMPLE_CONVERSION_FUNCTIONS(SMDiagnostic, LLVMSMDiagnosticRef)

extern "C" void LLVMSetInlineAsmDiagnosticHandler(
    LLVMContextRef C,
    LLVMContext::InlineAsmDiagHandlerTy H,
    void *CX)
{
    unwrap(C)->setInlineAsmDiagnosticHandler(H, CX);
}

extern "C" void LLVMWriteSMDiagnosticToString(LLVMSMDiagnosticRef d, RustStringRef str) {
    raw_rust_string_ostream os(str);
    unwrap(d)->print("", os);
}
