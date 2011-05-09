######################################################################
# rustc LLVM-extensions (C++) library variables and rules
######################################################################

RUSTLLVM_LIB_CS := $(addprefix rustllvm/, \
                     MachOObjectFile.cpp Passes.cpp Passes2.cpp)

RUSTLLVM_OBJS_CS := $(addprefix rustllvm/, RustWrapper.cpp)

RUSTLLVM_HDR := rustllvm/include/llvm-c/Object.h
RUSTLLVM_DEF := rustllvm/rustllvm$(CFG_DEF_SUFFIX)

RUSTLLVM_INCS := -iquote $(CFG_LLVM_INCDIR) \
                 -iquote $(S)src/rustllvm/include
RUSTLLVM_LIB_OBJS := $(RUSTLLVM_LIB_CS:.cpp=.o)
RUSTLLVM_OBJS_OBJS := $(RUSTLLVM_OBJS_CS:.cpp=.o)


# FIXME: Building a .a is a hack so that we build with both older and newer
# versions of LLVM. In newer versions some of the bits of this library are
# already in LLVM itself, so they are skipped.
rustllvm/rustllvmbits.a: $(RUSTLLVM_LIB_OBJS)
	rm -f $@
	ar crs $@ $^

# Note: We pass $(CFG_LLVM_LIBS) twice to fix the windows link since
# it has no -whole-archive.
rustllvm/$(CFG_RUSTLLVM): rustllvm/rustllvmbits.a $(RUSTLLVM_OBJS_OBJS) \
                          $(MKFILES) $(RUSTLLVM_HDR) $(RUSTLLVM_DEF)
	@$(call E, link: $@)
	$(Q)$(call CFG_LINK_C,$@,$(RUSTLLVM_OBJS_OBJS) \
	  $(CFG_GCCISH_PRE_LIB_FLAGS) $(CFG_LLVM_LIBS) \
          $(CFG_GCCISH_POST_LIB_FLAGS) rustllvm/rustllvmbits.a \
	  $(CFG_LLVM_LIBS) \
          $(CFG_LLVM_LDFLAGS),$(RUSTLLVM_DEF))


rustllvm/%.o: rustllvm/%.cpp $(MKFILES)
	@$(call E, compile: $@)
	$(Q)$(call CFG_COMPILE_C, $@, $(CFG_LLVM_CXXFLAGS) $(RUSTLLVM_INCS)) $<

