// server.c
#include "shared.h"

static void server_log(const char* msg) {
    (void)msg;
}

__attribute__((noreturn))
void _start(void) {
    long ep_id = ipc_ep_create();
    printf("Server: created ep_endpoint. Ep num: %u\n", ep_id);

    uint64_t addr = alloc_frame();

    printf("Server: allocated frame. Addr: 0x%x\n", addr);

    vma_map(0x5000, 4096, MAP_READ | MAP_WRITE | MAP_USER);
    printf("Server: mapped page with addr: 0x5000\n");

    volatile uint64_t *ptr = (volatile uint64_t *)0x5000;
    *ptr = 0xDEADBEEF;
    printf("wrote: 0x%x\n", *ptr);

    vma_unmap(0x5000);
    printf("Server: unmapped page with addr: 0x5000\n");

    sleep(100000ULL);

    for (;;) {
        ipc_msg_t msg;

        printf("Server: waiting for recv...\n");
        if (ipc_recv_msg((uint64_t)ep_id, &msg) != 0) {
            printf("Server: recv error\n");
            continue;
        }

        uint64_t seq       = msg.data[0];
        uint64_t client_ep = 2;

        printf("Server: got message seq=%u from client_ep=%u\n", seq, client_ep);

        uint64_t ret = ipc_send(client_ep, seq, 0, 0, 0);
        if (ret == 0) {
            printf("Server: reply sent\n");
        } else {
            printf("Server: reply failed\n");
        }
    }

    printf("Server: destroying ep...\n");
    ipc_ep_destroy(ep_id);
    printf("Server: ep destroyed!\n");

    for (;;) { spin_pause(); }
}