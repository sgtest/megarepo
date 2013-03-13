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
# Test variables
######################################################################

# The names of crates that must be tested
TEST_TARGET_CRATES = core std
TEST_HOST_CRATES = syntax rustc rustdoc rusti rust rustpkg
TEST_CRATES = $(TEST_TARGET_CRATES) $(TEST_HOST_CRATES)

# Markdown files under doc/ that should have their code extracted and run
DOC_TEST_NAMES = tutorial tutorial-ffi tutorial-macros tutorial-borrowed-ptr tutorial-tasks rust

######################################################################
# Environment configuration
######################################################################

# The arguments to all test runners
ifdef TESTNAME
  TESTARGS += $(TESTNAME)
endif

ifdef CHECK_XFAILS
  TESTARGS += --ignored
endif

# Arguments to the cfail/rfail/rpass/bench tests
ifdef CFG_VALGRIND
  CTEST_RUNTOOL = --runtool "$(CFG_VALGRIND)"
endif

# Arguments to the perf tests
ifdef CFG_PERF_TOOL
  CTEST_PERF_RUNTOOL = --runtool "$(CFG_PERF_TOOL)"
endif

CTEST_TESTARGS := $(TESTARGS)

ifdef VERBOSE
  CTEST_TESTARGS += --verbose
endif

# If we're running perf then set this environment variable
# to put the benchmarks into 'hard mode'
ifeq ($(MAKECMDGOALS),perf)
  RUST_BENCH=1
  export RUST_BENCH
endif

TEST_LOG_FILE=tmp/check-stage$(1)-T-$(2)-H-$(3)-$(4).log
TEST_OK_FILE=tmp/check-stage$(1)-T-$(2)-H-$(3)-$(4).ok

define DEF_TARGET_COMMANDS

ifdef CFG_UNIXY_$(1)
  CFG_RUN_TEST_$(1)=$$(call CFG_RUN_$(1),,$$(CFG_VALGRIND) $$(1))
endif

ifdef CFG_WINDOWSY_$(1)
  CFG_TESTLIB_$(1)=$$(CFG_BUILD_DIR)/$$(2)/$$(strip \
   $$(if $$(findstring stage0,$$(1)), \
       stage0/$$(CFG_LIBDIR), \
      $$(if $$(findstring stage1,$$(1)), \
           stage1/$$(CFG_LIBDIR), \
          $$(if $$(findstring stage2,$$(1)), \
               stage2/$$(CFG_LIBDIR), \
               $$(if $$(findstring stage3,$$(1)), \
                    stage3/$$(CFG_LIBDIR), \
               )))))/rustc/$$(CFG_BUILD_TRIPLE)/$$(CFG_LIBDIR)
  CFG_RUN_TEST_$(1)=$$(call CFG_RUN_$(1),$$(call CFG_TESTLIB_$(1),$$(1),$$(3)),$$(1))
endif

# Run the compiletest runner itself under valgrind
ifdef CTEST_VALGRIND
CFG_RUN_CTEST_$(1)=$$(call CFG_RUN_TEST_$$(CFG_BUILD_TRIPLE),$$(2),$$(3))
else
CFG_RUN_CTEST_$(1)=$$(call CFG_RUN_$$(CFG_BUILD_TRIPLE),$$(TLIB$$(1)_T_$$(3)_H_$$(3)),$$(2))
endif

endef

$(foreach target,$(CFG_TARGET_TRIPLES), \
  $(eval $(call DEF_TARGET_COMMANDS,$(target))))


######################################################################
# Main test targets
######################################################################

