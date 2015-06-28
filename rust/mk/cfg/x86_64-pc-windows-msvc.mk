# x86_64-pc-windows-msvc configuration
CC_x86_64-pc-windows-msvc="$(CFG_MSVC_CL_x86_64)" -nologo
LINK_x86_64-pc-windows-msvc="$(CFG_MSVC_LINK_x86_64)" -nologo
CXX_x86_64-pc-windows-msvc="$(CFG_MSVC_CL_x86_64)" -nologo
CPP_x86_64-pc-windows-msvc="$(CFG_MSVC_CL_x86_64)" -nologo
AR_x86_64-pc-windows-msvc="$(CFG_MSVC_LIB_x86_64)" -nologo
CFG_LIB_NAME_x86_64-pc-windows-msvc=$(1).dll
CFG_STATIC_LIB_NAME_x86_64-pc-windows-msvc=$(1).lib
CFG_LIB_GLOB_x86_64-pc-windows-msvc=$(1)-*.{dll,lib}
CFG_LIB_DSYM_GLOB_x86_64-pc-windows-msvc=$(1)-*.dylib.dSYM
CFG_JEMALLOC_CFLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_CFLAGS_x86_64-pc-windows-msvc := -MD
CFG_GCCISH_CXXFLAGS_x86_64-pc-windows-msvc := -MD
CFG_GCCISH_LINK_FLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_DEF_FLAG_x86_64-pc-windows-msvc :=
CFG_LLC_FLAGS_x86_64-pc-windows-msvc :=
CFG_INSTALL_NAME_x86_64-pc-windows-msvc =
CFG_EXE_SUFFIX_x86_64-pc-windows-msvc := .exe
CFG_WINDOWSY_x86_64-pc-windows-msvc := 1
CFG_UNIXY_x86_64-pc-windows-msvc :=
CFG_LDPATH_x86_64-pc-windows-msvc :=
CFG_RUN_x86_64-pc-windows-msvc=$(2)
CFG_RUN_TARG_x86_64-pc-windows-msvc=$(call CFG_RUN_x86_64-pc-windows-msvc,,$(2))
CFG_GNU_TRIPLE_x86_64-pc-windows-msvc := x86_64-pc-win32

# All windows nightiles are currently a GNU triple, so this MSVC triple is not
# bootstrapping from itself. This is relevant during stage0, and other parts of
# the build system take this into account.
BOOTSTRAP_FROM_x86_64-pc-windows-msvc := x86_64-pc-windows-gnu
