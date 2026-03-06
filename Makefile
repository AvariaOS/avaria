KERNEL_NAME := kernix
ISO_DIR := iso_root
ISO_NAME := kernel.iso
INITRD := initrd.tar
KERNEL_BIN := target/x86_64-unknown-none/release/$(KERNEL_NAME)
OVMF := /usr/share/ovmf/OVMF.fd
QEMU_FLAGS := -m 256M -smp 4 -serial stdio \
	-drive file=disk.img,if=none,id=disk0,format=raw \
	-device ahci,id=ahci0 \
	-device ide-hd,drive=disk0,bus=ahci0.0

MODULES_OUT := initrd/boot/modules
USER_OUT := initrd/boot/user

.PHONY: all build iso limine initrd modules userspace clean run run-uefi

all: iso

build:
	cargo build --release

limine:
	@if [ ! -d "limine" ]; then \
		git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1; \
	fi
	@if [ ! -f "limine/limine" ]; then \
		$(MAKE) -C limine; \
	fi

modules:
	mkdir -p $(MODULES_OUT)
	cd modules/memtest && cargo build --release
	cp modules/memtest/target/x86_64-unknown-none/release/memtest $(MODULES_OUT)/memtest.ko

userspace:
	mkdir -p $(USER_OUT)
	cd modules/init && cargo build --release
	cp modules/init/target/x86_64-unknown-none/release/init $(USER_OUT)/init.elf

initrd: modules userspace
	tar cf $(INITRD) -C initrd .

iso: build limine initrd
	rm -rf $(ISO_DIR)
	mkdir -p $(ISO_DIR)/boot/limine
	mkdir -p $(ISO_DIR)/EFI/BOOT
	cp $(KERNEL_BIN) $(ISO_DIR)/boot/kernel
	cp $(INITRD) $(ISO_DIR)/boot/initrd
	cp limine.conf $(ISO_DIR)/boot/limine/
	cp limine/limine-bios.sys $(ISO_DIR)/boot/limine/
	cp limine/limine-bios-cd.bin $(ISO_DIR)/boot/limine/
	cp limine/limine-uefi-cd.bin $(ISO_DIR)/boot/limine/
	cp limine/BOOTX64.EFI $(ISO_DIR)/EFI/BOOT/
	cp limine/BOOTIA32.EFI $(ISO_DIR)/EFI/BOOT/
	xorriso -as mkisofs \
		-b boot/limine/limine-bios-cd.bin \
		-no-emul-boot \
		-boot-load-size 4 \
		-boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part \
		--efi-boot-image \
		--protective-msdos-label \
		$(ISO_DIR) -o $(ISO_NAME)
	./limine/limine bios-install $(ISO_NAME)

run: iso
	qemu-system-x86_64 -cdrom $(ISO_NAME) $(QEMU_FLAGS)

run-uefi: iso
	qemu-system-x86_64 -cdrom $(ISO_NAME) $(QEMU_FLAGS) -bios ${OVMF}

clean:
	rm -rf $(ISO_DIR) $(ISO_NAME) $(INITRD)
	cd modules/memtest && cargo clean
	cd modules/init && cargo clean
	cargo clean
	rm -rf limine
	rm -rf initrd

	@if ls *.zip *.tar *.ko *.elf *.img *.iso 1> /dev/null 2>&1; then \
		rm *.zip *.tar *.ko *.elf *.img *.iso; \
	fi