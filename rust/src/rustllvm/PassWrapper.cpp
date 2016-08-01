// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#include <stdio.h>

#include "rustllvm.h"

#include "llvm/Support/CBindingWrapping.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Host.h"
#include "llvm/Analysis/TargetLibraryInfo.h"
#include "llvm/Analysis/TargetTransformInfo.h"
#include "llvm/Target/TargetMachine.h"
#include "llvm/Target/TargetSubtargetInfo.h"
#include "llvm/Transforms/IPO/PassManagerBuilder.h"


#include "llvm-c/Transforms/PassManagerBuilder.h"

using namespace llvm;
using namespace llvm::legacy;

extern cl::opt<bool> EnableARMEHABI;

typedef struct LLVMOpaquePass *LLVMPassRef;
typedef struct LLVMOpaqueTargetMachine *LLVMTargetMachineRef;

DEFINE_STDCXX_CONVERSION_FUNCTIONS(Pass, LLVMPassRef)
DEFINE_STDCXX_CONVERSION_FUNCTIONS(TargetMachine, LLVMTargetMachineRef)
DEFINE_STDCXX_CONVERSION_FUNCTIONS(PassManagerBuilder, LLVMPassManagerBuilderRef)

extern "C" void
LLVMInitializePasses() {
  PassRegistry &Registry = *PassRegistry::getPassRegistry();
  initializeCore(Registry);
  initializeCodeGen(Registry);
  initializeScalarOpts(Registry);
  initializeVectorization(Registry);
  initializeIPO(Registry);
  initializeAnalysis(Registry);
#if LLVM_VERSION_MINOR == 7
  initializeIPA(Registry);
#endif
  initializeTransformUtils(Registry);
  initializeInstCombine(Registry);
  initializeInstrumentation(Registry);
  initializeTarget(Registry);
}


enum class SupportedPassKind {
  Function,
  Module,
  Unsupported
};

extern "C" Pass*
LLVMRustFindAndCreatePass(const char *PassName) {
    StringRef SR(PassName);
    PassRegistry *PR = PassRegistry::getPassRegistry();

    const PassInfo *PI = PR->getPassInfo(SR);
    if (PI) {
        return PI->createPass();
    }
    return NULL;
}

extern "C" SupportedPassKind
LLVMRustPassKind(Pass *pass) {
    assert(pass);
    PassKind passKind = pass->getPassKind();
    if (passKind == PT_Module) {
        return SupportedPassKind::Module;
    } else if (passKind == PT_Function) {
        return SupportedPassKind::Function;
    } else {
        return SupportedPassKind::Unsupported;
    }
}

extern "C" void
LLVMRustAddPass(LLVMPassManagerRef PM, Pass *pass) {
    assert(pass);
    PassManagerBase *pm = unwrap(PM);
    pm->add(pass);
}

#ifdef LLVM_COMPONENT_X86
#define SUBTARGET_X86 SUBTARGET(X86)
#else
#define SUBTARGET_X86
#endif

#ifdef LLVM_COMPONENT_ARM
#define SUBTARGET_ARM SUBTARGET(ARM)
#else
#define SUBTARGET_ARM
#endif

#ifdef LLVM_COMPONENT_AARCH64
#define SUBTARGET_AARCH64 SUBTARGET(AArch64)
#else
#define SUBTARGET_AARCH64
#endif

#ifdef LLVM_COMPONENT_MIPS
#define SUBTARGET_MIPS SUBTARGET(Mips)
#else
#define SUBTARGET_MIPS
#endif

#ifdef LLVM_COMPONENT_POWERPC
#define SUBTARGET_PPC SUBTARGET(PPC)
#else
#define SUBTARGET_PPC
#endif

#define GEN_SUBTARGETS    \
        SUBTARGET_X86     \
        SUBTARGET_ARM     \
        SUBTARGET_AARCH64 \
        SUBTARGET_MIPS    \
        SUBTARGET_PPC

#define SUBTARGET(x) namespace llvm {                \
    extern const SubtargetFeatureKV x##FeatureKV[];  \
    extern const SubtargetFeatureKV x##SubTypeKV[];  \
  }

GEN_SUBTARGETS
#undef SUBTARGET

