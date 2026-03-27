#include "shared.h"

typedef struct {
    uint64_t self_tcb_cap;
    uint64_t self_vspace_cap;
    uint64_t self_cnode_cap;

    uint64_t cpio_addr;
} BootInfo_t;

void kill_sleep() {
    for (;;) { spin_pause(); }
}

__attribute__((noreturn, section(".text._start")))
void _start(BootInfo_t* boot_info) {
    int ret;

    printf("Init process started!\n");

    printf("Got initial boot info. TCB CAP: %d | vspace CAP: %d | cnode CAP: %d\n", boot_info->self_tcb_cap, boot_info->self_vspace_cap, boot_info->self_cnode_cap);

    uint64_t addr = alloc_frame();

    printf("Server: allocated frame. Addr: 0x%x\n", addr);

    ret = vma_map(boot_info->self_vspace_cap, 0x5000, 4096, MAP_READ | MAP_WRITE | MAP_USER);

    if (ret != 0) {
        printf("Server: invalid capability provided for map!\n");
        kill_sleep();
    }

    printf("Server: mapped page with addr: 0x5000\n");

    volatile uint64_t *ptr = (volatile uint64_t *)0x5000;
    *ptr = 0xDEADBEEF;
    printf("wrote: 0x%x\n", *ptr);

    ret = vma_unmap(boot_info->self_vspace_cap, 0x5000);
    printf("Server: unmapped page with addr: 0x5000\n");

    if (ret != 0) {
        printf("Server: invalid capability provided for unmap!\n");
        kill_sleep();
    }

    for (;;) { spin_pause(); }
}