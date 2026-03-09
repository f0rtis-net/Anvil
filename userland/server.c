// server.c
#include "shared.h"

static void server_log(const char* msg) {
    (void)msg;
}

__attribute__((noreturn))
void _start(void) {
    long ep_id = ipc_ep_create();
    printf("Server: created ep_endpoint. Ep num: %u\n", ep_id);

    for (;;) {
        ipc_msg_t msg;

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
   
    for (;;) { spin_pause(); }
}