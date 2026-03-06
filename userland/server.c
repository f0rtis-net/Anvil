// server.c
#include "shared.h"

static void server_log(const char* msg) {
    (void)msg;
}

__attribute__((noreturn))
void _start(void) {
    long ep_id = ipc_ep_create(); /* сервер стартует первым => ep_id = 1 */
    print_str("Server: created ep_endpoint. Ep num: ");
    print_num(ep_id);
    print_str("\n");

    for (;;) {
        ipc_msg_t msg;

        /* Блокируемся до прихода сообщения */
        if (ipc_recv_msg((uint64_t)ep_id, &msg) != 0) {
            print_str("Server: recv error\n");
            continue;
        }

        uint64_t seq       = msg.data[0]; /* i */
        uint64_t client_ep = msg.data[1]; /* ep клиента */

        print_str("Server: got message seq=");
        print_num(seq);
        print_str(" from client_ep=");
        print_num(client_ep);
        print_str("\n");

        /* IPC_REPLY не реализован в ядре — используем обычный SEND */
        uint64_t ret = ipc_send(client_ep, seq, 0, 0, 0);
        if (ret == 0) {
            print_str("Server: reply sent\n");
        } else {
            print_str("Server: reply failed\n");
        }
    }
}