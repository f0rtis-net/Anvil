// syscall_client.h
#pragma once
typedef unsigned char __uint8_t;
typedef unsigned short int __uint16_t;
typedef unsigned int __uint32_t;
typedef unsigned long int __uint64_t;

typedef __uint8_t uint8_t;
typedef __uint16_t uint16_t;
typedef __uint32_t uint32_t;
typedef __uint64_t uint64_t;


#define SYS_IPC_EP_CREATE  0x64
#define SYS_IPC_EP_DESTROY 0x65
#define SYS_IPC_SEND       0x60
#define SYS_IPC_RECV       0x61
#define SYS_IPC_CALL       0x62
#define SYS_IPC_REPLY      0x63
#define SYS_PRINT          0x10

typedef struct {
    uint64_t ep_id;
    uint64_t msg[4];
} ipc_syscall_args_t;


static inline void spin_pause(void) {
    asm volatile("pause");
}

static inline uint64_t syscall0(uint64_t number) {
    register uint64_t r9 asm("r9");
    __asm__ volatile (
        "syscall"
        : "=r"(r9)
        : "a"(number)
        : "rcx", "r11", "rdi", "rsi", "rdx", "r8", "r10",
          "r12", "r13", "r14", "r15", "memory"
    );
    return r9;
}

static inline uint64_t syscall1(uint64_t number, uint64_t arg1) {
    register uint64_t r9 asm("r9");
    __asm__ volatile (
        "syscall"
        : "=r"(r9)
        : "a"(number), "D"(arg1)
        : "rcx", "r11", "rsi", "rdx", "r8", "r10",
          "r12", "r13", "r14", "r15", "memory"
    );
    return r9;
}

static inline uint64_t syscall2(uint64_t number, uint64_t arg1, uint64_t arg2) {
    register uint64_t r9 asm("r9");
    __asm__ volatile (
        "syscall"
        : "=r"(r9)
        : "a"(number), "D"(arg1), "S"(arg2)
        : "rcx", "r11", "rdx", "r8", "r10",
          "r12", "r13", "r14", "r15", "memory"
    );
    return r9;
}

static inline uint64_t syscall3(uint64_t number, uint64_t arg1, uint64_t arg2, uint64_t arg3) {
    register uint64_t r9 asm("r9");
    __asm__ volatile (
        "syscall"
        : "=r"(r9)
        : "a"(number), "D"(arg1), "S"(arg2), "d"(arg3)
        : "rcx", "r11", "r8", "r10",
          "r12", "r13", "r14", "r15", "memory"
    );
    return r9;
}

static inline uint64_t syscall4(uint64_t number, uint64_t arg1, uint64_t arg2, 
                                uint64_t arg3, uint64_t arg4) {
    register uint64_t r9 asm("r9");
    register uint64_t r10 asm("r10") = arg4;
    __asm__ volatile (
        "syscall"
        : "=r"(r9)
        : "a"(number), "D"(arg1), "S"(arg2), "d"(arg3), "r"(r10)
        : "rcx", "r11", "r8",
          "r12", "r13", "r14", "r15", "memory"
    );
    return r9;
}

static inline uint64_t syscall5(uint64_t number, uint64_t arg1, uint64_t arg2,
                                uint64_t arg3, uint64_t arg4, uint64_t arg5) {
    register uint64_t r9 asm("r9");
    register uint64_t r10 asm("r10") = arg4;
    register uint64_t r8  asm("r8")  = arg5;
    __asm__ volatile (
        "syscall"
        : "=r"(r9)
        : "a"(number), "D"(arg1), "S"(arg2), "d"(arg3), "r"(r10), "r"(r8)
        : "rcx", "r11",
          "r12", "r13", "r14", "r15", "memory"
    );
    return r9;
}

typedef struct {
    uint64_t label;
    uint64_t data[4];
} ipc_msg_t;

static inline uint64_t ipc_recv_msg(uint64_t ep_id, ipc_msg_t *out) {
    register uint64_t r9  asm("r9");
    register uint64_t rdi asm("rdi") = ep_id;
    register uint64_t rsi asm("rsi");
    register uint64_t rdx asm("rdx");
    register uint64_t r10 asm("r10");
    register uint64_t r8  asm("r8");

    __asm__ volatile (
        "syscall"
        : "=r"(r9), "=r"(rsi), "=r"(rdx), "=r"(r10), "=r"(r8),
          "+r"(rdi)  /* rdi = label out */
        : "a"((uint64_t)SYS_IPC_RECV)
        : "rcx", "r11", "r12", "r13", "r14", "r15", "memory"
    );

    if (out) {
        out->label   = rdi;
        out->data[0] = rsi;
        out->data[1] = rdx;
        out->data[2] = r10;
        out->data[3] = r8;
    }
    return r9; /* 0 = ok, 1 = error */
}

static inline uint64_t ipc_ep_create(void) {
    return syscall1(SYS_IPC_EP_CREATE, 0); 
}

static inline uint64_t ipc_ep_destroy(uint64_t ep_id) {
    return syscall1(SYS_IPC_EP_DESTROY, ep_id);
}

static inline uint64_t ipc_send(uint64_t ep_id, uint64_t msg0, uint64_t msg1, 
                                uint64_t msg2, uint64_t msg3) {
    return syscall5(SYS_IPC_SEND, ep_id, msg0, msg1, msg2, msg3);
}

static inline uint64_t ipc_try_recv(uint64_t ep_id) {

    uint64_t ret = syscall1(SYS_IPC_RECV, ep_id);
    
    return ret;
}

static inline uint64_t ipc_call(uint64_t ep_id, uint64_t req0, uint64_t req1,
                                uint64_t req2, uint64_t req3,
                                uint64_t *resp0, uint64_t *resp1,
                                uint64_t *resp2, uint64_t *resp3) {
    return syscall5(SYS_IPC_CALL, ep_id, req0, req1, req2, req3);
}

static inline uint64_t ipc_reply(uint64_t ep_id, uint64_t resp0, uint64_t resp1,
                                 uint64_t resp2, uint64_t resp3) {
    return syscall5(SYS_IPC_REPLY, ep_id, resp0, resp1, resp2, resp3);
}

static inline uint64_t sys_print(const char *str, uint64_t len) {
    if ((uint64_t)str < 0x1000 || (uint64_t)str > 0x00007FFFFFFFFFFF) {
        return 1;
    }
    if (len > 4096) {
        return 1;
    }
    
    return syscall2(SYS_PRINT, (uint64_t)str, len);
}

static inline void print_str(const char *str) {
    uint64_t len = 0;
    while (str[len]) len++;
    sys_print(str, len);
}

static inline void print_num(uint64_t num) {
    char buf[32];
    int i = 30;
    buf[31] = '\0';
    
    if (num == 0) {
        print_str("0");
        return;
    }
    
    while (num > 0 && i >= 0) {
        buf[i--] = '0' + (num % 10);
        num /= 10;
    }
    
    print_str(&buf[i + 1]);
}