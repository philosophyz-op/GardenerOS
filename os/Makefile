TARGET := riscv64gc-unknown-none-elf
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os

run:
	qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios ../bootloader/opensbi.bin \
		-device loader,file=$(KERNEL_ELF),addr=0x80200000

clean:
	cargo clean

.PHONY: run clean
