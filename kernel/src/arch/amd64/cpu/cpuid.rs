use core::fmt;

use raw_cpuid::CpuId;

#[inline]
fn vendor_as_str(vendor: &[u8; 12]) -> &str {
    str::from_utf8(vendor).unwrap_or("InvalidCPU")
}

pub struct CpuIdInfoFull {
    pub vendor: [u8; 12],
    pub has_sse: bool,
    pub has_sse2: bool,
    pub has_xsave: bool,
    pub has_avx: bool,
    pub has_nx: bool,
    pub has_apic: bool,
    pub has_x2apic: bool,
    pub logical_cores: u8
}

impl fmt::Display for CpuIdInfoFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vendor = str::from_utf8(&self.vendor).unwrap_or("InvalidCPU");

        writeln!(f, "=========== CPUID INFO ===========")?;
        writeln!(f, "Vendor           : {}", vendor)?;
        writeln!(f, "SSE              : {}", self.has_sse)?;
        writeln!(f, "SSE2             : {}", self.has_sse2)?;
        writeln!(f, "XSAVE            : {}", self.has_xsave)?;
        writeln!(f, "AVX              : {}", self.has_avx)?;
        writeln!(f, "NX               : {}", self.has_nx)?;
        writeln!(f, "APIC             : {}", self.has_apic)?;
        writeln!(f, "X2APIC           : {}", self.has_x2apic)?;
        writeln!(f, "LOGICAL_CORES    : {}", self.logical_cores)?;
        write!(f, "=================================")
    }
}

pub fn get_cpuid_full() -> CpuIdInfoFull {
    let cpuid = CpuId::new();

    let vendor = if let Some(v) = cpuid.get_vendor_info() {
        let bytes = v.as_str().as_bytes();
        let mut buf = [0u8; 12];
        buf.copy_from_slice(&bytes[..12]);
        buf
    } else {
        *b"UnknownCPU  " 
    };

    let feature_info = cpuid.get_feature_info();
    let ext_features = cpuid.get_extended_processor_and_feature_identifiers();

    CpuIdInfoFull {
        vendor,

        has_sse: feature_info
            .as_ref()
            .map(|f| f.has_sse())
            .unwrap_or(false),

        has_sse2: feature_info
            .as_ref()
            .map(|f| f.has_sse2())
            .unwrap_or(false),

        has_xsave: feature_info
            .as_ref()
            .map(|f| f.has_xsave())
            .unwrap_or(false),

        has_avx: feature_info
            .as_ref()
            .map(|f| f.has_avx())
            .unwrap_or(false),

        has_apic: feature_info
            .as_ref()
            .map(|f| f.has_apic())
            .unwrap_or(false),

        has_x2apic: feature_info
            .as_ref()
            .map(|f| f.has_x2apic())
            .unwrap_or(false),

        has_nx: ext_features
            .as_ref()
            .map(|f| f.has_execute_disable())
            .unwrap_or(false),

        logical_cores: feature_info
            .as_ref()
            .map(|f| f.max_logical_processor_ids())
            .unwrap_or(1),
    }
}