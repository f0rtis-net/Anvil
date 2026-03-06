// client.c
#include "shared.h"

__attribute__((noreturn))
void _start(void) {
    long client_ep = ipc_ep_create();
    print_str("Client: created ep_endpoint. Ep num: ");
    print_num(client_ep);
    print_str("\n");

    long server_ep = 1; 

    /*for (int i = 1; i <= 5; i++) {
        uint64_t result = ipc_send(
            server_ep,
            (uint64_t)1337,
            (uint64_t)client_ep,  
            (uint64_t)(i * 2),
            0xCAFEBABE
        );

        if (result == 0) {
            print_str("Client: Message ");
            print_num(i);
            print_str(" sent to server\n");
        } else {
            print_str("Client: Failed to send message ");
            print_num(i);
            print_str("\n");
        }

        ipc_msg_t reply;
        while (ipc_recv_msg(client_ep, &reply) != 0) {
            spin_pause();
        }

        print_str("Client: Got reply for message ");
        print_num(i);
        print_str("\n");
    }

    print_str("Client: All messages sent and replied\n");*/
    for (;;) { spin_pause(); }
}