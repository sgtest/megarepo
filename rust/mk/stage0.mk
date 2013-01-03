# Extract the snapshot host compiler



$(HBIN0_H_$(CFG_HOST_TRIPLE))/rustc$(X):		\
		$(S)src/snapshots.txt					\
		$(S)src/etc/get-snapshot.py $(MKFILE_DEPS)
	@$(call E, fetch: $@)
#   Note: the variable "SNAPSHOT_FILE" is generally not set, and so
#   we generally only pass one argument to this script.  
ifdef CFG_ENABLE_LOCAL_RUST
	$(Q)$(S)src/etc/local_stage0.sh $(CFG_HOST_TRIPLE) $(CFG_LOCAL_RUST_ROOT)
else 
	$(Q)$(CFG_PYTHON) $(S)src/etc/get-snapshot.py $(CFG_HOST_TRIPLE) $(SNAPSHOT_FILE)
ifdef CFG_ENABLE_PAX_FLAGS
	@$(call E, apply PaX flags: $@)
	@"$(CFG_PAXCTL)" -cm "$@"
endif
endif 
	$(Q)touch $@

# Host libs will be extracted by the above rule

$(HLIB0_H_$(CFG_HOST_TRIPLE))/$(CFG_RUNTIME): \
		$(HBIN0_H_$(CFG_HOST_TRIPLE))/rustc$(X)
	$(Q)touch $@

$(HLIB0_H_$(CFG_HOST_TRIPLE))/$(CFG_CORELIB): \
		$(HBIN0_H_$(CFG_HOST_TRIPLE))/rustc$(X)
	$(Q)touch $@

$(HLIB0_H_$(CFG_HOST_TRIPLE))/$(CFG_STDLIB): \
		$(HBIN0_H_$(CFG_HOST_TRIPLE))/rustc$(X)
	$(Q)touch $@

$(HLIB0_H_$(CFG_HOST_TRIPLE))/$(CFG_LIBRUSTC): \
		$(HBIN0_H_$(CFG_HOST_TRIPLE))/rustc$(X)
	$(Q)touch $@

$(HLIB0_H_$(CFG_HOST_TRIPLE))/$(CFG_RUSTLLVM): \
		$(HBIN0_H_$(CFG_HOST_TRIPLE))/rustc$(X)
	$(Q)touch $@

# For other targets, let the host build the target:

define BOOTSTRAP_STAGE0
  # $(1) target to bootstrap
  # $(2) stage to bootstrap from
  # $(3) target to bootstrap from

$$(HBIN0_H_$(1))/rustc$$(X):								\
		$$(TBIN$(2)_T_$(1)_H_$(3))/rustc$$(X)
	@$$(call E, cp: $$@)
	$$(Q)cp $$< $$@

$$(HLIB0_H_$(1))/$$(CFG_RUNTIME): \
		$$(TLIB$(2)_T_$(1)_H_$(3))/$$(CFG_RUNTIME)
	@$$(call E, cp: $$@)
	$$(Q)cp $$< $$@

$$(HLIB0_H_$(1))/$(CFG_CORELIB): \
		$$(TLIB$(2)_T_$(1)_H_$(3))/$$(CFG_CORELIB)
	@$$(call E, cp: $$@)
	$$(Q)cp $$(TLIB$(2)_T_$(1)_H_$(3))/$$(CORELIB_GLOB) $$@

$$(HLIB0_H_$(1))/$(CFG_STDLIB): \
		$$(TLIB$(2)_T_$(1)_H_$(3))/$$(CFG_STDLIB)
	@$$(call E, cp: $$@)
	$$(Q)cp $$(TLIB$(2)_T_$(1)_H_$(3))/$$(STDLIB_GLOB) $$@

$$(HLIB0_H_$(1))/$(CFG_LIBRUSTC): \
		$$(TLIB$(2)_T_$(1)_H_$(3))/$$(CFG_LIBRUSTC)
	@$$(call E, cp: $$@)
	$$(Q)cp $$(TLIB$(2)_T_$(1)_H_$(3))/$$(LIBRUSTC_GLOB) $$@

$$(HLIB0_H_$(1))/$(CFG_RUSTLLVM): \
		$$(TLIB$(2)_T_$(1)_H_$(3))/$$(CFG_RUSTLLVM)
	@$$(call E, cp: $$@)
	$$(Q)cp $$< $$@

endef

# Use stage1 to build other architectures: then you don't have to wait
# for stage2, but you get the latest updates to the compiler source.
$(foreach t,$(NON_HOST_TRIPLES),								\
 $(eval $(call BOOTSTRAP_STAGE0,$(t),1,$(CFG_HOST_TRIPLE))))
