QEMU=SDL_VIDEO_X11_DGAMOUSE=0 qemu-system-$(ARCH)
QEMUFLAGS=-serial mon:stdio -d cpu_reset -d guest_errors
QEMUFLAGS+=-smp 4 -m 2048
net=no
vga=no
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
ifneq ($(gdb),yes)
	QEMUFLAGS+=-s
endif
ifeq ($(UNAME),Linux)
	ifneq ($(kvm),no)
		QEMUFLAGS+=-enable-kvm -cpu host
	endif
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
