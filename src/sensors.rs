//! Read-only Intel SMC temperatures using the existing project's SMC protocol.
use std::ffi::{c_char, c_int, c_void};
use std::mem;
use std::sync::{Mutex, OnceLock};
const KERNEL_INDEX_SMC: u32 = 2;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_READ_KEYINFO: u8 = 9;
struct AppleSmc {
    connection: u32,
}

impl AppleSmc {
    fn new() -> Option<Self> {
        let matching = unsafe { IOServiceMatching(c"AppleSMC".as_ptr()) };
        if matching.is_null() {
            return None;
        }
        let mut iterator = 0_u32;
        if unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != 0 {
            return None;
        }
        let service = unsafe { IOIteratorNext(iterator) };
        unsafe { IOObjectRelease(iterator) };
        if service == 0 {
            return None;
        }
        let mut connection = 0_u32;
        let result = unsafe { IOServiceOpen(service, mach_task_self_, 0, &mut connection) };
        unsafe { IOObjectRelease(service) };
        (result == 0 && connection != 0).then_some(Self { connection })
    }

    fn read_temperature(&self, key: [u8; 4]) -> Option<f64> {
        let mut input = SmcKeyData {
            key: u32::from_be_bytes(key),
            data8: SMC_CMD_READ_KEYINFO,
            ..SmcKeyData::default()
        };
        let mut output = SmcKeyData::default();
        let mut output_size = mem::size_of::<SmcKeyData>();
        if unsafe {
            IOConnectCallStructMethod(
                self.connection,
                KERNEL_INDEX_SMC,
                (&input as *const SmcKeyData).cast(),
                mem::size_of::<SmcKeyData>(),
                (&mut output as *mut SmcKeyData).cast(),
                &mut output_size,
            )
        } != 0
            || output_size != mem::size_of::<SmcKeyData>()
            || output.result != 0
        {
            return None;
        }
        let data_size = output.key_info.data_size;
        let data_type = output.key_info.data_type;
        if data_size > 32 {
            return None;
        }
        input.key_info.data_size = data_size;
        input.data8 = SMC_CMD_READ_BYTES;
        output = SmcKeyData::default();
        output_size = mem::size_of::<SmcKeyData>();
        if unsafe {
            IOConnectCallStructMethod(
                self.connection,
                KERNEL_INDEX_SMC,
                (&input as *const SmcKeyData).cast(),
                mem::size_of::<SmcKeyData>(),
                (&mut output as *mut SmcKeyData).cast(),
                &mut output_size,
            )
        } != 0
            || output_size != mem::size_of::<SmcKeyData>()
            || output.result != 0
        {
            return None;
        }
        decode_smc_temperature(data_size, data_type, &output.bytes)
    }
}

impl Drop for AppleSmc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.connection) };
    }
}

fn decode_smc_temperature(data_size: u32, data_type: u32, bytes: &[u8; 32]) -> Option<f64> {
    let value = match (data_type.to_be_bytes(), data_size) {
        (kind, size) if kind == *b"flt " && size >= 4 => {
            f64::from(f32::from_le_bytes(bytes[..4].try_into().ok()?))
        }
        (kind, size) if kind == *b"sp78" && size >= 2 => {
            f64::from(i16::from_be_bytes(bytes[..2].try_into().ok()?)) / 256.0
        }
        _ => return None,
    };
    (value.is_finite() && value > 0.0 && value < 150.0).then_some(value)
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyDataVersion {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyDataLimit {
    version: u16,
    length: u16,
    cpu_limit: u32,
    gpu_limit: u32,
    memory_limit: u32,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyInfo {
    data_size: u32,
    data_type: u32,
    attributes: u8,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyData {
    key: u32,
    version: SmcKeyDataVersion,
    limit: SmcKeyDataLimit,
    key_info: SmcKeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

pub fn temperature(key: [u8; 4]) -> Option<f64> {
    static SMC: OnceLock<Mutex<Option<AppleSmc>>> = OnceLock::new();
    let guard = SMC
        .get_or_init(|| Mutex::new(AppleSmc::new()))
        .lock()
        .ok()?;
    guard.as_ref()?.read_temperature(key)
}
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *const c_void;
    fn IOServiceGetMatchingServices(
        port: u32,
        matching: *const c_void,
        iterator: *mut u32,
    ) -> c_int;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> c_int;
    fn IOServiceOpen(service: u32, task: u32, kind: u32, connection: *mut u32) -> c_int;
    fn IOServiceClose(connection: u32) -> c_int;
    fn IOConnectCallStructMethod(
        connection: u32,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> c_int;
}
unsafe extern "C" {
    static mach_task_self_: u32;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smc_abi_and_signed_temperature() {
        assert_eq!(mem::size_of::<SmcKeyData>(), 80);
        let mut b = [0; 32];
        b[..2].copy_from_slice(&0x3280_i16.to_be_bytes());
        assert_eq!(
            decode_smc_temperature(2, u32::from_be_bytes(*b"sp78"), &b),
            Some(50.5)
        );
        assert_eq!(
            decode_smc_temperature(1, u32::from_be_bytes(*b"sp78"), &b),
            None
        );
    }
}
