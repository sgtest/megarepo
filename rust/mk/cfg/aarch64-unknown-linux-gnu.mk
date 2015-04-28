# aarch64-unknown-linux-gnu configuration
CROSS_PREFIX_aarch64-unknown-linux-gnu=aarch64-linux-gnu-
CC_aarch64-unknown-linux-gnu=gcc
CXX_aarch64-unknown-linux-gnu=g++
CPP_aarch64-unknown-linux-gnu=gcc -E
AR_aarch64-unknown-linux-gnu=ar
CFG_LIB_NAME_aarch64-unknown-linux-gnu=lib$(1).so
CFG_STATIC_LIB_NAME_aarch64-unknown-linux-gnu=lib$(1).a
CFG_LIB_GLOB_aarch64-unknown-linux-gnu=lib$(1)-*.so
CFG_LIB_DSYM_GLOB_aarch64-unknown-linux-gnu=lib$(1)-*.dylib.dSYM
CFG_JEMALLOC_CFLAGS_aarch64-unknown-linux-gnu := -D__aarch64__ $(CFLAGS)
CFG_GCCISH_CFLAGS_aarch64-unknown-linux-gnu := -Wall -g -fPIC -D__aarch64__ $(CFLAGS)
CFG_GCCISH_CXXFLAGS_aarch64-unknown-linux-gnu := -fno-rtti $(CXXFLAGS)
CFG_GCCISH_LINK_FLAGS_aarch64-unknown-linux-gnu := -shared -fPIC -g
CFG_GCCISH_DEF_FLAG_aarch64-unknown-linux-gnu := -Wl,--export-dynamic,--dynamic-list=
CFG_LLC_FLAGS_aarch64-unknown-linux-gnu :=
CFG_INSTALL_NAME_aarch64-unknown-linux-gnu =
CFG_EXE_SUFFIX_aarch64-unknown-linux-gnu :=
CFG_WINDOWSY_aarch64-unknown-linux-gnu :=
CFG_UNIXY_aarch64-unknown-linux-gnu := 1
CFG_LDPATH_aarch64-unknown-linux-gnu :=
CFG_RUN_aarch64-unknown-linux-gnu=$(2)
CFG_RUN_TARG_aarch64-unknown-linux-gnu=$(call CFG_RUN_aarch64-unknown-linux-gnu,,$(2))
RUSTC_FLAGS_aarch64-unknown-linux-gnu :=
RUSTC_CROSS_FLAGS_aarch64-unknown-linux-gnu :=
CFG_GNU_TRIPLE_aarch64-unknown-linux-gnu := aarch64-unknown-linux-gnu
