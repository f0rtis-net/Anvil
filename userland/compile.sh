x86_64-elf-gcc \
  -ffreestanding \
  -fno-pie -no-pie \
  -nostdlib \
  -nostartfiles \
  -nodefaultlibs \
  -Wl,-e,_start \
  -Wl,-Ttext=0x400000 \
  -o user.elf \
  test.c