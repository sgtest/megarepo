# Copyright 2014 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

################################################################################
# Native libraries built as part of the rust build process
#
# This portion of the rust build system is meant to keep track of native
# dependencies and how to build them. It is currently required that all native
# dependencies are built as static libraries, as slinging around dynamic
# libraries isn't exactly the most fun thing to do.
#
# This section should need minimal modification to add new libraries. The
# relevant variables are:
#
#   NATIVE_LIBS
#	This is a list of all native libraries which are built as part of the
#	build process. It will build all libraries into RT_OUTPUT_DIR with the
#	appropriate name of static library as dictated by the target platform
#
#   NATIVE_DEPS_<lib>
#	This is a list of files relative to the src/rt directory which are
#	needed to build the native library. Each file will be compiled to an
#	object file, and then all the object files will be assembled into an
#	archive (static library). The list contains files of any extension
#
# If adding a new library, you should update the NATIVE_LIBS list, and then list
# the required files below it. The list of required files is a list of files
# that's per-target so you're allowed to conditionally add files based on the
# target.
################################################################################
NATIVE_LIBS := hoedown miniz rust_test_helpers

# $(1) is the target triple
define NATIVE_LIBRARIES

NATIVE_DEPS_hoedown_$(1) := hoedown/src/autolink.c \
			hoedown/src/buffer.c \
			hoedown/src/document.c \
			hoedown/src/escape.c \
			hoedown/src/html.c \
			hoedown/src/html_blocks.c \
			hoedown/src/html_smartypants.c \
			hoedown/src/stack.c \
			hoedown/src/version.c
NATIVE_DEPS_miniz_$(1) = miniz.c
NATIVE_DEPS_rust_test_helpers_$(1) := rust_test_helpers.c

################################################################################
# You shouldn't find it that necessary to edit anything below this line.
################################################################################

# While we're defining the native libraries for each target, we define some
# common rules used to build files for various targets.

RT_OUTPUT_DIR_$(1) := $(1)/rt

$$(RT_OUTPUT_DIR_$(1))/%.o: $(S)src/rt/%.c $$(MKFILE_DEPS)
	@mkdir -p $$(@D)
	@$$(call E, compile: $$@)
	$$(Q)$$(call CFG_COMPILE_C_$(1), $$@, \
		$$(call CFG_CC_INCLUDE_$(1),$$(S)src/rt/hoedown/src) \
		$$(call CFG_CC_INCLUDE_$(1),$$(S)src/rt) \
                 $$(RUNTIME_CFLAGS_$(1))) $$<

$$(RT_OUTPUT_DIR_$(1))/%.o: $(S)src/rt/%.S $$(MKFILE_DEPS) \
	    $$(LLVM_CONFIG_$$(CFG_BUILD))
	@mkdir -p $$(@D)
	@$$(call E, compile: $$@)
	$$(Q)$$(call CFG_ASSEMBLE_$(1),$$@,$$<)

# On MSVC targets the compiler's default include path (e.g. where to find system
# headers) is specified by the INCLUDE environment variable. This may not be set
# so the ./configure script scraped the relevant values and this is the location
# that we put them into cl.exe's environment.
ifeq ($$(findstring msvc,$(1)),msvc)
$$(RT_OUTPUT_DIR_$(1))/%.o: \
	export INCLUDE := $$(CFG_MSVC_INCLUDE_PATH_$$(HOST_$(1)))
$(1)/rustllvm/%.o: \
	export INCLUDE := $$(CFG_MSVC_INCLUDE_PATH_$$(HOST_$(1)))
endif
endef

$(foreach target,$(CFG_TARGET),$(eval $(call NATIVE_LIBRARIES,$(target))))

# A macro for devining how to build third party libraries listed above (based
# on their dependencies).
#
# $(1) is the target
# $(2) is the lib name
define THIRD_PARTY_LIB

OBJS_$(2)_$(1) := $$(NATIVE_DEPS_$(2)_$(1):%=$$(RT_OUTPUT_DIR_$(1))/%)
OBJS_$(2)_$(1) := $$(OBJS_$(2)_$(1):.c=.o)
OBJS_$(2)_$(1) := $$(OBJS_$(2)_$(1):.cpp=.o)
OBJS_$(2)_$(1) := $$(OBJS_$(2)_$(1):.S=.o)
NATIVE_$(2)_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),$(2))
$$(RT_OUTPUT_DIR_$(1))/$$(NATIVE_$(2)_$(1)): $$(OBJS_$(2)_$(1))
	@$$(call E, link: $$@)
	$$(Q)$$(call CFG_CREATE_ARCHIVE_$(1),$$@) $$^

