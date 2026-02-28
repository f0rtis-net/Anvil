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
    syscall0(10);
    new_syscall0(10);
    new_syscall0(10);

    for (;;)
        asm volatile ("pause");
}