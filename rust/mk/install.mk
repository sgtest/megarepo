# Copyright 2012 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

# FIXME: Docs are currently not installed from the stageN dirs.
# For consistency it might be desirable for stageN to be an exact
# mirror of the installation directory structure.

# Installation macro. Call with source directory as arg 1,
# destination directory as arg 2, and filename/libname-glob as arg 3
ifdef VERBOSE
 INSTALL = install -m755 $(1)/$(3) $(2)/$(3)
 INSTALL_LIB = install -m644 `ls -drt1 $(1)/$(3) | tail -1` $(2)/
else
 INSTALL = $(Q)$(call E, install: $(2)/$(3)) && install -m755 $(1)/$(3) $(2)/$(3)
 INSTALL_LIB = $(Q)$(call E, install_lib: $(2)/$(3)) &&                    \
	       install -m644 `ls -drt1 $(1)/$(3) | tail -1` $(2)/
endif

# The stage we install from
ISTAGE = 2

PREFIX_ROOT = $(CFG_PREFIX)
PREFIX_BIN = $(PREFIX_ROOT)/bin
PREFIX_LIB = $(PREFIX_ROOT)/$(CFG_LIBDIR)

define INSTALL_PREPARE_N
  # $(1) is the target triple
  # $(2) is the host triple

# T{B,L} == Target {Bin, Lib} for stage ${ISTAGE}
TB$(1)$(2) = $$(TBIN$$(ISTAGE)_T_$(1)_H_$(2))
TL$(1)$(2) = $$(TLIB$$(ISTAGE)_T_$(1)_H_$(2))

# PT{R,B,L} == Prefix Target {Root, Bin, Lib}
PTR$(1)$(2) = $$(PREFIX_LIB)/rustc/$(1)
PTB$(1)$(2) = $$(PTR$(1)$(2))/bin
PTL$(1)$(2) = $$(PTR$(1)$(2))/$(CFG_LIBDIR)

endef

$(foreach target,$(CFG_TARGET_TRIPLES), \
 $(eval $(call INSTALL_PREPARE_N,$(target),$(CFG_BUILD_TRIPLE))))

define INSTALL_TARGET_N
install-target-$(1)-host-$(2): $$(TSREQ$$(ISTAGE)_T_$(1)_H_$(2)) $$(SREQ$$(ISTAGE)_T_$(1)_H_$(2))
	$$(Q)mkdir -p $$(PTL$(1)$(2))
	$$(Q)$$(call INSTALL,$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(CFG_RUNTIME_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(CORELIB_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(STDLIB_GLOB_$(1)))
	$$(Q)$$(call INSTALL,$$(TL$(1)$(2)),$$(PTL$(1)$(2)),libmorestack.a)

endef

define INSTALL_HOST_N
install-target-$(1)-host-$(2): $$(CSREQ$$(ISTAGE)_T_$(1)_H_$(2))
	$$(Q)mkdir -p $$(PTL$(1)$(2))
	$$(Q)$$(call INSTALL,$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(CFG_RUNTIME_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(CORELIB_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(STDLIB_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUSTC_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBSYNTAX_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUSTPKG_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUSTDOC_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUSTI_GLOB_$(1)))
	$$(Q)$$(call INSTALL_LIB, \
		$$(TL$(1)$(2)),$$(PTL$(1)$(2)),$$(LIBRUST_GLOB_$(1)))
	$$(Q)$$(call INSTALL,$$(TL$(1)$(2)),$$(PTL$(1)$(2)),libmorestack.a)

endef

$(foreach target,$(CFG_TARGET_TRIPLES), \
 $(if $(findstring $(target), $(CFG_BUILD_TRIPLE)), \
  $(eval $(call INSTALL_HOST_N,$(target),$(CFG_BUILD_TRIPLE))), \
  $(eval $(call INSTALL_TARGET_N,$(target),$(CFG_BUILD_TRIPLE)))))

INSTALL_TARGET_RULES = $(foreach target,$(CFG_TARGET_TRIPLES), \
 install-target-$(target)-host-$(CFG_BUILD_TRIPLE))

install: all install-host install-targets

# Shorthand for build/stageN/bin
HB = $(HBIN$(ISTAGE)_H_$(CFG_BUILD_TRIPLE))
HB2 = $(HBIN2_H_$(CFG_BUILD_TRIPLE))
# Shorthand for build/stageN/lib
HL = $(HLIB$(ISTAGE)_H_$(CFG_BUILD_TRIPLE))
# Shorthand for the prefix bin directory
PHB = $(PREFIX_BIN)
# Shorthand for the prefix bin directory
PHL = $(PREFIX_LIB)

install-host: $(CSREQ$(ISTAGE)_T_$(CFG_BUILD_TRIPLE)_H_$(CFG_BUILD_TRIPLE))
	$(Q)mkdir -p $(PREFIX_BIN)
	$(Q)mkdir -p $(PREFIX_LIB)
	$(Q)mkdir -p $(PREFIX_ROOT)/share/man/man1
	$(Q)$(call INSTALL,$(HB2),$(PHB),rustc$(X_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(HB2),$(PHB),rustpkg$(X_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(HB2),$(PHB),rustdoc$(X_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(HB2),$(PHB),rusti$(X_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(HB2),$(PHB),rust$(X_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(CORELIB_GLOB_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(STDLIB_GLOB_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(LIBRUSTC_GLOB_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(LIBSYNTAX_GLOB_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(LIBRUSTI_GLOB_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL_LIB,$(HL),$(PHL),$(LIBRUST_GLOB_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_RUNTIME_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(HL),$(PHL),$(CFG_RUSTLLVM_$(CFG_BUILD_TRIPLE)))
	$(Q)$(call INSTALL,$(S)/man, \
	     $(PREFIX_ROOT)/share/man/man1,rustc.1)

install-targets: $(INSTALL_TARGET_RULES)


HOST_LIB_FROM_HL_GLOB = \
  $(patsubst $(HL)/%,$(PHL)/%,$(wildcard $(HL)/$(1)))

uninstall:
	$(Q)rm -f $(PHB)/rustc$(X_$(CFG_BUILD_TRIPLE))
	$(Q)rm -f $(PHB)/rustpkg$(X_$(CFG_BUILD_TRIPLE))
	$(Q)rm -f $(PHB)/rusti$(X_$(CFG_BUILD_TRIPLE))
	$(Q)rm -f $(PHB)/rust$(X_$(CFG_BUILD_TRIPLE))
	$(Q)rm -f $(PHB)/rustdoc$(X_$(CFG_BUILD_TRIPLE))
	$(Q)rm -f $(PHL)/$(CFG_RUSTLLVM_$(CFG_BUILD_TRIPLE))
	$(Q)rm -f $(PHL)/$(CFG_RUNTIME_$(CFG_BUILD_TRIPLE))
	$(Q)for i in \
          $(call HOST_LIB_FROM_HL_GLOB,$(CORELIB_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(STDLIB_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUSTC_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBSYNTAX_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUSTPKG_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUSTDOC_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUSTI_GLOB_$(CFG_BUILD_TRIPLE))) \
          $(call HOST_LIB_FROM_HL_GLOB,$(LIBRUST_GLOB_$(CFG_BUILD_TRIPLE))) \
        ; \
        do rm -f $$i ; \
        done
	$(Q)rm -Rf $(PHL)/rustc
	$(Q)rm -f $(PREFIX_ROOT)/share/man/man1/rustc.1
