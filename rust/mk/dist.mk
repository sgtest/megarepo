######################################################################
# Distribution
######################################################################

PKG_NAME := rust
PKG_VER  = $(shell date +"%Y-%m-%d")-snap
PKG_DIR = $(PKG_NAME)-$(PKG_VER)
PKG_TAR = $(PKG_DIR).tar.gz

PKG_3RDPARTY := rt/valgrind.h rt/memcheck.h \
                rt/isaac/rand.h rt/isaac/standard.h \
                rt/uthash/uthash.h rt/uthash/utlist.h \
                rt/bigint/bigint.h rt/bigint/bigint_int.cpp \
                rt/bigint/bigint_ext.cpp rt/bigint/low_primes.h

PKG_FILES = \
    $(wildcard $(S)src/etc/*.*)                \
    $(S)LICENSE.txt $(S)README                 \
    $(S)configure $(S)Makefile.in              \
    $(addprefix $(S)src/,                      \
      README comp/README                       \
      $(RUNTIME_CS) $(RUNTIME_HDR)             \
      $(RUSTLLVM_LIB_CS) $(RUSTLLVM_OBJS_CS)   \
      $(RUSTLLVM_HDR)                          \
      $(PKG_3RDPARTY))                         \
    $(GENERATED)                               \
    $(COMPILER_INPUTS)                         \
    $(STDLIB_INPUTS)                           \
    $(ALL_TEST_INPUTS)                         \
    $(GENERATED)

dist: $(PKG_TAR)

$(PKG_TAR): $(GENERATED)
	@$(call E, making dist dir)
	$(Q)rm -Rf dist
	$(Q)mkdir -p dist/$(PKG_DIR)
	$(Q)tar -c $(PKG_FILES) | tar -x -C dist/$(PKG_DIR)
	$(Q)tar -czf $(PKG_TAR) -C dist $(PKG_DIR)
	$(Q)rm -Rf dist

distcheck: $(PKG_TAR)
	$(Q)rm -Rf dist
	$(Q)mkdir -p dist
	@$(call E, unpacking $(PKG_TAR) in dist/$(PKG_DIR))
	$(Q)cd dist && tar -xzf ../$(PKG_TAR)
	@$(call E, configuring in dist/$(PKG_DIR)-build)
	$(Q)mkdir -p dist/$(PKG_DIR)-build
	$(Q)cd dist/$(PKG_DIR)-build && ../$(PKG_DIR)/configure
	@$(call E, making 'check' in dist/$(PKG_DIR)-build)
	$(Q)make -C dist/$(PKG_DIR)-build check
	@$(call E, making 'clean' in dist/$(PKG_DIR)-build)
	$(Q)make -C dist/$(PKG_DIR)-build clean
	$(Q)rm -Rf dist
	@echo
	@echo -----------------------------------------------
	@echo $(PKG_TAR) ready for distribution
	@echo -----------------------------------------------


