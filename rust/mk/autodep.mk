######################################################################
# Auto-dependency
######################################################################

C_DEPFILES := $(RUNTIME_CS:%.cpp=%.d) $(RUSTLLVM_LIB_CS:%.cpp=%.d) \
              $(RUSTLLVM_OBJS_CS:%.cpp=%.d)

rt/%.d: rt/%.cpp $(MKFILES)
	@$(call E, dep: $@)
	$(Q)$(call CFG_DEPEND_C, $@ \
      $(subst $(S)src/,,$(patsubst %.cpp, %.o, $<)), \
      $(RUNTIME_INCS)) $< >$@.tmp
	$(Q)$(CFG_PATH_MUNGE) $@.tmp
	$(Q)rm -f $@.tmp.bak
	$(Q)mv $@.tmp $@

rustllvm/%.d: rustllvm/%.cpp $(MKFILES)
	@$(call E, dep: $@)
	$(Q)$(call CFG_DEPEND_C, $@ \
      $(subst $(S)src/,,$(patsubst %.cpp, %.o, $<)), \
      $(CFG_LLVM_CXXFLAGS) $(RUSTLLVM_INCS)) $< >$@.tmp
	$(Q)$(CFG_PATH_MUNGE) $@.tmp
	$(Q)rm -f $@.tmp.bak
	$(Q)mv $@.tmp $@

ifneq ($(MAKECMDGOALS),clean)
-include $(C_DEPFILES)
endif

depend: $(C_DEPFILES)
