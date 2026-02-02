static inline long syscall0(long num) {
    long ret;
    asm volatile (
        "syscall"
        : "=a"(ret)
        : "a"(num)
        : "rcx", "r11", "memory"
    );
    return ret;
}


__attribute__((noreturn))
void _start(void) {
    asm volatile ("int3");

    asm volatile ("int $0x80");

    syscall0(0);

    for (;;)
        asm volatile ("pause");
}