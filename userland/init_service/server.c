#include "shared.h"

typedef struct {
    uint64_t self_tcb_cap;
    uint64_t self_vspace_cap;
    uint64_t self_cnode_cap;
} BootInfo_t;

__attribute__((noreturn, section(".text._start")))
void _start(BootInfo_t* boot_info) {
    printf("Init process started!\n");

    printf("Got initial boot info. TCB CAP: %d | vspace CAP: %d | cnode CAP: %d\n", boot_info->self_tcb_cap, boot_info->self_vspace_cap, boot_info->self_cnode_cap);

    uint64_t addr = alloc_frame();

    printf("Server: allocated frame. Addr: 0x%x\n", addr);

    vma_map(0x5000, 4096, MAP_READ | MAP_WRITE | MAP_USER);
    printf("Server: mapped page with addr: 0x5000\n");

    volatile uint64_t *ptr = (volatile uint64_t *)0x5000;
    *ptr = 0xDEADBEEF;
    printf("wrote: 0x%x\n", *ptr);

    vma_unmap(0x5000);
    printf("Server: unmapped page with addr: 0x5000\n");

    for (;;) { spin_pause(); }
}