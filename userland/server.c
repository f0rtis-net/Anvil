#include "shared.h"

static void server_log(const char* msg) {
    (void)msg;
}

__attribute__((noreturn))
void _start(void) {
    long ep_id = ipc_ep_create();
    
    print_str("Server: created ep_endpoint. Ep num: ");
    print_num(ep_id);
    print_str("\n");
    for (;;) {}
}