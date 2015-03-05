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
# The various pieces of standalone documentation.
#
# The DOCS variable is their names (with no file extension).
#
# PDF_DOCS lists the targets for which PDF documentation should be
# build.
#
# RUSTDOC_FLAGS_xyz variables are extra arguments to pass to the
# rustdoc invocation for xyz.
#
# RUSTDOC_DEPS_xyz are extra dependencies for the rustdoc invocation
# on xyz.
#
# L10N_LANGS are the languages for which the docs have been
# translated.
######################################################################
DOCS := index intro tutorial \
    complement-lang-faq complement-design-faq complement-project-faq \
    rustdoc reference grammar

# Legacy guides, preserved for a while to reduce the number of 404s
DOCS += guide-crates guide-error-handling guide-ffi guide-macros guide \
    guide-ownership guide-plugins guide-pointers guide-strings guide-tasks \
    guide-testing


PDF_DOCS := reference

RUSTDOC_DEPS_reference := doc/full-toc.inc
RUSTDOC_FLAGS_reference := --html-in-header=doc/full-toc.inc

L10N_LANGS := ja

# Generally no need to edit below here.

# The options are passed to the documentation generators.
RUSTDOC_HTML_OPTS_NO_CSS = --html-before-content=doc/version_info.html \
	--html-in-header=doc/favicon.inc \
	--html-after-content=doc/footer.inc \
	--markdown-playground-url='http://play.rust-lang.org/'

RUSTDOC_HTML_OPTS = $(RUSTDOC_HTML_OPTS_NO_CSS) --markdown-css rust.css

PANDOC_BASE_OPTS := --standalone --toc --number-sections
PANDOC_TEX_OPTS = $(PANDOC_BASE_OPTS) --from=markdown --to=latex \
	--include-before-body=doc/version.tex \
	--include-before-body=doc/footer.tex \
	--include-in-header=doc/uptack.tex
PANDOC_EPUB_OPTS = $(PANDOC_BASE_OPTS) --to=epub

# The rustdoc executable...
RUSTDOC_EXE = $(HBIN2_H_$(CFG_BUILD))/rustdoc$(X_$(CFG_BUILD))
# ...with rpath included in case --disable-rpath was provided to
# ./configure
RUSTDOC = $(RPATH_VAR2_T_$(CFG_BUILD)_H_$(CFG_BUILD)) $(RUSTDOC_EXE)

# The rustbook executable...
RUSTBOOK_EXE = $(HBIN2_H_$(CFG_BUILD))/rustbook$(X_$(CFG_BUILD))
# ...with rpath included in case --disable-rpath was provided to
# ./configure
RUSTBOOK = $(RPATH_VAR2_T_$(CFG_BUILD)_H_$(CFG_BUILD)) $(RUSTBOOK_EXE)

D := $(S)src/doc

DOC_TARGETS := trpl style
COMPILER_DOC_TARGETS :=
DOC_L10N_TARGETS :=

# If NO_REBUILD is set then break the dependencies on rustdoc so we
# build the documentation without having to rebuild rustdoc.
ifeq ($(NO_REBUILD),)
HTML_DEPS := $(RUSTDOC_EXE)
else
HTML_DEPS :=
endif

# Check for xelatex

ifneq ($(CFG_XELATEX),)
    CFG_LATEX := $(CFG_XELATEX)
    XELATEX = 1
  else
    $(info cfg: no xelatex found, disabling LaTeX docs)
    NO_PDF_DOCS = 1
endif

ifeq ($(CFG_PANDOC),)
$(info cfg: no pandoc found, omitting PDF and EPUB docs)
ONLY_HTML_DOCS = 1
endif


######################################################################
# Rust version
######################################################################

