######################################################################
# Cleanup
######################################################################

.PHONY: clean

tidy:
	@$(call E, check: formatting)
	$(Q)echo \
      $(filter-out $(GENERATED) $(addprefix $(S)src/, $(GENERATED)) \
        $(addprefix $(S)src/, $(RUSTLLVM_LIB_CS) $(RUSTLLVM_OBJS_CS) \
          $(RUSTLLVM_HDR) $(PKG_3RDPARTY)) \
        $(S)src/etc/%, $(PKG_FILES)) \
    | xargs -n 10 python $(S)src/etc/tidy.py

clean:
	@$(call E, cleaning)
	$(Q)rm -f $(RUNTIME_OBJS) $(RUNTIME_DEF)
	$(Q)rm -f $(RUSTLLVM_LIB_OBJS) $(RUSTLLVM_OBJS_OBJS) $(RUSTLLVM_DEF)
	$(Q)rm -f $(BOOT_CMOS) $(BOOT_CMIS) $(BOOT_CMXS) $(BOOT_OBJS)
	$(Q)rm -f $(ML_DEPFILES) $(C_DEPFILES) $(CRATE_DEPFILES)
	$(Q)rm -f $(ML_DEPFILES:%.d=%.d.tmp)
	$(Q)rm -f $(C_DEPFILES:%.d=%.d.tmp)
	$(Q)rm -f $(CRATE_DEPFILES:%.d=%.d.tmp)
	$(Q)rm -f $(GENERATED)
	$(Q)rm -f stage0/rustc$(X) stage0/$(CFG_STDLIB)
	$(Q)rm -f stage1/rustc$(X) stage1/$(CFG_STDLIB) stage1/glue*
	$(Q)rm -f stage2/rustc$(X) stage2/$(CFG_STDLIB) stage2/glue*
	$(Q)rm -f stage3/rustc$(X) stage3/$(CFG_STDLIB) stage3/glue*
	$(Q)rm -f rustllvm/$(CFG_RUSTLLVM) rustllvm/rustllvmbits.a
	$(Q)rm -f rt/$(CFG_RUNTIME)
	$(Q)rm -Rf $(PKG_NAME)-*.tar.gz dist
	$(Q)rm -f $(foreach ext,o a d bc s exe,$(wildcard stage*/*.$(ext)))
	$(Q)rm -Rf $(foreach ext,out out.tmp                      \
                             stage0$(X) stage1$(X) stage2$(X) \
                             bc o s exe dSYM,                 \
                        $(wildcard test/*/*.$(ext) test/bench/*/*.$(ext)))
	$(Q)rm -Rf $(foreach ext, \
                 aux cp fn ky log pdf html pg toc tp vr cps, \
                 $(wildcard doc/*.$(ext)))
	$(Q)rm -Rf doc/version.texi
