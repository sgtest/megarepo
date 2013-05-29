# Copyright 2012 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

######################################################################
# rustc LLVM-extensions (C++) library variables and rules
######################################################################

define DEF_RUSTLLVM_TARGETS

# FIXME: Lately, on windows, llvm-config --includedir is not enough
# to find the llvm includes (probably because we're not actually installing
# llvm, but using it straight out of the build directory)
ifdef CFG_WINDOWSY_$(1)
LLVM_EXTRA_INCDIRS_$(1)= -iquote $(S)src/llvm/include \
                         -iquote llvm/$(1)/include
endif

RUSTLLVM_OBJS_CS_$(1) := $$(addprefix rustllvm/, RustWrapper.cpp PassWrapper.cpp)

RUSTLLVM_DEF_$(1) := rustllvm/rustllvm$(CFG_DEF_SUFFIX_$(1))

RUSTLLVM_INCS_$(1) = $$(LLVM_EXTRA_INCDIRS_$(1)) \
                     -iquote $$(LLVM_INCDIR_$(1)) \
                     -iquote $$(S)src/rustllvm/include
RUSTLLVM_OBJS_OBJS_$(1) := $$(RUSTLLVM_OBJS_CS_$(1):rustllvm/%.cpp=rustllvm/$(1)/%.o)
ALL_OBJ_FILES += $$(RUSTLLVM_OBJS_OBJS_$(1))

rustllvm/$(1)/$(CFG_RUSTLLVM_$(1)): $$(RUSTLLVM_OBJS_OBJS_$(1)) \
                          $$(MKFILE_DEPS) $$(RUSTLLVM_DEF_$(1))
	@$$(call E, link: $$@)
	$$(Q)$$(call CFG_LINK_CXX_$(1),$$@,$$(RUSTLLVM_OBJS_OBJS_$(1)) \
	  $$(CFG_GCCISH_PRE_LIB_FLAGS_$(1)) $$(LLVM_LIBS_$(1)) \
          $$(CFG_GCCISH_POST_LIB_FLAGS_$(1)) \
          $$(LLVM_LDFLAGS_$(1)),$$(RUSTLLVM_DEF_$(1)),$$(CFG_RUSTLLVM_$(1)))

rustllvm/$(1)/%.o: rustllvm/%.cpp $$(MKFILE_DEPS) $$(LLVM_CONFIG_$(1))
	@$$(call E, compile: $$@)
	$$(Q)$$(call CFG_COMPILE_CXX_$(1), $$@, $$(LLVM_CXXFLAGS_$(1)) $$(RUSTLLVM_INCS_$(1))) $$<
endef

# Instantiate template for all stages
$(foreach host,$(CFG_HOST_TRIPLES), \
 $(eval $(call DEF_RUSTLLVM_TARGETS,$(host))))
