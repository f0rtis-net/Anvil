use crate::selftest::SelfTestMask;

pub struct KernelArgs {
    pub selftest: bool,
    pub selftest_mask: SelfTestMask,

    pub debug: bool,
}

impl KernelArgs {
    pub fn default() -> Self {
        Self {
            selftest: false,
            selftest_mask: SelfTestMask::default(),
            debug: false
        }
    }
}

pub fn parse_cmdline(cmdline: &[u8]) -> KernelArgs {
    let mut args = KernelArgs::default();

    for token in cmdline.split(|&b| b == b' ') {
        match token {
            b"selftest=all" => {
                args.selftest = true;
            }
            b"selftest=pmm" => {
                args.selftest = true;
                args.selftest_mask.pmm = true;
            }
            b"debug" => {
                args.debug = true;
            }
            _ => {}
        }
    }

    args
}