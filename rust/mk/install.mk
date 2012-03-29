# FIXME: Docs are currently not installed from the stageN dirs.
# For consistency it might be desirable for stageN to be an exact
# mirror of the installation directory structure.

# Installation macro. Call with source directory as arg 1,
# destination directory as arg 2, and filename/libname-glob as arg 3
ifdef VERBOSE
 INSTALL = install -m755 $(1)/$(3) $(2)/$(3)
 INSTALL_LIB = install -m644 `ls -rt1 $(1)/$(3) | tail -1` $(2)/
else
 INSTALL = $(Q)$(call E, install: $(2)/$(3)) && install -m755 $(1)/$(3) $(2)/$(3)
 INSTALL_LIB = $(Q)$(call E, install_lib: $(2)/$(3)) &&                    \
	       install -m644 `ls -rt1 $(1)/$(3) | tail -1` $(2)/
endif

# The stage we install from
ISTAGE = 2

PREFIX_ROOT = $(CFG_PREFIX)
PREFIX_BIN = $(PREFIX_ROOT)/bin
PREFIX_LIB = $(PREFIX_ROOT)/$(CFG_LIBDIR)

define INSTALL_TARGET_N
  # $(1) is the target triple
  # $(2) is the host triple

# T{B,L} == Target {Bin, Lib} for stage ${ISTAGE}
TB$(1)$(2) = $$(TBIN$$(ISTAGE)_T_$(1)_H_$(2))
TL$(1)$(2) = $$(TLIB$$(ISTAGE)_T_$(1)_H_$(2))

# PT{R,B,L} == Prefix Target {Root, Bin, Lib}
PTR$(1)$(2) = $$(PREFIX_LIB)/rustc/$(1)
PTB$(1)$(2) = $$(PTR$(1)$(2))/bin
PTL$(1)$(2) = $$(PTR$(1)$(2))/$(CFG_LIBDIR)

install-target-$(1)-host-$(2): $$(SREQ$$(ISTAGE)_T_$(1)_H_$(2))
	$$(Q)mkdir -p $$(PTL$(1)$(2))
	$$(Q)$$(call INSTALL,$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(CFG_RUNTIME))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(CORELIB_GLOB))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(STDLIB_GLOB))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUSTC_GLOB))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUSTSYNTAX_GLOB))
	$$(Q)$$(call INSTALL,$$(TL$(1)$(2)),$$(PTL$(1)$(2)),libmorestack.a)

endef

$(foreach target,$(CFG_TARGET_TRIPLES), \
 $(eval $(call INSTALL_TARGET_N,$(target),$(CFG_HOST_TRIPLE))))

INSTALL_TARGET_RULES = $(foreach target,$(CFG_TARGET_TRIPLES), \
 install-target-$(target)-host-$(CFG_HOST_TRIPLE))

install: all install-host install-targets

# Shorthand for build/stageN/bin
HB = $(HBIN$(ISTAGE)_H_$(CFG_HOST_TRIPLE))
HB3 = $(HBIN3_H_$(CFG_HOST_TRIPLE))
# Shorthand for build/stageN/lib
HL = $(HLIB$(ISTAGE)_H_$(CFG_HOST_TRIPLE))
# Shorthand for the prefix bin directory
PHB = $(PREFIX_BIN)
# Shorthand for the prefix bin directory
PHL = $(PREFIX_LIB)

install-host: $(SREQ$(ISTAGE)_T_$(CFG_HOST_TRIPLE)_H_$(CFG_HOST_TRIPLE))
	$(Q)mkdir -p $(PREFIX_BIN)
	$(Q)mkdir -p $(PREFIX_LIB)
	$(Q)mkdir -p $(PREFIX_ROOT)/share/man/man1
	$(Q)$(call INSTALL,$(HB3),$(PHB),rustc$(X))
	$(Q)$(call INSTALL,$(HB3),$(PHB),cargo$(X))
	$(Q)$(call INSTALL,$(HB3),$(PHB),rustdoc$(X))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_RUNTIME))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(CORELIB_GLOB))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(STDLIB_GLOB))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(LIBRUSTC_GLOB))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(LIBRUSTSYNTAX_GLOB))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_RUSTLLVM))
	$(Q)$(call INSTALL,$(S)/man, \
	     $(PREFIX_ROOT)/share/man/man1,rustc.1)

install-targets: $(INSTALL_TARGET_RULES)


HOST_LIB_FROM_HL_GLOB = \
  $(patsubst $(HL)/%,$(PHL)/%,$(wildcard $(HL)/$(1)))

uninstall:
	$(Q)rm -f $(PHB)/rustc$(X)
	$(Q)rm -f $(PHB)/cargo$(X)
	$(Q)rm -f $(PHB)/rustdoc$(X)
	$(Q)rm -f $(PHL)/$(CFG_RUSTLLVM)
	$(Q)rm -f $(PHL)/$(CFG_RUNTIME)
	$(Q)for i in \
          $(call HOST_LIB_FROM_HL_GLOB,$(CORELIB_GLOB)) \
          $(call HOST_LIB_FROM_HL_GLOB,$(STDLIB_GLOB)) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUSTC_GLOB)) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUSTSYNTAX_GLOB)) \
        ; \
        do rm -f $$i ; \
        done
	$(Q)rm -Rf $(PHL)/rustc
	$(Q)rm -f $(PREFIX_ROOT)/share/man/man1/rustc.1
