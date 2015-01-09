# x86_64-apple-ios configuration
CFG_SDK_NAME_x86_64-apple-ios := iphonesimulator
CFG_SDK_ARCHS_x86_64-apple-ios := x86_64
ifneq ($(findstring darwin,$(CFG_OSTYPE)),)
CFG_IOSSIM_SDK_x86_64-apple-ios := $(shell xcrun --show-sdk-path -sdk iphonesimulator 2>/dev/null)
CFG_IOSSIM_FLAGS_x86_64-apple-ios := -m64 -target x86_64-apple-ios -isysroot $(CFG_IOSSIM_SDK_x86_64-apple-ios) -mios-simulator-version-min=7.0
CC_x86_64-apple-ios = $(shell xcrun -find -sdk iphonesimulator clang)
CXX_x86_64-apple-ios = $(shell xcrun -find -sdk iphonesimulator clang++)
CPP_x86_64-apple-ios = $(shell xcrun -find -sdk iphonesimulator clang++)
AR_x86_64-apple-ios = $(shell xcrun -find -sdk iphonesimulator ar)
endif
CFG_LIB_NAME_x86_64-apple-ios = lib$(1).a
CFG_LIB_GLOB_x86_64-apple-ios = lib$(1)-*.a
CFG_LIB_SKIP_INSTALL_x86_64-apple-ios = 1 #lib$(1)-*.a
CFG_STATIC_LIB_NAME_x86_64-apple-ios=lib$(1).a
CFG_LIB_DSYM_GLOB_x86_64-apple-ios = lib$(1)-*.a.dSYM
CFG_CFLAGS_x86_64-apple-ios := $(CFG_IOSSIM_FLAGS_x86_64-apple-ios)
CFG_JEMALLOC_CFLAGS_x86_64-apple-ios := $(CFG_IOSSIM_FLAGS_x86_64-apple-ios)
CFG_GCCISH_CFLAGS_x86_64-apple-ios := -Wall -Werror -fPIC $(CFG_IOSSIM_FLAGS_x86_64-apple-ios)
CFG_GCCISH_CXXFLAGS_x86_64-apple-ios := -fno-rtti $(CFG_IOSSIM_FLAGS_x86_64-apple-ios) -I$(CFG_IOSSIM_SDK_x86_64-apple-ios)/usr/include/c++/4.2.1
CFG_GCCISH_LINK_FLAGS_x86_64-apple-ios := -lpthread -Wl,-no_compact_unwind -m64 -Wl,-syslibroot $(CFG_IOSSIM_SDK_x86_64-apple-ios)
CFG_GCCISH_DEF_FLAG_x86_64-apple-ios := -Wl,-exported_symbols_list,
CFG_GCCISH_PRE_LIB_FLAGS_x86_64-apple-ios :=
CFG_GCCISH_POST_LIB_FLAGS_x86_64-apple-ios :=
CFG_DEF_SUFFIX_x86_64-apple-ios := .darwin.def
CFG_LLC_FLAGS_x86_64-apple-ios :=
CFG_INSTALL_NAME_x86_64-apple-ios = -Wl,-install_name,@rpath/$(1)
CFG_LIBUV_LINK_FLAGS_x86_64-apple-ios :=
CFG_EXE_SUFFIX_x86_64-apple-ios :=
CFG_WINDOWSY_x86_64-apple-ios :=
CFG_UNIXY_x86_64-apple-ios := 1
CFG_PATH_MUNGE_x86_64-apple-ios := true
CFG_LDPATH_x86_64-apple-ios :=
CFG_RUN_x86_64-apple-ios = $(2)
CFG_RUN_TARG_x86_64-apple-ios = $(call CFG_RUN_x86_64-apple-ios,,$(2))
CFG_GNU_TRIPLE_i386-apple-ios := x86_64-apple-ios