endef

$(foreach target,$(CFG_TARGET), \
 $(eval $(call RUNTIME_RULES,$(target))))
$(foreach lib,$(NATIVE_LIBS), \
 $(foreach target,$(CFG_TARGET), \
  $(eval $(call THIRD_PARTY_LIB,$(target),$(lib)))))


################################################################################
# Building third-party targets with external build systems
#
# This location is meant for dependencies which have external build systems. It
# is still assumed that the output of each of these steps is a static library
# in the correct location.
################################################################################

define DEF_THIRD_PARTY_TARGETS

# $(1) is the target triple

ifeq ($$(CFG_WINDOWSY_$(1)),1)
  # A bit of history here, this used to be --enable-lazy-lock added in #14006
  # which was filed with jemalloc in jemalloc/jemalloc#83 which was also
  # reported to MinGW: http://sourceforge.net/p/mingw-w64/bugs/395/
  #
  # When updating jemalloc to 4.0, however, it was found that binaries would
  # exit with the status code STATUS_RESOURCE_NOT_OWNED indicating that a thread
  # was unlocking a mutex it never locked. Disabling this "lazy lock" option
  # seems to fix the issue, but it was enabled by default for MinGW targets in
  # 13473c7 for jemalloc.
  #
  # As a result of all that, force disabling lazy lock on Windows, and after
  # reading some code it at least *appears* that the initialization of mutexes
  # is otherwise ok in jemalloc, so shouldn't cause problems hopefully...
  #
  # tl;dr: make windows behave like other platforms by disabling lazy locking,
  #        but requires passing an option due to a historical default with
  #        jemalloc.
  JEMALLOC_ARGS_$(1) := --disable-lazy-lock
else ifeq ($(OSTYPE_$(1)), apple-ios)
  JEMALLOC_ARGS_$(1) := --disable-tls
else ifeq ($(findstring android, $(OSTYPE_$(1))), android)
  # We force android to have prefixed symbols because apparently replacement of
  # the libc allocator doesn't quite work. When this was tested (unprefixed
  # symbols), it was found that the `realpath` function in libc would allocate
  # with libc malloc (not jemalloc malloc), and then the standard library would
  # free with jemalloc free, causing a segfault.
  #
  # If the test suite passes, however, without symbol prefixes then we should be
  # good to go!
  JEMALLOC_ARGS_$(1) := --disable-tls --with-jemalloc-prefix=je_
else ifeq ($(findstring dragonfly, $(OSTYPE_$(1))), dragonfly)
  JEMALLOC_ARGS_$(1) := --with-jemalloc-prefix=je_
endif

ifdef CFG_ENABLE_DEBUG_JEMALLOC
  JEMALLOC_ARGS_$(1) += --enable-debug --enable-fill
endif

################################################################################
# jemalloc
################################################################################

