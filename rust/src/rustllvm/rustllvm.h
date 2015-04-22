// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/InlineAsm.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/PassManager.h"
#include "llvm/IR/InlineAsm.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/Analysis/Passes.h"
#include "llvm/Analysis/Lint.h"
#include "llvm/ADT/ArrayRef.h"
#include "llvm/ADT/Triple.h"
#include "llvm/ADT/DenseSet.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/FormattedStream.h"
#include "llvm/Support/Timer.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Support/TargetSelect.h"
#include "llvm/Support/TargetRegistry.h"
#include "llvm/Support/SourceMgr.h"
#include "llvm/Support/Host.h"
#include "llvm/Support/Debug.h"
#include "llvm/Support/DynamicLibrary.h"
#include "llvm/Support/Memory.h"
#include "llvm/ExecutionEngine/ExecutionEngine.h"
#include "llvm/ExecutionEngine/MCJIT.h"
#include "llvm/ExecutionEngine/Interpreter.h"
#include "llvm/Target/TargetMachine.h"
#include "llvm/Target/TargetOptions.h"
#include "llvm/Transforms/Scalar.h"
#include "llvm/Transforms/IPO.h"
#include "llvm/Transforms/Instrumentation.h"
#include "llvm/Transforms/Vectorize.h"
#include "llvm/Bitcode/ReaderWriter.h"
#include "llvm-c/Core.h"
#include "llvm-c/BitReader.h"
#include "llvm-c/ExecutionEngine.h"
#include "llvm-c/Object.h"

#include "llvm/IR/IRPrintingPasses.h"
#include "llvm/IR/DebugInfo.h"
#include "llvm/IR/DIBuilder.h"
#include "llvm/Linker/Linker.h"

// Used by RustMCJITMemoryManager::getPointerToNamedFunction()
// to get around glibc issues. See the function for more information.
#ifdef __linux__
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#endif

void LLVMRustSetLastError(const char*);

typedef struct OpaqueRustString *RustStringRef;
typedef struct LLVMOpaqueTwine *LLVMTwineRef;
typedef struct LLVMOpaqueDebugLoc *LLVMDebugLocRef;
typedef struct LLVMOpaqueSMDiagnostic *LLVMSMDiagnosticRef;
typedef struct LLVMOpaqueRustJITMemoryManager *LLVMRustJITMemoryManagerRef;

extern "C" void
rust_llvm_string_write_impl(RustStringRef str, const char *ptr, size_t size);

class raw_rust_string_ostream : public llvm::raw_ostream  {
    RustStringRef str;
    uint64_t pos;

    void write_impl(const char *ptr, size_t size) override {
        rust_llvm_string_write_impl(str, ptr, size);
        pos += size;
    }

    uint64_t current_pos() const override {
        return pos;
    }

public:
    explicit raw_rust_string_ostream(RustStringRef str)
        : str(str), pos(0) { }

    ~raw_rust_string_ostream() {
        // LLVM requires this.
        flush();
    }
};