extern "C" bool
LLVMRustHasFeature(LLVMTargetMachineRef TM,
		   const char *feature) {
    TargetMachine *Target = unwrap(TM);
    const MCSubtargetInfo *MCInfo = Target->getMCSubtargetInfo();
    const FeatureBitset &Bits = MCInfo->getFeatureBits();
    const llvm::SubtargetFeatureKV *FeatureEntry;

#define SUBTARGET(x)                                        \
    if (MCInfo->isCPUStringValid(x##SubTypeKV[0].Key)) {    \
        FeatureEntry = x##FeatureKV;                       \
    } else

    GEN_SUBTARGETS {
        return false;
    }
#undef SUBTARGET

    while (strcmp(feature, FeatureEntry->Key) != 0)
        FeatureEntry++;

    return (Bits & FeatureEntry->Value) == FeatureEntry->Value;
}

extern "C" LLVMTargetMachineRef
LLVMRustCreateTargetMachine(const char *triple,
                            const char *cpu,
                            const char *feature,
                            CodeModel::Model CM,
                            LLVMRelocMode Reloc,
                            CodeGenOpt::Level OptLevel,
                            bool UseSoftFloat,
                            bool PositionIndependentExecutable,
                            bool FunctionSections,
                            bool DataSections) {

#if LLVM_VERSION_MINOR <= 8
    Reloc::Model RM;
#else
    Optional<Reloc::Model> RM;
#endif
    switch (Reloc){
        case LLVMRelocStatic:
            RM = Reloc::Static;
            break;
        case LLVMRelocPIC:
            RM = Reloc::PIC_;
            break;
        case LLVMRelocDynamicNoPic:
            RM = Reloc::DynamicNoPIC;
            break;
        default:
#if LLVM_VERSION_MINOR <= 8
            RM = Reloc::Default;
#endif
            break;
    }

    std::string Error;
    Triple Trip(Triple::normalize(triple));
    const llvm::Target *TheTarget = TargetRegistry::lookupTarget(Trip.getTriple(),
                                                                 Error);
    if (TheTarget == NULL) {
        LLVMRustSetLastError(Error.c_str());
        return NULL;
    }

    StringRef real_cpu = cpu;
    if (real_cpu == "native") {
        real_cpu = sys::getHostCPUName();
    }

    TargetOptions Options;
#if LLVM_VERSION_MINOR <= 8
    Options.PositionIndependentExecutable = PositionIndependentExecutable;
#endif

    Options.FloatABIType = FloatABI::Default;
    if (UseSoftFloat) {
        Options.FloatABIType = FloatABI::Soft;
    }
    Options.DataSections = DataSections;
    Options.FunctionSections = FunctionSections;

    TargetMachine *TM = TheTarget->createTargetMachine(Trip.getTriple(),
                                                       real_cpu,
                                                       feature,
                                                       Options,
                                                       RM,
                                                       CM,
                                                       OptLevel);
    return wrap(TM);
}

extern "C" void
LLVMRustDisposeTargetMachine(LLVMTargetMachineRef TM) {
    delete unwrap(TM);
}

// Unfortunately, LLVM doesn't expose a C API to add the corresponding analysis
// passes for a target to a pass manager. We export that functionality through
// this function.
extern "C" void
LLVMRustAddAnalysisPasses(LLVMTargetMachineRef TM,
                          LLVMPassManagerRef PMR,
                          LLVMModuleRef M) {
    PassManagerBase *PM = unwrap(PMR);
    PM->add(createTargetTransformInfoWrapperPass(
          unwrap(TM)->getTargetIRAnalysis()));
}

extern "C" void
LLVMRustConfigurePassManagerBuilder(LLVMPassManagerBuilderRef PMB,
                                    CodeGenOpt::Level OptLevel,
                                    bool MergeFunctions,
                                    bool SLPVectorize,
                                    bool LoopVectorize) {
    // Ignore mergefunc for now as enabling it causes crashes.
    //unwrap(PMB)->MergeFunctions = MergeFunctions;
    unwrap(PMB)->SLPVectorize = SLPVectorize;
    unwrap(PMB)->OptLevel = OptLevel;
    unwrap(PMB)->LoopVectorize = LoopVectorize;
}

// Unfortunately, the LLVM C API doesn't provide a way to set the `LibraryInfo`
// field of a PassManagerBuilder, we expose our own method of doing so.
extern "C" void
LLVMRustAddBuilderLibraryInfo(LLVMPassManagerBuilderRef PMB,
                              LLVMModuleRef M,
                              bool DisableSimplifyLibCalls) {
    Triple TargetTriple(unwrap(M)->getTargetTriple());
    TargetLibraryInfoImpl *TLI = new TargetLibraryInfoImpl(TargetTriple);
    if (DisableSimplifyLibCalls)
      TLI->disableAllFunctions();
    unwrap(PMB)->LibraryInfo = TLI;
}

// Unfortunately, the LLVM C API doesn't provide a way to create the
// TargetLibraryInfo pass, so we use this method to do so.
extern "C" void
LLVMRustAddLibraryInfo(LLVMPassManagerRef PMB,
                       LLVMModuleRef M,
                       bool DisableSimplifyLibCalls) {
    Triple TargetTriple(unwrap(M)->getTargetTriple());
    TargetLibraryInfoImpl TLII(TargetTriple);
    if (DisableSimplifyLibCalls)
      TLII.disableAllFunctions();
    unwrap(PMB)->add(new TargetLibraryInfoWrapperPass(TLII));
}

// Unfortunately, the LLVM C API doesn't provide an easy way of iterating over
// all the functions in a module, so we do that manually here. You'll find
// similar code in clang's BackendUtil.cpp file.
extern "C" void
LLVMRustRunFunctionPassManager(LLVMPassManagerRef PM, LLVMModuleRef M) {
    llvm::legacy::FunctionPassManager *P = unwrap<llvm::legacy::FunctionPassManager>(PM);
    P->doInitialization();
    for (Module::iterator I = unwrap(M)->begin(),
         E = unwrap(M)->end(); I != E; ++I)
        if (!I->isDeclaration())
            P->run(*I);
    P->doFinalization();
}

extern "C" void
LLVMRustSetLLVMOptions(int Argc, char **Argv) {
    // Initializing the command-line options more than once is not allowed. So,
    // check if they've already been initialized.  (This could happen if we're
    // being called from rustpkg, for example). If the arguments change, then
    // that's just kinda unfortunate.
    static bool initialized = false;
    if (initialized) return;
    initialized = true;
    cl::ParseCommandLineOptions(Argc, Argv);
}

extern "C" bool
LLVMRustWriteOutputFile(LLVMTargetMachineRef Target,
                        LLVMPassManagerRef PMR,
                        LLVMModuleRef M,
                        const char *path,
                        TargetMachine::CodeGenFileType FileType) {
  llvm::legacy::PassManager *PM = unwrap<llvm::legacy::PassManager>(PMR);

  std::string ErrorInfo;
  std::error_code EC;
  raw_fd_ostream OS(path, EC, sys::fs::F_None);
  if (EC)
    ErrorInfo = EC.message();
  if (ErrorInfo != "") {
    LLVMRustSetLastError(ErrorInfo.c_str());
    return false;
  }

  unwrap(Target)->addPassesToEmitFile(*PM, OS, FileType, false);
  PM->run(*unwrap(M));

  // Apparently `addPassesToEmitFile` adds a pointer to our on-the-stack output
  // stream (OS), so the only real safe place to delete this is here? Don't we
  // wish this was written in Rust?
  delete PM;
  return true;
}

extern "C" void
LLVMRustPrintModule(LLVMPassManagerRef PMR,
                    LLVMModuleRef M,
                    const char* path) {
  llvm::legacy::PassManager *PM = unwrap<llvm::legacy::PassManager>(PMR);
  std::string ErrorInfo;

  std::error_code EC;
  raw_fd_ostream OS(path, EC, sys::fs::F_None);
  if (EC)
    ErrorInfo = EC.message();

  formatted_raw_ostream FOS(OS);

  PM->add(createPrintModulePass(FOS));

  PM->run(*unwrap(M));
}

extern "C" void
LLVMRustPrintPasses() {
    LLVMInitializePasses();
    struct MyListener : PassRegistrationListener {
        void passEnumerate(const PassInfo *info) {
            if (info->getPassArgument() && *info->getPassArgument()) {
                printf("%15s - %s\n", info->getPassArgument(),
                       info->getPassName());
            }
        }
    } listener;

    PassRegistry *PR = PassRegistry::getPassRegistry();
    PR->enumerateWith(&listener);
}

extern "C" void
LLVMRustAddAlwaysInlinePass(LLVMPassManagerBuilderRef PMB, bool AddLifetimes) {
    unwrap(PMB)->Inliner = createAlwaysInlinerPass(AddLifetimes);
}

extern "C" void
LLVMRustRunRestrictionPass(LLVMModuleRef M, char **symbols, size_t len) {
    llvm::legacy::PassManager passes;

#if LLVM_VERSION_MINOR <= 8
    ArrayRef<const char*> ref(symbols, len);
    passes.add(llvm::createInternalizePass(ref));
#else
    auto PreserveFunctions = [=](const GlobalValue &GV) {
        for (size_t i=0; i<len; i++) {
            if (GV.getName() == symbols[i]) {
                return true;
            }
        }
        return false;
    };

    passes.add(llvm::createInternalizePass(PreserveFunctions));
#endif

    passes.run(*unwrap(M));
}

extern "C" void
LLVMRustMarkAllFunctionsNounwind(LLVMModuleRef M) {
    for (Module::iterator GV = unwrap(M)->begin(),
         E = unwrap(M)->end(); GV != E; ++GV) {
        GV->setDoesNotThrow();
        Function *F = dyn_cast<Function>(GV);
        if (F == NULL)
            continue;

        for (Function::iterator B = F->begin(), BE = F->end(); B != BE; ++B) {
            for (BasicBlock::iterator I = B->begin(), IE = B->end();
                 I != IE; ++I) {
                if (isa<InvokeInst>(I)) {
                    InvokeInst *CI = cast<InvokeInst>(I);
                    CI->setDoesNotThrow();
                }
            }
        }
    }
}

extern "C" void
LLVMRustSetDataLayoutFromTargetMachine(LLVMModuleRef Module,
                                       LLVMTargetMachineRef TMR) {
    TargetMachine *Target = unwrap(TMR);
    unwrap(Module)->setDataLayout(Target->createDataLayout());
}

extern "C" LLVMTargetDataRef
LLVMRustGetModuleDataLayout(LLVMModuleRef M) {
    return wrap(&unwrap(M)->getDataLayout());
}

extern "C" void
LLVMRustSetModulePIELevel(LLVMModuleRef M) {
#if LLVM_VERSION_MINOR >= 9
    unwrap(M)->setPIELevel(PIELevel::Level::Large);
#endif
}