ifdef CFG_ENABLE_FAST_MAKE
JEMALLOC_DEPS := $(S)/.gitmodules
else
JEMALLOC_DEPS := $(wildcard \
		   $(S)src/jemalloc/* \
		   $(S)src/jemalloc/*/* \
		   $(S)src/jemalloc/*/*/* \
		   $(S)src/jemalloc/*/*/*/*)
endif

# See #17183 for details, this file is touched during the build process so we
# don't want to consider it as a dependency.
JEMALLOC_DEPS := $(filter-out $(S)src/jemalloc/VERSION,$(JEMALLOC_DEPS))

JEMALLOC_NAME_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),jemalloc)
ifeq ($$(CFG_WINDOWSY_$(1)),1)
  JEMALLOC_REAL_NAME_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),jemalloc_s)
else
  JEMALLOC_REAL_NAME_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),jemalloc_pic)
endif
JEMALLOC_LIB_$(1) := $$(RT_OUTPUT_DIR_$(1))/$$(JEMALLOC_NAME_$(1))
JEMALLOC_BUILD_DIR_$(1) := $$(RT_OUTPUT_DIR_$(1))/jemalloc
JEMALLOC_LOCAL_$(1) := $$(JEMALLOC_BUILD_DIR_$(1))/lib/$$(JEMALLOC_REAL_NAME_$(1))

$$(JEMALLOC_LOCAL_$(1)): $$(JEMALLOC_DEPS) $$(MKFILE_DEPS)
	@$$(call E, make: jemalloc)
	cd "$$(JEMALLOC_BUILD_DIR_$(1))"; "$(S)src/jemalloc/configure" \
		$$(JEMALLOC_ARGS_$(1)) $(CFG_JEMALLOC_FLAGS) \
		--build=$$(CFG_GNU_TRIPLE_$(CFG_BUILD)) --host=$$(CFG_GNU_TRIPLE_$(1)) \
		CC="$$(CC_$(1)) $$(CFG_JEMALLOC_CFLAGS_$(1))" \
		AR="$$(AR_$(1))" \
		RANLIB="$$(AR_$(1)) s" \
		CPPFLAGS="-I $(S)src/rt/" \
		EXTRA_CFLAGS="-g1 -ffunction-sections -fdata-sections"
	$$(Q)$$(MAKE) -C "$$(JEMALLOC_BUILD_DIR_$(1))" build_lib_static

ifeq ($(1),$$(CFG_BUILD))
ifneq ($$(CFG_JEMALLOC_ROOT),)
$$(JEMALLOC_LIB_$(1)): $$(CFG_JEMALLOC_ROOT)/libjemalloc_pic.a
	@$$(call E, copy: jemalloc)
	$$(Q)cp $$< $$@
else
$$(JEMALLOC_LIB_$(1)): $$(JEMALLOC_LOCAL_$(1))
	$$(Q)cp $$< $$@
endif
else
$$(JEMALLOC_LIB_$(1)): $$(JEMALLOC_LOCAL_$(1))
	$$(Q)cp $$< $$@
endif

################################################################################
# compiler-rt
################################################################################

ifdef CFG_ENABLE_FAST_MAKE
COMPRT_DEPS := $(S)/.gitmodules
else
COMPRT_DEPS := $(wildcard \
              $(S)src/compiler-rt/* \
              $(S)src/compiler-rt/*/* \
              $(S)src/compiler-rt/*/*/* \
              $(S)src/compiler-rt/*/*/*/*)
endif

# compiler-rt's build system is a godawful mess. Here we figure out
# the ridiculous platform-specific values and paths necessary to get
# useful artifacts out of it.

COMPRT_NAME_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),compiler-rt)
COMPRT_LIB_$(1) := $$(RT_OUTPUT_DIR_$(1))/$$(COMPRT_NAME_$(1))
COMPRT_BUILD_DIR_$(1) := $$(RT_OUTPUT_DIR_$(1))/compiler-rt

COMPRT_ARCH_$(1) := $$(word 1,$$(subst -, ,$(1)))

# All this is to figure out the path to the compiler-rt bin
ifeq ($$(findstring windows-msvc,$(1)),windows-msvc)
COMPRT_DIR_$(1) := windows/Release
COMPRT_LIB_NAME_$(1) := clang_rt.builtins-$$(patsubst i%86,i386,$$(COMPRT_ARCH_$(1)))
endif

ifeq ($$(findstring windows-gnu,$(1)),windows-gnu)
COMPRT_DIR_$(1) := windows
COMPRT_LIB_NAME_$(1) := clang_rt.builtins-$$(COMPRT_ARCH_$(1))
endif

ifeq ($$(findstring darwin,$(1)),darwin)
COMPRT_DIR_$(1) := builtins
COMPRT_LIB_NAME_$(1) := clang_rt.builtins_$$(patsubst i686,i386,$$(COMPRT_ARCH_$(1)))_osx
endif

ifeq ($$(findstring ios,$(1)),ios)
COMPRT_DIR_$(1) := builtins
COMPRT_ARCH_$(1) := $$(patsubst armv7s,armv7em,$$(COMPRT_ARCH_$(1)))
COMPRT_LIB_NAME_$(1) := clang_rt.hard_pic_$$(COMPRT_ARCH_$(1))_macho_embedded
ifeq ($$(COMPRT_ARCH_$(1)),aarch64)
COMPRT_LIB_NAME_$(1) := clang_rt.builtins_arm64_ios
endif
COMPRT_DEFINES_$(1) := -DCOMPILER_RT_ENABLE_IOS=ON
endif

ifndef COMPRT_DIR_$(1)
# NB: FreeBSD and NetBSD output to "linux"...
COMPRT_DIR_$(1) := linux
COMPRT_ARCH_$(1) := $$(patsubst i586,i386,$$(COMPRT_ARCH_$(1)))

ifeq ($$(findstring android,$(1)),android)
ifeq ($$(findstring arm,$$(COMPRT_ARCH_$(1))),arm)
COMPRT_ARCH_$(1) := armhf
endif
endif

ifeq ($$(findstring eabihf,$(1)),eabihf)
ifeq ($$(findstring armv7,$(1)),)
COMPRT_LIB_NAME_$(1) := clang_rt.builtins-armhf
endif
endif

ifndef COMPRT_LIB_NAME_$(1)
COMPRT_LIB_NAME_$(1) := clang_rt.builtins-$$(COMPRT_ARCH_$(1))
endif
endif


ifeq ($$(findstring windows-gnu,$(1)),windows-gnu)
COMPRT_LIB_FILE_$(1) := lib$$(COMPRT_LIB_NAME_$(1)).a
endif

ifeq ($$(findstring android,$(1)),android)
ifeq ($$(findstring arm,$(1)),arm)
COMPRT_LIB_FILE_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),$$(COMPRT_LIB_NAME_$(1))-android)
endif
endif

ifndef COMPRT_LIB_FILE_$(1)
COMPRT_LIB_FILE_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),$$(COMPRT_LIB_NAME_$(1)))
endif

COMPRT_OUTPUT_$(1) := $$(COMPRT_BUILD_DIR_$(1))/lib/$$(COMPRT_DIR_$(1))/$$(COMPRT_LIB_FILE_$(1))

ifeq ($$(findstring windows-msvc,$(1)),windows-msvc)
COMPRT_BUILD_ARGS_$(1) := //v:m //nologo
COMPRT_BUILD_TARGET_$(1) := lib/builtins/builtins
COMPRT_BUILD_CC_$(1) :=
else
COMPRT_BUILD_ARGS_$(1) :=
ifndef COMPRT_BUILD_TARGET_$(1)
COMPRT_BUILD_TARGET_$(1) := $$(COMPRT_LIB_NAME_$(1))
endif
COMPRT_BUILD_CC_$(1) := -DCMAKE_C_COMPILER=$$(call FIND_COMPILER,$$(CC_$(1))) \
			-DCMAKE_CXX_COMPILER=$$(call FIND_COMPILER,$$(CXX_$(1)))

ifeq ($$(findstring ios,$(1)),)
COMPRT_BUILD_CC_$(1) := $$(COMPRT_BUILD_CC_$(1)) \
			-DCMAKE_C_FLAGS="$$(CFG_GCCISH_CFLAGS_$(1)) -Wno-error"
endif

endif

ifeq ($$(findstring emscripten,$(1)),emscripten)

# FIXME: emscripten doesn't use compiler-rt and can't build it without
# further hacks
$$(COMPRT_LIB_$(1)):
	touch $$@

else

$$(COMPRT_LIB_$(1)): $$(COMPRT_DEPS) $$(MKFILE_DEPS) $$(LLVM_CONFIG_$$(CFG_BUILD))
	@$$(call E, cmake: compiler-rt)
	$$(Q)rm -rf $$(COMPRT_BUILD_DIR_$(1))
	$$(Q)mkdir $$(COMPRT_BUILD_DIR_$(1))
	$$(Q)cd "$$(COMPRT_BUILD_DIR_$(1))"; \
		$$(CFG_CMAKE) "$(S)src/compiler-rt" \
		-DCMAKE_BUILD_TYPE=$$(LLVM_BUILD_CONFIG_MODE) \
		-DLLVM_CONFIG_PATH=$$(LLVM_CONFIG_$$(CFG_BUILD)) \
		-DCOMPILER_RT_DEFAULT_TARGET_TRIPLE=$(1) \
		-DCOMPILER_RT_BUILD_SANITIZERS=OFF \
		-DCOMPILER_RT_BUILD_EMUTLS=OFF \
		$$(COMPRT_DEFINES_$(1)) \
		$$(COMPRT_BUILD_CC_$(1)) \
		-G"$$(CFG_CMAKE_GENERATOR)"
	$$(Q)$$(CFG_CMAKE) --build "$$(COMPRT_BUILD_DIR_$(1))" \
		--target $$(COMPRT_BUILD_TARGET_$(1)) \
		--config $$(LLVM_BUILD_CONFIG_MODE) \
		-- $$(COMPRT_BUILD_ARGS_$(1)) $$(MFLAGS)
	$$(Q)cp "$$(COMPRT_OUTPUT_$(1))" $$@

endif

################################################################################
# libbacktrace
#
# We use libbacktrace on linux to get symbols in backtraces, but only on linux.
# Elsewhere we use other system utilities, so this library is only built on
# linux.
################################################################################

BACKTRACE_NAME_$(1) := $$(call CFG_STATIC_LIB_NAME_$(1),backtrace)
BACKTRACE_LIB_$(1) := $$(RT_OUTPUT_DIR_$(1))/$$(BACKTRACE_NAME_$(1))
BACKTRACE_BUILD_DIR_$(1) := $$(RT_OUTPUT_DIR_$(1))/libbacktrace

# We don't use this on platforms that aren't linux-based (with the exception of
# msys2/mingw builds on windows, which use it to read the dwarf debug
# information) so just make the file available, the compilation of libstd won't
# actually build it.
ifeq ($$(findstring darwin,$$(OSTYPE_$(1))),darwin)
# See comment above
$$(BACKTRACE_LIB_$(1)):
	touch $$@

else ifeq ($$(findstring ios,$$(OSTYPE_$(1))),ios)
# See comment above
$$(BACKTRACE_LIB_$(1)):
	touch $$@
else ifeq ($$(findstring msvc,$(1)),msvc)
# See comment above
$$(BACKTRACE_LIB_$(1)):
	touch $$@
else ifeq ($$(findstring emscripten,$(1)),emscripten)
# FIXME: libbacktrace doesn't understand the emscripten triple
$$(BACKTRACE_LIB_$(1)):
	touch $$@
else

ifdef CFG_ENABLE_FAST_MAKE
BACKTRACE_DEPS := $(S)/.gitmodules
else
BACKTRACE_DEPS := $(wildcard $(S)src/libbacktrace/*)
endif

# We need to export CFLAGS because otherwise it doesn't pick up cross compile
# builds. If libbacktrace doesn't realize this, it will attempt to read 64-bit
# elf headers when compiled for a 32-bit system, yielding blank backtraces.
#
# This also removes the -Werror flag specifically to prevent errors during
# configuration.
#
# Down below you'll also see echos into the config.h generated by the
# ./configure script. This is done to force libbacktrace to *not* use the
# atomic/sync functionality because it pulls in unnecessary dependencies and we
# never use it anyway.
#
# We also use `env PWD=` to clear the PWD environment variable, and then
# execute the command in a new shell. This is necessary to workaround a
# buildbot/msys2 bug: the shell is launched with PWD set to a windows-style path,
# which results in all further uses of `pwd` also printing a windows-style path,
# which breaks libbacktrace's configure script. Clearing PWD within the same
# shell is not sufficient.

$$(BACKTRACE_BUILD_DIR_$(1))/Makefile: $$(BACKTRACE_DEPS) $$(MKFILE_DEPS)
	@$$(call E, configure: libbacktrace for $(1))
	$$(Q)rm -rf $$(BACKTRACE_BUILD_DIR_$(1))
	$$(Q)mkdir -p $$(BACKTRACE_BUILD_DIR_$(1))
	$$(Q)(cd $$(BACKTRACE_BUILD_DIR_$(1)) && env \
	      PWD= \
	      CC="$$(CC_$(1))" \
	      AR="$$(AR_$(1))" \
	      RANLIB="$$(AR_$(1)) s" \
	      CFLAGS="$$(CFG_GCCISH_CFLAGS_$(1)) -Wno-error -fno-stack-protector" \
	      $(S)src/libbacktrace/configure --build=$(CFG_GNU_TRIPLE_$(CFG_BUILD)) --host=$(CFG_GNU_TRIPLE_$(1)))
	$$(Q)echo '#undef HAVE_ATOMIC_FUNCTIONS' >> \
	      $$(BACKTRACE_BUILD_DIR_$(1))/config.h
	$$(Q)echo '#undef HAVE_SYNC_FUNCTIONS' >> \
	      $$(BACKTRACE_BUILD_DIR_$(1))/config.h

$$(BACKTRACE_LIB_$(1)): $$(BACKTRACE_BUILD_DIR_$(1))/Makefile $$(MKFILE_DEPS)
	@$$(call E, make: libbacktrace)
	$$(Q)$$(MAKE) -C $$(BACKTRACE_BUILD_DIR_$(1)) \
		INCDIR=$(S)src/libbacktrace
	$$(Q)cp $$(BACKTRACE_BUILD_DIR_$(1))/.libs/libbacktrace.a $$@

endif

################################################################################
# libc/libunwind for musl
#
# When we're building a musl-like target we're going to link libc/libunwind
# statically into the standard library and liblibc, so we need to make sure
# they're in a location that we can find
################################################################################

ifeq ($$(findstring musl,$(1)),musl)
$$(RT_OUTPUT_DIR_$(1))/%: $$(CFG_MUSL_ROOT)/lib/%
	cp $$^ $$@
else
# Ask gcc where it is
$$(RT_OUTPUT_DIR_$(1))/%:
	cp $$(shell $$(CC_$(1)) -print-file-name=$$(@F)) $$@
endif

endef

# Instantiate template for all stages/targets
$(foreach target,$(CFG_TARGET), \
     $(eval $(call DEF_THIRD_PARTY_TARGETS,$(target))))
