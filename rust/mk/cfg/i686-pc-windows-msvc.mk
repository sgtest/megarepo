# i686-pc-windows-msvc configuration
CC_i686-pc-windows-msvc="$(CFG_MSVC_CL_i386)" -nologo
LINK_i686-pc-windows-msvc="$(CFG_MSVC_LINK_i386)" -nologo
CXX_i686-pc-windows-msvc="$(CFG_MSVC_CL_i386)" -nologo
CPP_i686-pc-windows-msvc="$(CFG_MSVC_CL_i386)" -nologo
AR_i686-pc-windows-msvc="$(CFG_MSVC_LIB_i386)" -nologo
CFG_LIB_NAME_i686-pc-windows-msvc=$(1).dll
CFG_STATIC_LIB_NAME_i686-pc-windows-msvc=$(1).lib
CFG_LIB_GLOB_i686-pc-windows-msvc=$(1)-*.{dll,lib}
CFG_LIB_DSYM_GLOB_i686-pc-windows-msvc=$(1)-*.dylib.dSYM
CFG_JEMALLOC_CFLAGS_i686-pc-windows-msvc :=
CFG_GCCISH_CFLAGS_i686-pc-windows-msvc := -MD
CFG_GCCISH_CXXFLAGS_i686-pc-windows-msvc := -MD
CFG_GCCISH_LINK_FLAGS_i686-pc-windows-msvc :=
CFG_GCCISH_DEF_FLAG_i686-pc-windows-msvc :=
CFG_LLC_FLAGS_i686-pc-windows-msvc :=
CFG_INSTALL_NAME_i686-pc-windows-msvc =
CFG_EXE_SUFFIX_i686-pc-windows-msvc := .exe
CFG_WINDOWSY_i686-pc-windows-msvc := 1
CFG_UNIXY_i686-pc-windows-msvc :=
CFG_LDPATH_i686-pc-windows-msvc :=
CFG_RUN_i686-pc-windows-msvc=$(2)
CFG_RUN_TARG_i686-pc-windows-msvc=$(call CFG_RUN_i686-pc-windows-msvc,,$(2))
CFG_GNU_TRIPLE_i686-pc-windows-msvc := i686-pc-win32

# All windows nightiles are currently a GNU triple, so this MSVC triple is not
# bootstrapping from itself. This is relevant during stage0, and other parts of
# the build system take this into account.
BOOTSTRAP_FROM_i686-pc-windows-msvc := i686-pc-windows-gnu
