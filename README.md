# Anvil kernel. In hard developing...

## Architectural Postulate

This project implements a **microkernel-based operating system with a strong focus on security and strict isolation**.

The kernel and HAL are intentionally kept **minimal** and are responsible only for tasks that **cannot be safely or reasonably delegated to user space**.  
Any logic that is not fundamental to secure hardware control or execution context management is considered user-level functionality and is implemented as services.

### Core Principles

- **HAL as the Root of Trust**  
  The HAL is responsible solely for architecture initialization, isolation, and safe control transfer.  
  It contains no policy, has no knowledge of service types, and makes no semantic decisions.

- **Strict Kernel ↔ User Boundary**  
  Transitions between execution modes occur only through formal mechanisms (interrupts, exceptions, syscalls).  
  User-mode code never inherits kernel state.

- **Virtual Memory as a Security Primitive**  
  Address spaces are isolated by hardware.  
  Services cannot access kernel memory or each other unless explicitly permitted.

- **Interrupts as Events, Not Logic**  
  Interrupt handling in the kernel is minimal and free of complex logic.  
  All decisions about further execution are made centrally and formally.

- **User-Mode Services**  
  File systems, networking, console handling, drivers, and other subsystems run outside the kernel and interact with it through well-defined interfaces.

- **Minimal and Auditable Design**  
  Kernel and HAL code are designed to be:
  - small
  - deterministic
  - easy to audit
  - suitable for future formal verification

### Goal

The goal of this project is not maximum performance at any cost, but **predictability, isolation, and architectural clarity**, enabling the construction of a reliable system on top of a minimal trusted kernel.

If functionality can be safely implemented in user space, it **does not belong in the kernel**.


### syscalls
thread_exit
thread_sleep - done

tcb_configure
tcb_set_regs
tcb_resume
tcb_suspend
tcb_read_regs

invoke
recv
reply
reply_recv

cnode_copy
cnode_move
cnode_delete
cnode_revoke

frame_alloc - done
vma_map - done
vma_unmap - done
mprotect - done

notify_signal
notify_wait
notify_poll

## Credits:

[Project base](https://github.com/jasondyoungberg/limine-rust-template/tree/trunk)
[Learning base](https://wiki.osdev.org/)