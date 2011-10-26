ifdef VERBOSE
 INSTALL = cp $(1)/$(3) $(2)/$(3)
else
 INSTALL = @$(call E, install: $(2)/$(3)) && cp $(1)/$(3) $(2)/$(3)
endif

# The stage we install from
ISTAGE = 3

PREFIX_ROOT = $(CFG_PREFIX)
PREFIX_BIN = $(PREFIX_ROOT)/bin
PREFIX_LIB = $(PREFIX_ROOT)/lib

HB = $(HOST_BIN$(ISTAGE))
HL = $(HOST_LIB$(ISTAGE))
PHB = $(PREFIX_BIN)
PHL = $(PREFIX_LIB)

define INSTALL_TARGET_N

PREFIX_TARGET_ROOT$(1) = $$(PREFIX_LIB)/rustc/$(1)
PREFIX_TARGET_BIN$(1) = $$(PREFIX_TARGET_ROOT$(1))/bin
PREFIX_TARGET_LIB$(1) = $$(PREFIX_TARGET_ROOT$(1))/lib

TB$(1) = $$(TARGET_BIN$$(ISTAGE)$(1))
TL$(1) = $$(TARGET_LIB$$(ISTAGE)$(1))
PTB$(1) = $$(PREFIX_TARGET_BIN$(1))
PTL$(1) = $$(PREFIX_TARGET_LIB$(1))

install-target$(1): $$(SREQ$$(ISTAGE)$(1))
	$(Q)mkdir -p $$(PREFIX_TARGET_LIB$(1))
	$(Q)$(call INSTALL,$$(TL$(1)),$$(PTL$(1)),$$(CFG_RUNTIME))
	$(Q)$(call INSTALL,$$(TL$(1)),$$(PTL$(1)),$$(CFG_STDLIB))
	$(Q)$(call INSTALL,$$(TL$(1)),$$(PTL$(1)),intrinsics.bc)
endef

$(foreach target,$(CFG_TARGET_TRIPLES), \
 $(eval $(call INSTALL_TARGET_N,$(target))))

INSTALL_TARGET_RULES = $(foreach target,$(CFG_TARGET_TRIPLES), \
 install-target$(target))

install: install-host install-targets

install-host: $(SREQ$(ISTAGE)$(CFG_HOST_TRIPLE))
	$(Q)mkdir -p $(PREFIX_BIN)
	$(Q)mkdir -p $(PREFIX_LIB)
	$(Q)$(call INSTALL,$(HB),$(PHB),rustc$(X))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_RUNTIME))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_STDLIB))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_RUSTLLVM))
	$(Q)$(call INSTALL,$(S)/man, \
	     $(PREFIX_ROOT)/share/man/man1,rustc.1)

install-targets: $(INSTALL_TARGET_RULES)