check: cleantestlibs cleantmptestlogs tidy all check-stage2
	$(Q)$(CFG_PYTHON) $(S)src/etc/check-summary.py tmp/*.log

check-notidy: cleantestlibs cleantmptestlogs all check-stage2
	$(Q)$(CFG_PYTHON) $(S)src/etc/check-summary.py tmp/*.log

check-full: cleantestlibs cleantmptestlogs tidy \
            all check-stage1 check-stage2 check-stage3
	$(Q)$(CFG_PYTHON) $(S)src/etc/check-summary.py tmp/*.log

check-test: cleantestlibs cleantmptestlogs all check-stage2-rfail
	$(Q)$(CFG_PYTHON) $(S)src/etc/check-summary.py tmp/*.log

check-lite: cleantestlibs cleantmptestlogs \
	check-stage2-core check-stage2-std check-stage2-rpass \
	check-stage2-rfail check-stage2-cfail
	$(Q)$(CFG_PYTHON) $(S)src/etc/check-summary.py tmp/*.log

.PHONY: cleantmptestlogs cleantestlibs

cleantmptestlogs:
	$(Q)rm -f tmp/*.log

cleantestlibs:
	$(Q)find $(CFG_BUILD_TRIPLE)/test \
         -name '*.[odasS]' -o \
         -name '*.so' -o      \
         -name '*.dylib' -o   \
         -name '*.dll' -o     \
         -name '*.def' -o     \
         -name '*.bc' -o      \
         -name '*.dSYM' -o    \
         -name '*.libaux' -o      \
         -name '*.out' -o     \
         -name '*.err' -o     \
	 -name '*.debugger.script' \
         | xargs rm -rf


######################################################################
# Tidy
######################################################################

ifdef CFG_NOTIDY
tidy:
else

ALL_CS := $(wildcard $(S)src/rt/*.cpp \
                     $(S)src/rt/*/*.cpp \
                     $(S)src/rt/*/*/*.cpp \
                     $(S)srcrustllvm/*.cpp)
ALL_CS := $(filter-out $(S)src/rt/bigint/bigint_ext.cpp \
                       $(S)src/rt/bigint/bigint_int.cpp \
                       $(S)src/rt/miniz.cpp \
                       $(S)src/rt/linenoise/linenoise.c \
                       $(S)src/rt/linenoise/utf8.c \
	,$(ALL_CS))
ALL_HS := $(wildcard $(S)src/rt/*.h \
                     $(S)src/rt/*/*.h \
                     $(S)src/rt/*/*/*.h \
                     $(S)srcrustllvm/*.h)
ALL_HS := $(filter-out $(S)src/rt/vg/valgrind.h \
                       $(S)src/rt/vg/memcheck.h \
                       $(S)src/rt/uthash/uthash.h \
                       $(S)src/rt/uthash/utlist.h \
                       $(S)src/rt/msvc/typeof.h \
                       $(S)src/rt/msvc/stdint.h \
                       $(S)src/rt/msvc/inttypes.h \
                       $(S)src/rt/bigint/bigint.h \
                       $(S)src/rt/linenoise/linenoise.h \
                       $(S)src/rt/linenoise/utf8.h \
	,$(ALL_HS))

# Run the tidy script in multiple parts to avoid huge 'echo' commands
tidy:
		@$(call E, check: formatting)
		$(Q)find $(S)src -name '*.r[sc]' \
		| grep '^$(S)src/test' -v \
		| xargs -n 10 $(CFG_PYTHON) $(S)src/etc/tidy.py
		$(Q)find $(S)src/etc -name '*.py' \
		| xargs -n 10 $(CFG_PYTHON) $(S)src/etc/tidy.py
		$(Q)echo $(ALL_CS) \
	  	| xargs -n 10 $(CFG_PYTHON) $(S)src/etc/tidy.py
		$(Q)echo $(ALL_HS) \
	  	| xargs -n 10 $(CFG_PYTHON) $(S)src/etc/tidy.py

endif


######################################################################
# Sets of tests
######################################################################

define DEF_TEST_SETS

check-stage$(1)-T-$(2)-H-$(3)-exec:     				\
	check-stage$(1)-T-$(2)-H-$(3)-rpass-exec			\
	check-stage$(1)-T-$(2)-H-$(3)-rfail-exec			\
	check-stage$(1)-T-$(2)-H-$(3)-cfail-exec			\
	check-stage$(1)-T-$(2)-H-$(3)-rpass-full-exec			\
        check-stage$(1)-T-$(2)-H-$(3)-crates-exec                      \
	check-stage$(1)-T-$(2)-H-$(3)-bench-exec			\
	check-stage$(1)-T-$(2)-H-$(3)-debuginfo-exec \
	check-stage$(1)-T-$(2)-H-$(3)-doc-exec \
	check-stage$(1)-T-$(2)-H-$(3)-pretty-exec

# Only test the compiler-dependent crates when the target is
# able to build a compiler (when the target triple is in the set of host triples)
ifneq ($$(findstring $(2),$$(CFG_HOST_TRIPLES)),)

check-stage$(1)-T-$(2)-H-$(3)-crates-exec: \
	$$(foreach crate,$$(TEST_CRATES), \
           check-stage$(1)-T-$(2)-H-$(3)-$$(crate)-exec)

else

check-stage$(1)-T-$(2)-H-$(3)-crates-exec: \
	$$(foreach crate,$$(TEST_TARGET_CRATES), \
           check-stage$(1)-T-$(2)-H-$(3)-$$(crate)-exec)

endif

check-stage$(1)-T-$(2)-H-$(3)-doc-exec: \
        $$(foreach docname,$$(DOC_TEST_NAMES), \
           check-stage$(1)-T-$(2)-H-$(3)-doc-$$(docname)-exec)

check-stage$(1)-T-$(2)-H-$(3)-pretty-exec: \
	check-stage$(1)-T-$(2)-H-$(3)-pretty-rpass-exec	\
	check-stage$(1)-T-$(2)-H-$(3)-pretty-rpass-full-exec	\
	check-stage$(1)-T-$(2)-H-$(3)-pretty-rfail-exec	\
	check-stage$(1)-T-$(2)-H-$(3)-pretty-bench-exec	\
	check-stage$(1)-T-$(2)-H-$(3)-pretty-pretty-exec

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(foreach stage,$(STAGES), \
    $(eval $(call DEF_TEST_SETS,$(stage),$(target),$(host))))))


######################################################################
# Crate testing
######################################################################

define TEST_RUNNER

$(3)/test/coretest.stage$(1)-$(2)$$(X_$(2)):			\
		$$(CORELIB_CRATE) $$(CORELIB_INPUTS)	\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_STDLIB_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/stdtest.stage$(1)-$(2)$$(X_$(2)):			\
		$$(STDLIB_CRATE) $$(STDLIB_INPUTS)	\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_STDLIB_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/syntaxtest.stage$(1)-$(2)$$(X_$(2)):			\
		$$(LIBSYNTAX_CRATE) $$(LIBSYNTAX_INPUTS)	\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_STDLIB_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/rustctest.stage$(1)-$(2)$$(X_$(2)):					\
		$$(COMPILER_CRATE) $$(COMPILER_INPUTS) \
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_RUSTLLVM_$(2)) \
                $$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_LIBSYNTAX_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/rustpkgtest.stage$(1)-$(2)$$(X_$(2)):					\
		$$(RUSTPKG_LIB) $$(RUSTPKG_INPUTS)		\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_LIBRUSTC_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/rustitest.stage$(1)-$(2)$$(X_$(2)):					\
		$$(RUSTI_LIB) $$(RUSTI_INPUTS)		\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_LIBRUSTC_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/rusttest.stage$(1)-$(2)$$(X_$(2)):					\
		$$(RUST_LIB) $$(RUST_INPUTS)		\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_LIBRUSTC_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

$(3)/test/rustdoctest.stage$(1)-$(2)$$(X_$(2)):					\
		$$(RUSTDOC_LIB) $$(RUSTDOC_INPUTS)		\
		$$(TLIB$(1)_T_$(2)_H_$(3))/$$(CFG_LIBRUSTC_$(2))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE$(1)_T_$(2)_H_$(3)) -o $$@ $$< --test

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(eval $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(eval $(foreach stage,$(STAGES), \
   $(eval $(call TEST_RUNNER,$(stage),$(target),$(host))))))))

define DEF_TEST_CRATE_RULES
check-stage$(1)-T-$(2)-H-$(3)-$(4)-exec: $$(call TEST_OK_FILE,$(1),$(2),$(3),$(4))

$$(call TEST_OK_FILE,$(1),$(2),$(3),$(4)): \
		$(3)/test/$(4)test.stage$(1)-$(2)$$(X_$(2))
	@$$(call E, run: $$<)
	$$(Q)$$(call CFG_RUN_TEST_$(2),$$<,$(2),$(3)) $$(TESTARGS)	\
	--logfile $$(call TEST_LOG_FILE,$(1),$(2),$(3),$(4)) \
	&& touch $$@
endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(foreach stage,$(STAGES), \
   $(foreach crate, $(TEST_CRATES), \
    $(eval $(call DEF_TEST_CRATE_RULES,$(stage),$(target),$(host),$(crate)))))))


######################################################################
# Rules for the compiletest tests (rpass, rfail, etc.)
######################################################################

RPASS_RC := $(wildcard $(S)src/test/run-pass/*.rc)
RPASS_RS := $(wildcard $(S)src/test/run-pass/*.rs)
RPASS_FULL_RC := $(wildcard $(S)src/test/run-pass-fulldeps/*.rc)
RPASS_FULL_RS := $(wildcard $(S)src/test/run-pass-fulldeps/*.rs)
RFAIL_RC := $(wildcard $(S)src/test/run-fail/*.rc)
RFAIL_RS := $(wildcard $(S)src/test/run-fail/*.rs)
CFAIL_RC := $(wildcard $(S)src/test/compile-fail/*.rc)
CFAIL_RS := $(wildcard $(S)src/test/compile-fail/*.rs)
BENCH_RS := $(wildcard $(S)src/test/bench/*.rs)
PRETTY_RS := $(wildcard $(S)src/test/pretty/*.rs)
DEBUGINFO_RS := $(wildcard $(S)src/test/debug-info/*.rs)

# perf tests are the same as bench tests only they run under
# a performance monitor.
PERF_RS := $(wildcard $(S)src/test/bench/*.rs)

RPASS_TESTS := $(RPASS_RC) $(RPASS_RS)
RPASS_FULL_TESTS := $(RPASS_FULL_RC) $(RPASS_FULL_RS)
RFAIL_TESTS := $(RFAIL_RC) $(RFAIL_RS)
CFAIL_TESTS := $(CFAIL_RC) $(CFAIL_RS)
BENCH_TESTS := $(BENCH_RS)
PERF_TESTS := $(PERF_RS)
PRETTY_TESTS := $(PRETTY_RS)
DEBUGINFO_TESTS := $(DEBUGINFO_RS)

CTEST_SRC_BASE_rpass = run-pass
CTEST_BUILD_BASE_rpass = run-pass
CTEST_MODE_rpass = run-pass
CTEST_RUNTOOL_rpass = $(CTEST_RUNTOOL)

CTEST_SRC_BASE_rpass-full = run-pass-full
CTEST_BUILD_BASE_rpass-full = run-pass-full
CTEST_MODE_rpass-full = run-pass
CTEST_RUNTOOL_rpass-full = $(CTEST_RUNTOOL)

CTEST_SRC_BASE_rfail = run-fail
CTEST_BUILD_BASE_rfail = run-fail
CTEST_MODE_rfail = run-fail
CTEST_RUNTOOL_rfail = $(CTEST_RUNTOOL)

CTEST_SRC_BASE_cfail = compile-fail
CTEST_BUILD_BASE_cfail = compile-fail
CTEST_MODE_cfail = compile-fail
CTEST_RUNTOOL_cfail = $(CTEST_RUNTOOL)

CTEST_SRC_BASE_bench = bench
CTEST_BUILD_BASE_bench = bench
CTEST_MODE_bench = run-pass
CTEST_RUNTOOL_bench = $(CTEST_RUNTOOL)

CTEST_SRC_BASE_perf = bench
CTEST_BUILD_BASE_perf = perf
CTEST_MODE_perf = run-pass
CTEST_RUNTOOL_perf = $(CTEST_PERF_RUNTOOL)

CTEST_SRC_BASE_debuginfo = debug-info
CTEST_BUILD_BASE_debuginfo = debug-info
CTEST_MODE_debuginfo = debug-info
CTEST_RUNTOOL_debuginfo = $(CTEST_RUNTOOL)

ifeq ($(CFG_GDB),)
CTEST_DISABLE_debuginfo = "no gdb found"
endif

ifeq ($(CFG_OSTYPE),apple-darwin)
CTEST_DISABLE_debuginfo = "gdb on darwing needs root"
endif

define DEF_CTEST_VARS

# All the per-stage build rules you might want to call from the
# command line.
#
# $(1) is the stage number
# $(2) is the target triple to test
# $(3) is the host triple to test

# Prerequisites for compiletest tests
TEST_SREQ$(1)_T_$(2)_H_$(3) = \
	$$(HBIN$(1)_H_$(3))/compiletest$$(X_$(3)) \
	$$(SREQ$(1)_T_$(2)_H_$(3))

# Rules for the cfail/rfail/rpass/bench/perf test runner

CTEST_COMMON_ARGS$(1)-T-$(2)-H-$(3) :=						\
		--compile-lib-path $$(HLIB$(1)_H_$(3))				\
        --run-lib-path $$(TLIB$(1)_T_$(2)_H_$(3))			\
        --rustc-path $$(HBIN$(1)_H_$(3))/rustc$$(X_$(3))			\
        --aux-base $$(S)src/test/auxiliary/                 \
        --stage-id stage$(1)-$(2)							\
        --rustcflags "$(RUSTC_FLAGS_$(2)) $$(CFG_RUSTC_FLAGS) --target=$(2)" \
        $$(CTEST_TESTARGS)

CTEST_DEPS_rpass_$(1)-T-$(2)-H-$(3) = $$(RPASS_TESTS)
CTEST_DEPS_rpass_full_$(1)-T-$(2)-H-$(3) = $$(RPASS_FULL_TESTS) $$(TLIBRUSTC_DEFAULT$(1)_T_$(2)_H_$(3))
CTEST_DEPS_rfail_$(1)-T-$(2)-H-$(3) = $$(RFAIL_TESTS)
CTEST_DEPS_cfail_$(1)-T-$(2)-H-$(3) = $$(CFAIL_TESTS)
CTEST_DEPS_bench_$(1)-T-$(2)-H-$(3) = $$(BENCH_TESTS)
CTEST_DEPS_perf_$(1)-T-$(2)-H-$(3) = $$(PERF_TESTS)
CTEST_DEPS_debuginfo_$(1)-T-$(2)-H-$(3) = $$(DEBUGINFO_TESTS)

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(eval $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(eval $(foreach stage,$(STAGES), \
   $(eval $(call DEF_CTEST_VARS,$(stage),$(target),$(host))))))))

define DEF_RUN_COMPILETEST

CTEST_ARGS$(1)-T-$(2)-H-$(3)-$(4) := \
        $$(CTEST_COMMON_ARGS$(1)-T-$(2)-H-$(3))	\
        --src-base $$(S)src/test/$$(CTEST_SRC_BASE_$(4))/ \
        --build-base $(3)/test/$$(CTEST_BUILD_BASE_$(4))/ \
        --mode $$(CTEST_MODE_$(4)) \
	$$(CTEST_RUNTOOL_$(4))

check-stage$(1)-T-$(2)-H-$(3)-$(4)-exec: $$(call TEST_OK_FILE,$(1),$(2),$(3),$(4))

ifeq ($$(CTEST_DISABLE_$(4)),)

$$(call TEST_OK_FILE,$(1),$(2),$(3),$(4)): \
		$$(TEST_SREQ$(1)_T_$(2)_H_$(3)) \
                $$(CTEST_DEPS_$(4)_$(1)-T-$(2)-H-$(3))
	@$$(call E, run $(4): $$<)
	$$(Q)$$(call CFG_RUN_CTEST_$(2),$(1),$$<,$(3)) \
		$$(CTEST_ARGS$(1)-T-$(2)-H-$(3)-$(4)) \
		--logfile $$(call TEST_LOG_FILE,$(1),$(2),$(3),$(4)) \
                && touch $$@

else

$$(call TEST_OK_FILE,$(1),$(2),$(3),$(4)): \
		$$(TEST_SREQ$(1)_T_$(2)_H_$(3)) \
                $$(CTEST_DEPS_$(4)_$(1)-T-$(2)-H-$(3))
	@$$(call E, run $(4): $$<)
	@$$(call E, warning: tests disabled: $$(CTEST_DISABLE_$(4)))
	touch $$@

endif

endef

CTEST_NAMES = rpass rpass-full rfail cfail bench perf debuginfo

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(eval $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(eval $(foreach stage,$(STAGES), \
   $(eval $(foreach name,$(CTEST_NAMES), \
   $(eval $(call DEF_RUN_COMPILETEST,$(stage),$(target),$(host),$(name))))))))))

PRETTY_NAMES = pretty-rpass pretty-rpass-full pretty-rfail pretty-bench pretty-pretty
PRETTY_DEPS_pretty-rpass = $(RPASS_TESTS)
PRETTY_DEPS_pretty-rpass-full = $(RPASS_FULL_TESTS)
PRETTY_DEPS_pretty-rfail = $(RFAIL_TESTS)
PRETTY_DEPS_pretty-bench = $(BENCH_TESTS)
PRETTY_DEPS_pretty-pretty = $(PRETTY_TESTS)
PRETTY_DIRNAME_pretty-rpass = run-pass
PRETTY_DIRNAME_pretty-rpass-full = run-pass-full
PRETTY_DIRNAME_pretty-rfail = run-fail
PRETTY_DIRNAME_pretty-bench = bench
PRETTY_DIRNAME_pretty-pretty = pretty

define DEF_RUN_PRETTY_TEST

PRETTY_ARGS$(1)-T-$(2)-H-$(3)-$(4) :=			\
		$$(CTEST_COMMON_ARGS$(1)-T-$(2)-H-$(3))	\
        --src-base $$(S)src/test/$$(PRETTY_DIRNAME_$(4))/ \
        --build-base $(3)/test/$$(PRETTY_DIRNAME_$(4))/ \
        --mode pretty

check-stage$(1)-T-$(2)-H-$(3)-$(4)-exec: $$(call TEST_OK_FILE,$(1),$(2),$(3),$(4))

$$(call TEST_OK_FILE,$(1),$(2),$(3),$(4)): \
	        $$(TEST_SREQ$(1)_T_$(2)_H_$(3))		\
	        $$(PRETTY_DEPS_$(4))
	@$$(call E, run pretty-rpass: $$<)
	$$(Q)$$(call CFG_RUN_CTEST_$(2),$(1),$$<,$(3)) \
		$$(PRETTY_ARGS$(1)-T-$(2)-H-$(3)-$(4)) \
		--logfile $$(call TEST_LOG_FILE,$(1),$(2),$(3),$(4)) \
                && touch $$@

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(foreach stage,$(STAGES), \
   $(foreach pretty-name,$(PRETTY_NAMES), \
    $(eval $(call DEF_RUN_PRETTY_TEST,$(stage),$(target),$(host),$(pretty-name)))))))

define DEF_RUN_DOC_TEST

DOC_TEST_ARGS$(1)-T-$(2)-H-$(3)-doc-$(4) := \
        $$(CTEST_COMMON_ARGS$(1)-T-$(2)-H-$(3))	\
        --src-base $(3)/test/doc-$(4)/	\
        --build-base $(3)/test/doc-$(4)/	\
        --mode run-pass

check-stage$(1)-T-$(2)-H-$(3)-doc-$(4)-exec: $$(call TEST_OK_FILE,$(1),$(2),$(3),doc-$(4))

$$(call TEST_OK_FILE,$(1),$(2),$(3),doc-$(4)): \
	        $$(TEST_SREQ$(1)_T_$(2)_H_$(3))		\
                doc-$(4)-extract$(3)
	@$$(call E, run doc-$(4): $$<)
	$$(Q)$$(call CFG_RUN_CTEST_$(2),$(1),$$<,$(3)) \
                $$(DOC_TEST_ARGS$(1)-T-$(2)-H-$(3)-doc-$(4)) \
		--logfile $$(call TEST_LOG_FILE,$(1),$(2),$(3),doc-$(4)) \
                && touch $$@

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(foreach stage,$(STAGES), \
   $(foreach docname,$(DOC_TEST_NAMES), \
    $(eval $(call DEF_RUN_DOC_TEST,$(stage),$(target),$(host),$(docname)))))))


######################################################################
# Extracting tests for docs
######################################################################

EXTRACT_TESTS := "$(CFG_PYTHON)" $(S)src/etc/extract-tests.py

define DEF_DOC_TEST_HOST

doc-$(2)-extract$(1):
	@$$(call E, extract: $(2) tests)
	$$(Q)rm -f $(1)/test/doc-$(2)/*.rs
	$$(Q)$$(EXTRACT_TESTS) $$(S)doc/$(2).md $(1)/test/doc-$(2)

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(foreach docname,$(DOC_TEST_NAMES), \
  $(eval $(call DEF_DOC_TEST_HOST,$(host),$(docname)))))


######################################################################
# Shortcut rules
######################################################################

TEST_GROUPS = \
	crates \
	$(foreach crate,$(TEST_CRATES),$(crate)) \
	rpass \
	rpass-full \
	rfail \
	cfail \
	bench \
	perf \
	debuginfo \
	doc \
	$(foreach docname,$(DOC_TEST_NAMES),$(docname)) \
	pretty \
	pretty-rpass \
	pretty-rpass-full \
	pretty-rfail \
	pretty-bench \
	pretty-pretty \
	$(NULL)

define DEF_CHECK_FOR_STAGE_AND_TARGET_AND_HOST
check-stage$(1)-T-$(2)-H-$(3): check-stage$(1)-T-$(2)-H-$(3)-exec
endef

$(foreach stage,$(STAGES), \
 $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(foreach host,$(CFG_HOST_TRIPLES), \
   $(eval $(call DEF_CHECK_FOR_STAGE_AND_TARGET_AND_HOST,$(stage),$(target),$(host))))))

define DEF_CHECK_FOR_STAGE_AND_TARGET_AND_HOST_AND_GROUP
check-stage$(1)-T-$(2)-H-$(3)-$(4): check-stage$(1)-T-$(2)-H-$(3)-$(4)-exec
endef

$(foreach stage,$(STAGES), \
 $(foreach target,$(CFG_TARGET_TRIPLES), \
  $(foreach host,$(CFG_HOST_TRIPLES), \
   $(foreach group,$(TEST_GROUPS), \
    $(eval $(call DEF_CHECK_FOR_STAGE_AND_TARGET_AND_HOST_AND_GROUP,$(stage),$(target),$(host),$(group)))))))

define DEF_CHECK_FOR_STAGE
check-stage$(1): check-stage$(1)-H-$$(CFG_BUILD_TRIPLE)
check-stage$(1)-H-all: $$(foreach target,$$(CFG_TARGET_TRIPLES), \
                           check-stage$(1)-H-$$(target))
endef

$(foreach stage,$(STAGES), \
 $(eval $(call DEF_CHECK_FOR_STAGE,$(stage))))

define DEF_CHECK_FOR_STAGE_AND_GROUP
check-stage$(1)-$(2): check-stage$(1)-H-$$(CFG_BUILD_TRIPLE)-$(2)
check-stage$(1)-H-all-$(2): $$(foreach target,$$(CFG_TARGET_TRIPLES), \
                               check-stage$(1)-H-$$(target)-$(2))
endef

$(foreach stage,$(STAGES), \
 $(foreach group,$(TEST_GROUPS), \
  $(eval $(call DEF_CHECK_FOR_STAGE_AND_GROUP,$(stage),$(group)))))


define DEF_CHECK_FOR_STAGE_AND_HOSTS
check-stage$(1)-H-$(2): $$(foreach target,$$(CFG_TARGET_TRIPLES), \
                           check-stage$(1)-T-$$(target)-H-$(2))
endef

$(foreach stage,$(STAGES), \
 $(foreach host,$(CFG_HOST_TRIPLES), \
  $(eval $(call DEF_CHECK_FOR_STAGE_AND_HOSTS,$(stage),$(host)))))

define DEF_CHECK_FOR_STAGE_AND_HOSTS_AND_GROUP
check-stage$(1)-H-$(2)-$(3): $$(foreach target,$$(CFG_TARGET_TRIPLES), \
                                check-stage$(1)-T-$$(target)-H-$(2)-$(3))
endef

$(foreach stage,$(STAGES), \
 $(foreach host,$(CFG_HOST_TRIPLES), \
  $(foreach group,$(TEST_GROUPS), \
   $(eval $(call DEF_CHECK_FOR_STAGE_AND_HOSTS_AND_GROUP,$(stage),$(host),$(group))))))

######################################################################
# check-fast rules
######################################################################

FT := run_pass_stage2
FT_LIB := $(call CFG_LIB_NAME_$(CFG_BUILD_TRIPLE),$(FT))
FT_DRIVER := $(FT)_driver

GENERATED += tmp/$(FT).rc tmp/$(FT_DRIVER).rs

tmp/$(FT).rc tmp/$(FT_DRIVER).rs: \
		$(RPASS_TESTS) \
		$(S)src/etc/combine-tests.py
	@$(call E, check: building combined stage2 test runner)
	$(Q)$(CFG_PYTHON) $(S)src/etc/combine-tests.py

define DEF_CHECK_FAST_FOR_T_H
# $(1) unused
# $(2) target triple
# $(3) host triple

$$(TLIB2_T_$(2)_H_$(3))/$$(FT_LIB): \
		tmp/$$(FT).rc \
		$$(SREQ2_T_$(2)_H_$(3))
	@$$(call E, compile_and_link: $$@)
	$$(STAGE2_T_$(2)_H_$(3)) --lib -o $$@ $$<

$(3)/test/$$(FT_DRIVER)-$(2)$$(X_$(2)): \
		tmp/$$(FT_DRIVER).rs \
		$$(TLIB2_T_$(2)_H_$(3))/$$(FT_LIB) \
		$$(SREQ2_T_$(2)_H_$(3))
	@$$(call E, compile_and_link: $$@ $$<)
	$$(STAGE2_T_$(2)_H_$(3)) -o $$@ $$<

$(3)/test/$$(FT_DRIVER)-$(2).out: \
		$(3)/test/$$(FT_DRIVER)-$(2)$$(X_$(2)) \
		$$(SREQ2_T_$(2)_H_$(3))
	$$(Q)$$(call CFG_RUN_TEST_$(2),$$<,$(2),$(3)) \
	--logfile tmp/$$(FT_DRIVER)-$(2).log

check-fast-T-$(2)-H-$(3):     			\
	$(3)/test/$$(FT_DRIVER)-$(2).out

endef

$(foreach host,$(CFG_HOST_TRIPLES), \
 $(eval $(foreach target,$(CFG_TARGET_TRIPLES), \
   $(eval $(call DEF_CHECK_FAST_FOR_T_H,,$(target),$(host))))))

check-fast: tidy check-fast-H-$(CFG_BUILD_TRIPLE)

define DEF_CHECK_FAST_FOR_H

check-fast-H-$(1): 		check-fast-T-$(1)-H-$(1)

endef

$(foreach host,$(CFG_HOST_TRIPLES),			\
 $(eval $(call DEF_CHECK_FAST_FOR_H,$(host))))