doc/version.tex: $(MKFILE_DEPS) $(wildcard $(D)/*.*) | doc/
	@$(call E, version-stamp: $@)
	$(Q)echo "$(CFG_VERSION)" >$@

HTML_DEPS += doc/version_info.html
doc/version_info.html: $(D)/version_info.html.template $(MKFILE_DEPS) \
                       $(wildcard $(D)/*.*) | doc/
	@$(call E, version-info: $@)
	$(Q)sed -e "s/VERSION/$(CFG_RELEASE)/; \
                s/SHORT_HASH/$(CFG_SHORT_VER_HASH)/; \
                s/STAMP/$(CFG_VER_HASH)/;" $< >$@

GENERATED += doc/version.tex doc/version_info.html

######################################################################
# Docs, from rustdoc and sometimes pandoc
######################################################################

doc/:
	@mkdir -p $@

HTML_DEPS += doc/rust.css
doc/rust.css: $(D)/rust.css | doc/
	@$(call E, cp: $@)
	$(Q)cp -PRp $< $@ 2> /dev/null

HTML_DEPS += doc/favicon.inc
doc/favicon.inc: $(D)/favicon.inc | doc/
	@$(call E, cp: $@)
	$(Q)cp -PRp $< $@ 2> /dev/null

doc/full-toc.inc: $(D)/full-toc.inc | doc/
	@$(call E, cp: $@)
	$(Q)cp -PRp $< $@ 2> /dev/null

HTML_DEPS += doc/footer.inc
doc/footer.inc: $(D)/footer.inc | doc/
	@$(call E, cp: $@)
	$(Q)cp -PRp $< $@ 2> /dev/null

# The (english) documentation for each doc item.

define DEF_SHOULD_BUILD_PDF_DOC
SHOULD_BUILD_PDF_DOC_$(1) = 1
endef
$(foreach docname,$(PDF_DOCS),$(eval $(call DEF_SHOULD_BUILD_PDF_DOC,$(docname))))

doc/footer.tex: $(D)/footer.inc | doc/
	@$(call E, pandoc: $@)
	$(CFG_PANDOC) --from=html --to=latex $< --output=$@

doc/uptack.tex: $(D)/uptack.tex | doc/
	$(Q)cp $< $@

# HTML (rustdoc)
DOC_TARGETS += doc/not_found.html
doc/not_found.html: $(D)/not_found.md $(HTML_DEPS) | doc/
	@$(call E, rustdoc: $@)
	$(Q)$(RUSTDOC) $(RUSTDOC_HTML_OPTS_NO_CSS) \
		--markdown-css http://doc.rust-lang.org/rust.css $<

define DEF_DOC

# HTML (rustdoc)
DOC_TARGETS += doc/$(1).html
doc/$(1).html: $$(D)/$(1).md $$(HTML_DEPS) $$(RUSTDOC_DEPS_$(1)) | doc/
	@$$(call E, rustdoc: $$@)
	$$(Q)$$(RUSTDOC) $$(RUSTDOC_HTML_OPTS) $$(RUSTDOC_FLAGS_$(1)) $$<

ifneq ($(ONLY_HTML_DOCS),1)

# EPUB (pandoc directly)
DOC_TARGETS += doc/$(1).epub
doc/$(1).epub: $$(D)/$(1).md | doc/
	@$$(call E, pandoc: $$@)
	$$(CFG_PANDOC) $$(PANDOC_EPUB_OPTS) $$< --output=$$@

# PDF (md =(pandoc)=> tex =(pdflatex)=> pdf)
DOC_TARGETS += doc/$(1).tex
doc/$(1).tex: $$(D)/$(1).md doc/uptack.tex doc/footer.tex doc/version.tex | doc/
	@$$(call E, pandoc: $$@)
	$$(CFG_PANDOC) $$(PANDOC_TEX_OPTS) $$< --output=$$@

ifneq ($(NO_PDF_DOCS),1)
ifeq ($$(SHOULD_BUILD_PDF_DOC_$(1)),1)
DOC_TARGETS += doc/$(1).pdf
ifneq ($(XELATEX),1)
doc/$(1).pdf: doc/$(1).tex
	@$$(call E, latex compiler: $$@)
	$$(Q)$$(CFG_LATEX) \
	-interaction=batchmode \
	-output-directory=doc \
	$$<
else
# The version of xelatex on the snap bots seemingly ingores -output-directory
# So we'll output to . and move to the doc directory manually.
# This will leave some intermediate files in the build directory.
doc/$(1).pdf: doc/$(1).tex
	@$$(call E, latex compiler: $$@)
	$$(Q)$$(CFG_LATEX) \
	-interaction=batchmode \
	-output-directory=. \
	$$<
	$$(Q)mv ./$(1).pdf $$@
endif # XELATEX
endif # SHOULD_BUILD_PDF_DOCS_$(1)
endif # NO_PDF_DOCS

endif # ONLY_HTML_DOCS

endef

$(foreach docname,$(DOCS),$(eval $(call DEF_DOC,$(docname))))


######################################################################
# Rustdoc (libstd/extra)
######################################################################


# The library documenting macro
#
# $(1) - The crate name (std/extra)
#
# Passes --cfg stage2 to rustdoc because it uses the stage2 librustc.
define DEF_LIB_DOC

# If NO_REBUILD is set then break the dependencies on rustdoc so we
# build crate documentation without having to rebuild rustdoc.
ifeq ($(NO_REBUILD),)
LIB_DOC_DEP_$(1) = \
	$$(CRATEFILE_$(1)) \
	$$(RSINPUTS_$(1)) \
	$$(RUSTDOC_EXE) \
	$$(foreach dep,$$(RUST_DEPS_$(1)), \
		$$(TLIB2_T_$(CFG_BUILD)_H_$(CFG_BUILD))/stamp.$$(dep)) \
	$$(foreach dep,$$(filter $$(DOC_CRATES), $$(RUST_DEPS_$(1))), \
		doc/$$(dep)/)
else
LIB_DOC_DEP_$(1) = $$(CRATEFILE_$(1)) $$(RSINPUTS_$(1))
endif

doc/$(1)/:
	$$(Q)mkdir -p $$@

$(2) += doc/$(1)/index.html
doc/$(1)/index.html: CFG_COMPILER_HOST_TRIPLE = $(CFG_TARGET)
doc/$(1)/index.html: $$(LIB_DOC_DEP_$(1)) doc/$(1)/
	@$$(call E, rustdoc: $$@)
	$$(Q)CFG_LLVM_LINKAGE_FILE=$$(LLVM_LINKAGE_PATH_$(CFG_BUILD)) \
		$$(RUSTDOC) --cfg dox --cfg stage2 $$<
endef

$(foreach crate,$(DOC_CRATES),$(eval $(call DEF_LIB_DOC,$(crate),DOC_TARGETS)))
$(foreach crate,$(COMPILER_DOC_CRATES),$(eval $(call DEF_LIB_DOC,$(crate),COMPILER_DOC_TARGETS)))

ifdef CFG_DISABLE_DOCS
  $(info cfg: disabling doc build (CFG_DISABLE_DOCS))
  DOC_TARGETS :=
  COMPILER_DOC_TARGETS :=
endif

docs: $(DOC_TARGETS)
compiler-docs: $(COMPILER_DOC_TARGETS)

trpl: doc/book/index.html

doc/book/index.html: $(RUSTBOOK_EXE) $(wildcard $(S)/src/doc/trpl/*.md) | doc/
	@$(call E, rustbook: $@)
	$(Q)rm -rf doc/book
	$(Q)$(RUSTBOOK) build $(S)src/doc/trpl doc/book

style: doc/style/index.html

doc/style/index.html: $(RUSTBOOK_EXE) $(wildcard $(S)/src/doc/style/*.md) | doc/
	@$(call E, rustbook: $@)
	$(Q)rm -rf doc/style
	$(Q)$(RUSTBOOK) build $(S)src/doc/style doc/style
