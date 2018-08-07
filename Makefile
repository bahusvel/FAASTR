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

# An empty target
FORCE:

# A method of creating a listing for any binary
%.list: %
	objdump -C -M intel -D $< > $@
