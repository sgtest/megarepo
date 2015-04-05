# These deliberately use `=` and not `:=` so that client makefiles can
# augment HOST_RPATH_DIR / TARGET_RPATH_DIR.
HOST_RPATH_ENV = \
    $(LD_LIB_PATH_ENVVAR)="$(TMPDIR):$(HOST_RPATH_DIR):$($(LD_LIB_PATH_ENVVAR))"
TARGET_RPATH_ENV = \
    $(LD_LIB_PATH_ENVVAR)="$(TMPDIR):$(TARGET_RPATH_DIR):$($(LD_LIB_PATH_ENVVAR))"

BARE_RUSTC := $(HOST_RPATH_ENV) $(RUSTC)
RUSTC := $(BARE_RUSTC) --out-dir $(TMPDIR) -L $(TMPDIR)
CC := $(CC) -L $(TMPDIR)
HTMLDOCCK := $(PYTHON) $(S)/src/etc/htmldocck.py

# This is the name of the binary we will generate and run; use this
# e.g. for `$(CC) -o $(RUN_BINFILE)`.
RUN_BINFILE = $(TMPDIR)/$(1)

# RUN and FAIL are basic way we will invoke the generated binary.  On
# non-windows platforms, they set the LD_LIBRARY_PATH environment
# variable before running the binary.

RLIB_GLOB = lib$(1)*.rlib
STATICLIB = $(TMPDIR)/lib$(1).a
STATICLIB_GLOB = lib$(1)*.a
BIN = $(1)

UNAME = $(shell uname)
ifneq (,$(findstring MINGW,$(UNAME)))
IS_WINDOWS=1
endif

ifeq ($(UNAME),Darwin)
RUN = $(TARGET_RPATH_ENV) $(RUN_BINFILE)
FAIL = $(TARGET_RPATH_ENV) $(RUN_BINFILE) && exit 1 || exit 0
DYLIB_GLOB = lib$(1)*.dylib
DYLIB = $(TMPDIR)/lib$(1).dylib
RPATH_LINK_SEARCH =
else
ifdef IS_WINDOWS
RUN = PATH="$(PATH):$(TARGET_RPATH_DIR)" $(RUN_BINFILE)
FAIL = PATH="$(PATH):$(TARGET_RPATH_DIR)" $(RUN_BINFILE) && exit 1 || exit 0
DYLIB_GLOB = $(1)*.dll
DYLIB = $(TMPDIR)/$(1).dll
BIN = $(1).exe
RPATH_LINK_SEARCH =
else
RUN = $(TARGET_RPATH_ENV) $(RUN_BINFILE)
FAIL = $(TARGET_RPATH_ENV) $(RUN_BINFILE) && exit 1 || exit 0
DYLIB_GLOB = lib$(1)*.so
DYLIB = $(TMPDIR)/lib$(1).so
RPATH_LINK_SEARCH = -Wl,-rpath-link=$(1)
endif
endif

# Extra flags needed to compile a working executable with the standard library
ifdef IS_WINDOWS
	EXTRACFLAGS := -lws2_32 -luserenv
else
ifeq ($(UNAME),Darwin)
else
ifeq ($(UNAME),FreeBSD)
	EXTRACFLAGS := -lm -lpthread -lgcc_s
else
ifeq ($(UNAME),Bitrig)
	EXTRACFLAGS := -lm -lpthread
	EXTRACXXFLAGS := -lc++ -lc++abi
else
ifeq ($(UNAME),OpenBSD)
	EXTRACFLAGS := -lm -lpthread
else
	EXTRACFLAGS := -lm -lrt -ldl -lpthread
	EXTRACXXFLAGS := -lstdc++
endif
endif
endif
endif
endif

REMOVE_DYLIBS     = rm $(TMPDIR)/$(call DYLIB_GLOB,$(1))
REMOVE_RLIBS      = rm $(TMPDIR)/$(call RLIB_GLOB,$(1))

%.a: %.o
	ar crus $@ $<
%.dylib: %.o
	$(CC) -dynamiclib -Wl,-dylib -o $@ $<
%.so: %.o
	$(CC) -o $@ $< -shared
%.dll: lib%.o
	$(CC) -o $@ $< -shared

$(TMPDIR)/lib%.o: %.c
	$(CC) -c -o $@ $<
