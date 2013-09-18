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
LLVM_DEPS_SRC=$(call rwildcard,$(CFG_LLVM_SRC_DIR)/lib,*cpp *hpp)
LLVM_DEPS_INC=$(call rwildcard,$(CFG_LLVM_SRC_DIR)/include,*cpp *hpp)
LLVM_DEPS=$(LLVM_DEPS_SRC) $(LLVM_DEPS_INC)
endif

define DEF_LLVM_RULES

# If CFG_LLVM_ROOT is defined then we don't build LLVM ourselves
ifeq ($(CFG_LLVM_ROOT),)

LLVM_STAMP_$(1) = $$(CFG_LLVM_BUILD_DIR_$(1))/llvm-auto-clean-stamp

$$(LLVM_CONFIG_$(1)): $$(LLVM_DEPS) $$(LLVM_STAMP_$(1))
	@$$(call E, make: llvm)
	$$(Q)$$(MAKE) -C $$(CFG_LLVM_BUILD_DIR_$(1)) $$(CFG_LLVM_BUILD_ENV_$(1))
	$$(Q)touch $$(LLVM_CONFIG_$(1))
endif

# This is used to independently force an LLVM clean rebuild
# when we changed something not otherwise captured by builtin
# dependencies. In these cases, commit a change that touches
# the stamp in the source dir.
$$(LLVM_STAMP_$(1)): $(S)src/rustllvm/llvm-auto-clean-trigger
	$(Q)$(MAKE) clean-llvm
	touch $$@

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(eval $(call DEF_LLVM_RULES,$(host))))
