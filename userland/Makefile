CC = gcc
CFLAGS = -nostdlib -nostdinc -ffreestanding -O2 \
         -fno-stack-protector -mno-red-zone \
         -fno-builtin -static \
         -Wl,-Ttext=0x400000 \
         -Wl,--build-id=none \
         -nodefaultlibs

all: server.elf client.elf

server.elf: server.c shared.h
	$(CC) $(CFLAGS) -o $@ $<

client.elf: client.c shared.h
	$(CC) $(CFLAGS) -o $@ $<

clean:
	rm -f *.elf
