QEMU=SDL_VIDEO_X11_DGAMOUSE=0 qemu-system-$(ARCH)
QEMUFLAGS=-serial mon:stdio -d cpu_reset -d guest_errors -d int
QEMUFLAGS+=-smp 4 -m 2048
net=no
vga=no
IVSHMEM=yes
IVSHMEM_SIZE=$(shell echo $$(( 1024 * 1024 )) )
ifeq ($(iommu),yes)
	QEMUFLAGS+=-machine q35,iommu=on
else
	QEMUFLAGS+=-machine q35
endif
ifeq ($(net),no)
	QEMUFLAGS+=-net none
else
	ifneq ($(bridge),)
		QEMUFLAGS+=-netdev bridge,br=$(bridge),id=net0 -device e1000,netdev=net0,id=nic0
	else
		QEMUFLAGS+=-net nic,model=e1000 -net user -net dump,file=build/network.pcap
	endif
	ifeq ($(net),redir)
		QEMUFLAGS+=-redir tcp:8023::8023 -redir tcp:8080::8080
	endif
endif
ifeq ($(vga),no)
	QEMUFLAGS+=-nographic -vga none
endif
ifeq ($(gdb),yes)
	QEMUFLAGS+=-S -gdb tcp::9000
endif
ifeq ($(UNAME),Linux)
	ifneq ($(kvm),no)
		QEMUFLAGS+=-enable-kvm -cpu host
	endif
endif
ifeq ($(IVSHMEM), yes)
	QEMUFLAGS+= -chardev socket,path=/tmp/ivshmem_socket,id=ivshmem_socket
	QEMUFLAGS+= -device ivshmem-doorbell,chardev=ivshmem_socket,vectors=1
endif
#,int,pcall
#-device intel-iommu

# build/extra.qcow2:
# 	qemu-img create -f qcow2 $@ 1G

qemu: build/harddrive.bin # build/extra.qcow2
	$(QEMU) $(QEMUFLAGS) \
		-drive file=build/harddrive.bin,format=raw \
		# -drive file=build/extra.qcow2

qemu_no_build: # build/extra.qcow2
	$(QEMU) $(QEMUFLAGS) \
		-drive file=build/harddrive.bin,format=raw \
		# -drive file=build/extra.qcow2

qemu_iso: build/livedisk.iso # build/extra.qcow2
	$(QEMU) $(QEMUFLAGS) \
		-boot d -cdrom build/livedisk.iso \
		# -drive file=build/extra.qcow2

qemu_iso_no_build: # build/extra.qcow2
	$(QEMU) $(QEMUFLAGS) \
		-boot d -cdrom build/livedisk.iso \
		# -drive file=build/extra.qcow2

debug_qemu:
	gdb -ex 'symbol-file build/kernel_live.sym' -ex 'target remote localhost:9000'
