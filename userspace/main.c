#include <stddef.h>
#include <stdint.h>

long syscall3(long number, long arg1, long arg2, long arg3) {
    long ret;
    asm volatile (
        "mov %[num], %%rax\n\t"
        "mov %[a1], %%rdi\n\t"
        "mov %[a2], %%rsi\n\t"
        "mov %[a3], %%rdx\n\t"
        "int $0x80\n\t"
        "mov %%rax, %[ret]"
        : [ret]"=r"(ret)
        : [num]"r"(number), [a1]"r"(arg1), [a2]"r"(arg2), [a3]"r"(arg3)
        : "rax", "rdi", "rsi", "rdx", "rcx", "r11", "memory"
    );
    return ret;
}

void sys_exit(int code) {
    asm volatile (
        "mov $60, %%rax\n\t"   
        "mov %0, %%rdi\n\t"
        "int $0x80"
        :
        : "r"((long)code)
        : "rax", "rdi", "rcx", "r11", "memory"
    );
    __builtin_unreachable();
}

long sys_write(int fd, const char *buf, size_t len) {
    return syscall3(1, fd, (long)buf, len); 
}

void _start(void) {
    const char msg[] = "Hello world from usermode!!!!\n";
    sys_write(1, msg, sizeof(msg) - 1);
    sys_exit(0);
}