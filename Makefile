# Configuration and variables
include mk/config.mk

live: build/livedisk.bin

iso: build/livedisk.iso

clean:
	rm -rf build || true
	mkdir build

include mk/qemu.mk
include mk/virtualbox.mk

# Kernel recipes
include mk/kernel.mk

# Filesystem recipes
# include mk/initfs.mk
# include mk/filesystem.mk

# Disk images
include mk/disk.mk
include mk/tools.mk
include mk/tests.mk

rust-src:
	git submodule add https://github.com/rust-lang/rust rust
	cd rust && git checkout e026b59cf4f4cb9dd86510438085efafbc630e5a
	cd rust && git submodule update --init src/stdsimd

rust-src-redox:
	git submodule add https://gitlab.redox-os.org/redox-os/rust rust
	cd rust && git checkout 81c2bf4e51647295d3d92952dbb0464b460df0c3
	cd rust && git submodule update --init src/stdsimd

# An empty target
FORCE:

# A method of creating a listing for any binary
%.list: %
	objdump -C -M intel -D $< > $@

debug:
	gdb
