######################################################################
# Distribution
######################################################################

PKG_NAME := rust
PKG_DIR = $(PKG_NAME)-$(CFG_RELEASE)
PKG_TAR = $(PKG_DIR).tar.gz

ifdef CFG_MAKENSIS
PKG_NSI = $(S)src/etc/pkg/rust.nsi
PKG_EXE = $(PKG_DIR)-install.exe
endif

PKG_GITMODULES := $(S)src/libuv $(S)src/llvm

PKG_FILES := \
    $(S)LICENSE.txt $(S)README.txt             \
    $(S)configure $(S)Makefile.in              \
    $(S)doc                                    \
    $(addprefix $(S)src/,                      \
      README.txt                               \
      cargo                                    \
      comp                                     \
      compiletest                              \
      etc                                      \
      fuzzer                                   \
      libcore                                  \
      libstd                                   \
      rt                                       \
      rustdoc                                  \
      rustllvm                                 \
      snapshots.txt                            \
      test)                                    \
    $(PKG_GITMODULES)                          \
    $(filter-out Makefile config.mk, $(MKFILE_DEPS))

UNROOTED_PKG_FILES := $(patsubst $(S)%,./%,$(PKG_FILES))

lic.txt: $(S)LICENSE.txt
	@$(call E, crlf: $@)
	@$(Q)perl -pe 's@\r\n|\n@\r\n@go' <$< >$@

$(PKG_EXE): all rustc-stage3 $(PKG_NSI) $(PKG_FILES) lic.txt
	@$(call E, makensis: $@)
	$(Q)makensis -NOCD -V1 "-XOutFile $@" "-XLicenseData lic.txt" $<
	$(Q)rm -f lic.txt

$(PKG_TAR): $(PKG_FILES)
	@$(call E, making dist dir)
	$(Q)rm -Rf dist
	$(Q)mkdir -p dist/$(PKG_DIR)
	$(Q)tar \
         -C $(S) \
         --exclude-vcs \
         --exclude=*~ \
         --exclude=*/llvm/test/*/*.ll \
         --exclude=*/llvm/test/*/*.td \
         --exclude=*/llvm/test/*/*.s \
         --exclude=*/llvm/test/*/*/*.ll \
         --exclude=*/llvm/test/*/*/*.td \
         --exclude=*/llvm/test/*/*/*.s \
         -c $(UNROOTED_PKG_FILES) | tar -x -C dist/$(PKG_DIR)
	$(Q)tar -czf $(PKG_TAR) -C dist $(PKG_DIR)
	$(Q)rm -Rf dist

.PHONY: dist nsis-dist distcheck

ifdef CFG_WINDOWSY

dist: $(PKG_EXE)

distcheck: dist
	@echo
	@echo -----------------------------------------------
	@echo $(PKG_EXE) ready for distribution
	@echo -----------------------------------------------

else

dist: $(PKG_TAR)

nsis-dist: $(PKG_EXE)

distcheck: $(PKG_TAR)
	$(Q)rm -Rf dist
	$(Q)mkdir -p dist
	@$(call E, unpacking $(PKG_TAR) in dist/$(PKG_DIR))
	$(Q)cd dist && tar -xzf ../$(PKG_TAR)
	@$(call E, configuring in dist/$(PKG_DIR)-build)
	$(Q)mkdir -p dist/$(PKG_DIR)-build
	$(Q)cd dist/$(PKG_DIR)-build && ../$(PKG_DIR)/configure
	@$(call E, making 'check' in dist/$(PKG_DIR)-build)
	$(Q)+make -C dist/$(PKG_DIR)-build check
	@$(call E, making 'clean' in dist/$(PKG_DIR)-build)
	$(Q)+make -C dist/$(PKG_DIR)-build clean
	$(Q)rm -Rf dist
	@echo
	@echo -----------------------------------------------
	@echo $(PKG_TAR) ready for distribution
	@echo -----------------------------------------------

endif
