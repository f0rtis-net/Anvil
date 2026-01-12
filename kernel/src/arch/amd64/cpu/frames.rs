use core::fmt;

#[repr(C)]
pub struct InterruptFrame {
    pub ds: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,

    pub rbp: u64,

    pub rdi: u64,
    pub rsi: u64,

    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,

    pub interrupt: u64,
    pub error: u64,

    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl fmt::Display for InterruptFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "=== Interrupt Frame =================================="
        )?;

        writeln!(
            f,
            "INT {}  ERR {:#018x}",
            self.interrupt,
            self.error,
        )?;

        writeln!(
            f,
            "RIP {:#018x}  RSP {:#018x}  RFLAGS {:#018x}",
            self.rip,
            self.rsp,
            self.rflags,
        )?;

        writeln!(
            f,
            "CS  {:#06x}        SS  {:#06x}        DS  {:#06x}",
            self.cs,
            self.ss,
            self.ds,
        )?;

        writeln!(f)?;

        writeln!(
            f,
            "RAX {:#018x}  RBX {:#018x}  RCX {:#018x}  RDX {:#018x}",
            self.rax,
            self.rbx,
            self.rcx,
            self.rdx,
        )?;

        writeln!(
            f,
            "RSI {:#018x}  RDI {:#018x}  RBP {:#018x}",
            self.rsi,
            self.rdi,
            self.rbp,
        )?;

        writeln!(
            f,
            "R8  {:#018x}  R9  {:#018x}  R10 {:#018x}  R11 {:#018x}",
            self.r8,
            self.r9,
            self.r10,
            self.r11,
        )?;

        writeln!(
            f,
            "R12 {:#018x}  R13 {:#018x}  R14 {:#018x}  R15 {:#018x}",
            self.r12,
            self.r13,
            self.r14,
            self.r15,
        )?;

        writeln!(
            f,
            "======================================================"
        )
    }
}