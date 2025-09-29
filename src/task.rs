use core::{fmt, ops::{Add, Sub}};

use x86_64::{structures::{paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB}}, VirtAddr};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Pid(i32);

impl Pid {
    pub const fn new(value: i32) -> Self {
        Pid(value)
    }

    pub const fn as_raw(self) -> i32 {
        self.0
    }

    pub fn is_init(self) -> bool {
        self.0 == 1
    }

    pub fn is_valid(self) -> bool {
        self.0 > 0
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PID({})", self.0)
    }
}

impl Add for Pid {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Pid(self.0 + rhs.0)
    }
}

impl Sub for Pid {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Pid(self.0 - rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Starting,
    Zombie
}   

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskPriority{
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Copy)]
pub struct Context {
    pub rbp: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub struct Task {
    pub pid: usize,
    pub ctx: Context,
    pub state: TaskState,
    pub priority: TaskPriority,
    pub quant: usize,
    pub exit_code: i64,
    pub ticks_left: usize,
}

pub struct TaskManager {
    curr_pid: usize
}

impl TaskManager {
    pub fn new() -> Self {
        Self { 
            curr_pid: 0
        }
    }

    fn allocate_pages(
        &self,
        start_addr: u64,
        pages_ammo: usize,
        mapper: &mut impl Mapper<Size4KiB>,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> u64 {
        assert!(pages_ammo >= 1, "Need 1 & more pages for map");

        let first_page: Page<Size4KiB> =
            Page::containing_address(VirtAddr::new(start_addr));

        let first_stack_page = first_page;
        let last_stack_page = first_page + pages_ammo as u64;

        for page in Page::range_inclusive(first_stack_page, last_stack_page) {
            let frame = frame_allocator
                .allocate_frame()
                .expect("Memory allocation limit");
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            unsafe { mapper.map_to(page, frame, flags, frame_allocator).unwrap().flush() };
        }

        let rsp = last_stack_page.start_address().as_u64();

        rsp
    }

    fn fill_main_registers(&self, ctx: &mut Context) {
        ctx.r15 = 0;
        ctx.r14 = 0;
        ctx.r13 = 0;
        ctx.r12 = 0;
        ctx.r11 = 0;
        ctx.r10 = 0;
        ctx.r9  = 0;
        ctx.r8  = 0;
        ctx.rsi = 0;
        ctx.rdi = 0;
        ctx.rbp = 0;
        ctx.rbx = 0;
        ctx.rdx = 0;
        ctx.rcx = 0;
        ctx.rax = 0;

        //set flag to ready for interrupting
        ctx.rflags = 1 << 9;  
    }

    fn select_quants(&self, priority: &TaskPriority) -> usize {
        match priority {
            TaskPriority::High => 10,
            TaskPriority::Normal => 5,
            TaskPriority::Low => 1
        }
    }

    fn get_pid(&mut self) -> usize {
        let result = self.curr_pid;
        self.curr_pid += 1;
        result
    }

    pub fn create_task(&mut self, priority: TaskPriority, entry: extern "C" fn(), stack_addr: u64, mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Task {
        let mut ctx = Context::default();

        self.fill_main_registers(&mut ctx);

        let rsp = self.allocate_pages(stack_addr, 1, mapper, frame_allocator);

        
        ctx.rsp = rsp;
        ctx.rip = entry as u64;

        let quant = self.select_quants(&priority);

        Task { 
            pid: self.get_pid(), 
            ctx, 
            state: TaskState::Starting, 
            priority: priority, 
            quant: quant,
            exit_code: 0,
            ticks_left: quant
        }
    }

    pub fn destroy_task<'a>(&self, task: &'a Task) {
        //will be unmap stack memory
    }
}