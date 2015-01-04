# Copyright 2014 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

BG = $(CFG_BUILD_DIR)/grammar/
SG = $(S)src/grammar/
B = $(CFG_BUILD_DIR)/$(CFG_BUILD)/stage2/
L = $(B)lib/rustlib/$(CFG_BUILD)/lib
LD = $(CFG_BUILD)/stage2/lib/rustlib/$(CFG_BUILD)/lib/
RUSTC = $(STAGE2_T_$(CFG_BUILD)_H_$(CFG_BUILD))

# Run the reference lexer against libsyntax and compare the tokens and spans.
# If "// ignore-lexer-test" is present in the file, it will be ignored.
#
# $(1) is the file to test.
define LEXER_TEST
grep "// ignore-lexer-test" $(1) ; \
  if [ $$? -eq 1 ]; then \
   CLASSPATH=$(B)grammar $(CFG_GRUN) RustLexer tokens -tokens < $(1) \
   | $(B)grammar/verify $(1) ; \
  fi
endef

$(BG):
	$(Q)mkdir -p $(BG)

$(BG)RustLexer.class: $(BG) $(SG)RustLexer.g4
	$(Q)$(CFG_ANTLR4) -o $(BG) $(SG)RustLexer.g4
	$(Q)$(CFG_JAVAC) -d $(BG) $(BG)RustLexer.java

check-build-lexer-verifier: $(BG)verify

ifeq ($(NO_REBUILD),)
VERIFY_DEPS :=  rustc-stage2-H-$(CFG_BUILD) $(LD)stamp.rustc
else
VERIFY_DEPS :=
endif

$(BG)verify: $(BG) $(SG)verify.rs $(VERIFY_DEPS)
	$(Q)$(RUSTC) --out-dir $(BG) -L $(L) $(SG)verify.rs

ifdef CFG_JAVAC
ifdef CFG_ANTLR4
ifdef CFG_GRUN
check-lexer: $(BG) $(BG)RustLexer.class check-build-lexer-verifier
	$(info Verifying libsyntax against the reference lexer ...)
	$(Q)$(SG)check.sh $(S) "$(BG)" \
		"$(CFG_GRUN)" "$(BG)verify" "$(BG)RustLexer.tokens"
else
$(info cfg: grun not available, skipping lexer test...)
check-lexer:

endif
else
$(info cfg: antlr4 not available, skipping lexer test...)
check-lexer:

endif
else
$(info cfg: javac not available, skipping lexer test...)
check-lexer:

endif
