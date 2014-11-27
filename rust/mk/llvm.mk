# Copyright 2012 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.


ifdef CFG_ENABLE_FAST_MAKE
LLVM_DEPS := $(S)/.gitmodules
else

# This is just a rough approximation of LLVM deps
LLVM_DEPS_SRC=$(call rwildcard,$(CFG_LLVM_SRC_DIR)lib,*cpp *hpp)
LLVM_DEPS_INC=$(call rwildcard,$(CFG_LLVM_SRC_DIR)include,*cpp *hpp)
LLVM_DEPS=$(LLVM_DEPS_SRC) $(LLVM_DEPS_INC)
endif

define DEF_LLVM_RULES

# If CFG_LLVM_ROOT is defined then we don't build LLVM ourselves
ifeq ($(CFG_LLVM_ROOT),)

LLVM_STAMP_$(1) = $$(CFG_LLVM_BUILD_DIR_$(1))/llvm-auto-clean-stamp

$$(LLVM_CONFIG_$(1)): $$(LLVM_DEPS) $$(LLVM_STAMP_$(1))
	@$$(call E, make: llvm)
	$$(Q)$$(MAKE) -C $$(CFG_LLVM_BUILD_DIR_$(1)) $$(CFG_LLVM_BUILD_ENV_$(1)) ONLY_TOOLS="$$(LLVM_TOOLS)"
	$$(Q)touch $$(LLVM_CONFIG_$(1))
endif

# This is used to independently force an LLVM clean rebuild
# when we changed something not otherwise captured by builtin
# dependencies. In these cases, commit a change that touches
# the stamp in the source dir.
$$(LLVM_STAMP_$(1)): $(S)src/rustllvm/llvm-auto-clean-trigger
	@$$(call E, make: cleaning llvm)
	$(Q)$(MAKE) clean-llvm$(1)
	@$$(call E, make: done cleaning llvm)
	touch $$@

ifeq ($$(CFG_ENABLE_LLVM_STATIC_STDCPP),1)
LLVM_STDCPP_LOCATION_$(1) = $$(shell $$(CC_$(1)) $$(CFG_GCCISH_CFLAGS_$(1)) \
					-print-file-name=libstdc++.a)
else
LLVM_STDCPP_LOCATION_$(1) =
endif


# LLVM linkage:
LLVM_LINKAGE_PATH_$(1):=$$(abspath $$(RT_OUTPUT_DIR_$(1))/llvmdeps.rs)
$$(LLVM_LINKAGE_PATH_$(1)): $(S)src/etc/mklldeps.py $$(LLVM_CONFIG_$(1))
	$(Q)$(CFG_PYTHON) "$$<" "$$@" "$$(LLVM_COMPONENTS)" "$$(CFG_ENABLE_LLVM_STATIC_STDCPP)" \
		$$(LLVM_CONFIG_$(1))
endef

$(foreach host,$(CFG_HOST), \
 $(eval $(call DEF_LLVM_RULES,$(host))))

$(foreach host,$(CFG_HOST), \
 $(eval LLVM_CONFIGS := $(LLVM_CONFIGS) $(LLVM_CONFIG_$(host))))

# This can't be done in target.mk because it's included before this file.
define LLVM_LINKAGE_DEPS
$$(TLIB$(1)_T_$(2)_H_$(3))/stamp.rustc_llvm: $$(LLVM_LINKAGE_PATH_$(3))
endef

$(foreach source,$(CFG_HOST), \
 $(foreach target,$(CFG_TARGET), \
  $(eval $(call LLVM_LINKAGE_DEPS,0,$(target),$(source))) \
  $(eval $(call LLVM_LINKAGE_DEPS,1,$(target),$(source))) \
  $(eval $(call LLVM_LINKAGE_DEPS,2,$(target),$(source))) \
  $(eval $(call LLVM_LINKAGE_DEPS,3,$(target),$(source)))))
