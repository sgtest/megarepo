.PHONY: $(CFG_LLVM_INST_DIR)/bin/llc

$(CFG_LLVM_INST_DIR)/bin/llc:
	@$(call E, make: llvm)
	$(Q)make install -C $(CFG_LLVM_BUILD_DIR)