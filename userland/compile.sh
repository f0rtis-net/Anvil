gcc \
  -ffreestanding \
  -fno-pie -no-pie \
  -nostdlib \
  -nostartfiles \
  -nodefaultlibs \
  -fno-stack-protector \
  -O2 \
  -Wl,-e,_start \
  -Wl,-Ttext=0x400000 \
  -o user.elf \
  test.c