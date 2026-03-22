CC = gcc
OBJCOPY = objcopy

CFLAGS = -nostdlib -nostdinc -ffreestanding -O2 \
         -fno-stack-protector -mno-red-zone \
         -fno-builtin -static \
         -Wl,-Ttext=0x400000 \
         -Wl,--build-id=none \
         -Wl,-e,_start \
         -Wl,-T,link.ld \
         -nodefaultlibs

all: server.bin

server.elf: server.c shared.h
	$(CC) $(CFLAGS) -o $@ $<

server.bin: server.elf
	$(OBJCOPY) -O binary $< $@

clean:
	rm -f *.elf *.bin
