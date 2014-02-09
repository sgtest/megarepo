# Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

######################################################################
# Doc variables and rules
######################################################################

DOCS :=
CDOCS :=
DOCS_L10N :=
HTML_DEPS := doc/

BASE_DOC_OPTS := --standalone --toc --number-sections
HTML_OPTS = $(BASE_DOC_OPTS) --to=html5 --section-divs --css=rust.css \
    --include-before-body=doc/version_info.html \
    --include-in-header=doc/favicon.inc --include-after-body=doc/footer.inc
TEX_OPTS = $(BASE_DOC_OPTS) --include-before-body=doc/version.md \
    --from=markdown --include-before-body=doc/footer.tex --to=latex
EPUB_OPTS = $(BASE_DOC_OPTS) --to=epub

D := $(S)src/doc

######################################################################
# Rust version
######################################################################

doc/version.md: $(MKFILE_DEPS) $(wildcard $(D)/*.*) | doc/
	@$(call E, version-stamp: $@)
	$(Q)echo "$(CFG_VERSION)" >$@

HTML_DEPS += doc/version_info.html
doc/version_info.html: $(D)/version_info.html.template $(MKFILE_DEPS) \
                       $(wildcard $(D)/*.*) | doc/
	@$(call E, version-info: $@)
	sed -e "s/VERSION/$(CFG_RELEASE)/; s/SHORT_HASH/$(shell echo \
                    $(CFG_VER_HASH) | head -c 8)/;\
                s/STAMP/$(CFG_VER_HASH)/;" $< >$@

GENERATED += doc/version.md doc/version_info.html

######################################################################
# Docs, from pandoc, rustdoc (which runs pandoc), and node
######################################################################

doc/:
	@mkdir -p $@

HTML_DEPS += doc/rust.css
doc/rust.css: $(D)/rust.css | doc/
	@$(call E, cp: $@)
	$(Q)cp -a $< $@ 2> /dev/null

HTML_DEPS += doc/favicon.inc
doc/favicon.inc: $(D)/favicon.inc | doc/
	@$(call E, cp: $@)
	$(Q)cp -a $< $@ 2> /dev/null

doc/full-toc.inc: $(D)/full-toc.inc | doc/
	@$(call E, cp: $@)
	$(Q)cp -a $< $@ 2> /dev/null

HTML_DEPS += doc/footer.inc
doc/footer.inc: $(D)/footer.inc | doc/
	@$(call E, cp: $@)
	$(Q)cp -a $< $@ 2> /dev/null

doc/footer.tex: $(D)/footer.tex | doc/
	@$(call E, cp: $@)
	$(Q)cp -a $< $@ 2> /dev/null

ifeq ($(CFG_PANDOC),)
  $(info cfg: no pandoc found, omitting docs)
  NO_DOCS = 1
endif

ifeq ($(CFG_NODE),)
  $(info cfg: no node found, omitting docs)
  NO_DOCS = 1
endif

ifneq ($(NO_DOCS),1)

DOCS += doc/rust.html
doc/rust.html: $(D)/rust.md doc/full-toc.inc $(HTML_DEPS) | doc/
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --include-in-header=doc/full-toc.inc --output=$@

DOCS += doc/rust.tex
doc/rust.tex: $(D)/rust.md doc/footer.tex doc/version.md | doc/
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js $< | \
	$(CFG_PANDOC) $(TEX_OPTS) --output=$@

DOCS += doc/rust.epub
doc/rust.epub: $(D)/rust.md | doc/
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(EPUB_OPTS) --output=$@

DOCS += doc/rustdoc.html
doc/rustdoc.html: $(D)/rustdoc.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/tutorial.html
doc/tutorial.html: $(D)/tutorial.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/tutorial.tex
doc/tutorial.tex: $(D)/tutorial.md doc/footer.tex doc/version.md
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js $< | \
	$(CFG_PANDOC) $(TEX_OPTS) --output=$@

DOCS += doc/tutorial.epub
doc/tutorial.epub: $(D)/tutorial.md
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(EPUB_OPTS) --output=$@


DOCS_L10N += doc/l10n/ja/tutorial.html
doc/l10n/ja/tutorial.html: doc/l10n/ja/tutorial.md doc/version_info.html doc/rust.css
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight doc/l10n/ja/tutorial.md | \
          $(CFG_PANDOC) --standalone --toc \
           --section-divs --number-sections \
           --from=markdown --to=html5 --css=../../rust.css \
           --include-before-body=doc/version_info.html \
           --output=$@

# Complementary documentation
#
DOCS += doc/index.html
doc/index.html: $(D)/index.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/complement-lang-faq.html
doc/complement-lang-faq.html: $(D)/complement-lang-faq.md doc/full-toc.inc $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --include-in-header=doc/full-toc.inc --output=$@

DOCS += doc/complement-project-faq.html
doc/complement-project-faq.html: $(D)/complement-project-faq.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/complement-cheatsheet.html
doc/complement-cheatsheet.html: $(D)/complement-cheatsheet.md doc/full-toc.inc $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --include-in-header=doc/full-toc.inc --output=$@

DOCS += doc/complement-bugreport.html
doc/complement-bugreport.html: $(D)/complement-bugreport.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

# Guides

DOCS += doc/guide-macros.html
doc/guide-macros.html: $(D)/guide-macros.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-container.html
doc/guide-container.html: $(D)/guide-container.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-ffi.html
doc/guide-ffi.html: $(D)/guide-ffi.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-testing.html
doc/guide-testing.html: $(D)/guide-testing.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-lifetimes.html
doc/guide-lifetimes.html: $(D)/guide-lifetimes.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-tasks.html
doc/guide-tasks.html: $(D)/guide-tasks.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-pointers.html
doc/guide-pointers.html: $(D)/guide-pointers.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

DOCS += doc/guide-runtime.html
doc/guide-runtime.html: $(D)/guide-runtime.md $(HTML_DEPS)
	@$(call E, pandoc: $@)
	$(Q)$(CFG_NODE) $(D)/prep.js --highlight $< | \
	$(CFG_PANDOC) $(HTML_OPTS) --output=$@

  ifeq ($(CFG_PDFLATEX),)
    $(info cfg: no pdflatex found, omitting doc/rust.pdf)
  else
    ifeq ($(CFG_XETEX),)
      $(info cfg: no xetex found, disabling doc/rust.pdf)
    else
      ifeq ($(CFG_LUATEX),)
        $(info cfg: lacking luatex, disabling pdflatex)
      else

DOCS += doc/rust.pdf
doc/rust.pdf: doc/rust.tex
	@$(call E, pdflatex: $@)
	$(Q)$(CFG_PDFLATEX) \
        -interaction=batchmode \
        -output-directory=doc \
        $<

DOCS += doc/tutorial.pdf
doc/tutorial.pdf: doc/tutorial.tex
	@$(call E, pdflatex: $@)
	$(Q)$(CFG_PDFLATEX) \
        -interaction=batchmode \
        -output-directory=doc \
        $<

      endif
    endif
  endif

endif # No pandoc / node

######################################################################
# LLnextgen (grammar analysis from refman)
######################################################################

ifeq ($(CFG_LLNEXTGEN),)
  $(info cfg: no llnextgen found, omitting grammar-verification)
else
.PHONY: verify-grammar

doc/rust.g: rust.md $(S)src/etc/extract_grammar.py
	@$(call E, extract_grammar: $@)
	$(Q)$(CFG_PYTHON) $(S)src/etc/extract_grammar.py $< >$@

verify-grammar: doc/rust.g
	@$(call E, LLnextgen: $<)
	$(Q)$(CFG_LLNEXTGEN) --generate-lexer-wrapper=no $< >$@
	$(Q)rm -f doc/rust.c doc/rust.h
endif


######################################################################
# Rustdoc (libstd/extra)
######################################################################

# The rustdoc executable, rpath included in case --disable-rpath was provided to
# ./configure
RUSTDOC = $(HBIN2_H_$(CFG_BUILD))/rustdoc$(X_$(CFG_BUILD))

# The library documenting macro
#
# $(1) - The crate name (std/extra)
#
# Passes --cfg stage2 to rustdoc because it uses the stage2 librustc.
define libdoc
doc/$(1)/index.html:							    \
	    $$(CRATEFILE_$(1))						    \
	    $$(RSINPUTS_$(1))						    \
	    $$(RUSTDOC)							    \
	    $$(foreach dep,$$(RUST_DEPS_$(1)),				    \
		$$(TLIB2_T_$(CFG_BUILD)_H_$(CFG_BUILD))/stamp.$$(dep))
	@$$(call E, rustdoc: $$@)
	$$(Q)$$(RPATH_VAR2_T_$(CFG_BUILD)_H_$(CFG_BUILD)) $$(RUSTDOC) \
	    --cfg stage2 $$<

endef

$(foreach crate,$(CRATES),$(eval $(call libdoc,$(crate))))

DOCS += $(DOC_CRATES:%=doc/%/index.html)

CDOCS += doc/rustc/index.html
CDOCS += doc/syntax/index.html

ifdef CFG_DISABLE_DOCS
  $(info cfg: disabling doc build (CFG_DISABLE_DOCS))
  DOCS :=
endif

docs: $(DOCS)
compiler-docs: $(CDOCS)

docs-l10n: $(DOCS_L10N)

doc/l10n/%.md: doc/po/%.md.po doc/po4a.conf
	po4a --copyright-holder="The Rust Project Developers" \
	     --package-name="Rust" \
	     --package-version="$(CFG_RELEASE)" \
	     -M UTF-8 -L UTF-8 \
	     doc/po4a.conf

.PHONY: docs-l10n
