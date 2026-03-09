// client.c
#include "shared.h"

__attribute__((noreturn))
void _start(void) {
    long client_ep = ipc_ep_create();
    printf("Client: created ep_endpoint. Ep num: %u\n", client_ep);

    long server_ep = 1; 

    for (int i = 1; i <= 5; i++) {
        while(1) {
            uint64_t result = ipc_send(
            server_ep,
            (uint64_t)1337,
            (uint64_t)client_ep,  
            (uint64_t)(i * 2),
            0xCAFEBABE
            );

            if (result == 0) {
                printf("Client: Message %d sent to server\n", i);
                break;
            } else if (result == 1) {
                printf("Client: Failed to send message %d. Invalid endpoint\n", i);
                break;
            } else if (result == 17) {
                printf("Client: Server is not ready, retrying...\n");
                continue; 
            }
        }

        ipc_msg_t reply;
        while (ipc_recv_msg(client_ep, &reply) != 0) {
            spin_pause();
        }

        printf("Client: Got reply for message %d\n", i);
    }

    printf("Client: All messages sent and replied\n");
    for (;;) { spin_pause(); }
}