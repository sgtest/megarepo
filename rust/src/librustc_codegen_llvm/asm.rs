use crate::builder::Builder;
use crate::context::CodegenCx;
use crate::llvm;
use crate::type_of::LayoutLlvmExt;
use crate::value::Value;

use rustc_codegen_ssa::mir::operand::OperandValue;
use rustc_codegen_ssa::mir::place::PlaceRef;
use rustc_codegen_ssa::traits::*;
use rustc_hir as hir;
use rustc_span::Span;

use libc::{c_char, c_uint};
use log::debug;

impl AsmBuilderMethods<'tcx> for Builder<'a, 'll, 'tcx> {
    fn codegen_llvm_inline_asm(
        &mut self,
        ia: &hir::LlvmInlineAsmInner,
        outputs: Vec<PlaceRef<'tcx, &'ll Value>>,
        mut inputs: Vec<&'ll Value>,
        span: Span,
    ) -> bool {
        let mut ext_constraints = vec![];
        let mut output_types = vec![];

        // Prepare the output operands
        let mut indirect_outputs = vec![];
        for (i, (out, &place)) in ia.outputs.iter().zip(&outputs).enumerate() {
            if out.is_rw {
                let operand = self.load_operand(place);
                if let OperandValue::Immediate(_) = operand.val {
                    inputs.push(operand.immediate());
                }
                ext_constraints.push(i.to_string());
            }
            if out.is_indirect {
                let operand = self.load_operand(place);
                if let OperandValue::Immediate(_) = operand.val {
                    indirect_outputs.push(operand.immediate());
                }
            } else {
                output_types.push(place.layout.llvm_type(self.cx()));
            }
        }
        if !indirect_outputs.is_empty() {
            indirect_outputs.extend_from_slice(&inputs);
            inputs = indirect_outputs;
        }

        let clobbers = ia.clobbers.iter().map(|s| format!("~{{{}}}", &s));

        // Default per-arch clobbers
        // Basically what clang does
        let arch_clobbers = match &self.sess().target.target.arch[..] {
            "x86" | "x86_64" => vec!["~{dirflag}", "~{fpsr}", "~{flags}"],
            "mips" | "mips64" => vec!["~{$1}"],
            _ => Vec::new(),
        };

        let all_constraints = ia
            .outputs
            .iter()
            .map(|out| out.constraint.to_string())
            .chain(ia.inputs.iter().map(|s| s.to_string()))
            .chain(ext_constraints)
            .chain(clobbers)
            .chain(arch_clobbers.iter().map(|s| (*s).to_string()))
            .collect::<Vec<String>>()
            .join(",");

        debug!("Asm Constraints: {}", &all_constraints);

        // Depending on how many outputs we have, the return type is different
        let num_outputs = output_types.len();
        let output_type = match num_outputs {
            0 => self.type_void(),
            1 => output_types[0],
            _ => self.type_struct(&output_types, false),
        };

        let asm = ia.asm.as_str();
        let r = inline_asm_call(
            self,
            &asm,
            &all_constraints,
            &inputs,
            output_type,
            ia.volatile,
            ia.alignstack,
            ia.dialect,
        );
        if r.is_none() {
            return false;
        }
        let r = r.unwrap();

        // Again, based on how many outputs we have
        let outputs = ia.outputs.iter().zip(&outputs).filter(|&(ref o, _)| !o.is_indirect);
        for (i, (_, &place)) in outputs.enumerate() {
            let v = if num_outputs == 1 { r } else { self.extract_value(r, i as u64) };
            OperandValue::Immediate(v).store(self, place);
        }

        // Store mark in a metadata node so we can map LLVM errors
        // back to source locations.  See #17552.
        unsafe {
            let key = "srcloc";
            let kind = llvm::LLVMGetMDKindIDInContext(
                self.llcx,
                key.as_ptr() as *const c_char,
                key.len() as c_uint,
            );

            let val: &'ll Value = self.const_i32(span.ctxt().outer_expn().as_u32() as i32);

            llvm::LLVMSetMetadata(r, kind, llvm::LLVMMDNodeInContext(self.llcx, &val, 1));
        }

        true
    }
}

impl AsmMethods for CodegenCx<'ll, 'tcx> {
    fn codegen_global_asm(&self, ga: &hir::GlobalAsm) {
        let asm = ga.asm.as_str();
        unsafe {
            llvm::LLVMRustAppendModuleInlineAsm(self.llmod, asm.as_ptr().cast(), asm.len());
        }
    }
}

fn inline_asm_call(
    bx: &mut Builder<'a, 'll, 'tcx>,
    asm: &str,
    cons: &str,
    inputs: &[&'ll Value],
    output: &'ll llvm::Type,
    volatile: bool,
    alignstack: bool,
    dia: ::rustc_ast::ast::LlvmAsmDialect,
) -> Option<&'ll Value> {
    let volatile = if volatile { llvm::True } else { llvm::False };
    let alignstack = if alignstack { llvm::True } else { llvm::False };

    let argtys = inputs
        .iter()
        .map(|v| {
            debug!("Asm Input Type: {:?}", *v);
            bx.cx.val_ty(*v)
        })
        .collect::<Vec<_>>();

    debug!("Asm Output Type: {:?}", output);
    let fty = bx.cx.type_func(&argtys[..], output);
    unsafe {
        // Ask LLVM to verify that the constraints are well-formed.
        let constraints_ok = llvm::LLVMRustInlineAsmVerify(fty, cons.as_ptr().cast(), cons.len());
        debug!("constraint verification result: {:?}", constraints_ok);
        if constraints_ok {
            let v = llvm::LLVMRustInlineAsm(
                fty,
                asm.as_ptr().cast(),
                asm.len(),
                cons.as_ptr().cast(),
                cons.len(),
                volatile,
                alignstack,
                llvm::AsmDialect::from_generic(dia),
            );
            Some(bx.call(v, inputs, None))
        } else {
            // LLVM has detected an issue with our constraints, bail out
            None
        }
    }
}
