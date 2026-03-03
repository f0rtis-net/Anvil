static inline long syscall0(long nr) {
    long ret;
    asm volatile (
        "int $0x80"
        : "=a"(ret)
        : "a"(nr)
        : "memory"
    );
    return ret;
}

static inline long new_syscall0(long nr) {
    long ret;
    asm volatile (
        "syscall"
        : "=a"(ret)
        : "a"(nr)
        : "rcx", "r11", "memory" 
    );
    return ret;
}

__attribute__((noreturn))
void _start(void) {
    new_syscall0(0x60);
    new_syscall0(0x61);
    new_syscall0(0x62);
    new_syscall0(0x63);
    new_syscall0(0x64);
    new_syscall0(0x65);

    for (;;)
        asm volatile ("pause");
}