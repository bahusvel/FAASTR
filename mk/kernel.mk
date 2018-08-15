FEATURES=graphical_debug acpi

build/libkernel.a: mk/kernel.mk kernel/Cargo.toml kernel/src/* kernel/src/*/* kernel/src/*/*/* kernel/src/*/*/*/*
	cd kernel && xargo rustc --lib --features "$(FEATURES)" --target $(KTARGET) --release -- -C soft-float -C debuginfo=2 --emit link=../$@
# Temporary fix for https://github.com/redox-os/redox/issues/963 allowing to build on macOS
# ifeq ($(UNAME),Darwin)
# 	cd kernel && CC=$(ARCH)-elf-gcc AR=$(ARCH)-elf-ar CFLAGS=-ffreestanding INITFS_FOLDER=$(ROOT)/build/initfs xargo rustc --lib --target $(KTARGET) --release -- -C soft-float -C debuginfo=2 --emit link=../$@
# else
# 	cd kernel && INITFS_FOLDER=$(ROOT)/build/initfs xargo rustc --lib --target $(KTARGET) --release -- -C soft-float -C debuginfo=2 --emit link=../$@
# endif

build/libkernel_live.a: mk/kernel.mk kernel/Cargo.toml kernel/src/* kernel/src/*/* kernel/src/*/*/* kernel/src/*/*/*/* build/initfs
	cd kernel && INITFS_FOLDER=$(ROOT)/build/initfs xargo rustc --lib --features "$(FEATURES)" --target $(KTARGET) --release -- -C soft-float -C debuginfo=2 --emit link=../$@

build/kernel: kernel/linkers/$(ARCH).ld build/libkernel.a
	$(LD) --gc-sections -z max-page-size=0x1000 -T $< -o $@ build/libkernel.a
	objcopy --only-keep-debug $@ $@.sym
	objcopy --strip-debug $@

build/kernel_live: kernel/linkers/$(ARCH).ld build/libkernel_live.a #build/live.o
	$(LD) --gc-sections -z max-page-size=0x1000 -T $< -o $@ build/libkernel_live.a #build/live.o
	objcopy --only-keep-debug $@ $@.sym
	objcopy --strip-debug $@

# build/live.o: build/filesystem.bin
# 	#TODO: More general use of $(ARCH)
# 	objcopy -I binary -O elf64-x86-64 -B i386:x86-64 $< $@ \
# 		--redefine-sym _binary_build_filesystem_bin_start=__live_start \
# 		--redefine-sym _binary_build_filesystem_bin_end=__live_end \
# 		--redefine-sym _binary_build_filesystem_bin_size=__live_size
