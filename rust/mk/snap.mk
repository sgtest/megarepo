
snap-stage1: stage1/rustc$(X) stage1/lib/$(CFG_RUNTIME) \
	stage1/$(CFG_RUSTLLVM) stage1/lib/main.o
	$(S)src/etc/make-snapshot.py stage1

snap-stage2: stage2/rustc$(X) stage2/lib/$(CFG_STDLIB) \
	stage2/lib/libstd.rlib stage2/lib/$(CFG_RUNTIME) \
	stage2/$(CFG_RUSTLLVM) stage2/lib/main.o
	$(S)src/etc/make-snapshot.py stage2

snap-stage3: stage3/rustc$(X) stage3/lib/$(CFG_STDLIB) \
	stage3/lib/libstd.rlib stage3/lib/$(CFG_RUNTIME) \
	stage3/$(CFG_RUSTLLVM) stage3/lib/main.o
	$(S)src/etc/make-snapshot.py stage3

