//! Minimal AppleSMC client over IOKit (read / write keys).

use std::ffi::{c_char, c_void};

type KernReturn = i32;
type MachPort = u32;
type IoObject = u32;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: MachPort, matching: *mut c_void) -> IoObject;
    fn IOServiceOpen(service: IoObject, owning_task: MachPort, kind: u32, connect: *mut u32) -> KernReturn;
    fn IOServiceClose(connect: u32) -> KernReturn;
    fn IOObjectRelease(obj: IoObject) -> KernReturn;
    fn IOConnectCallStructMethod(
        connect: u32,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> KernReturn;
}

extern "C" {
    static mach_task_self_: MachPort;
}

const KERNEL_INDEX_SMC: u32 = 2;
const CMD_READ_BYTES: u8 = 5;
const CMD_WRITE_BYTES: u8 = 6;
const CMD_READ_KEYINFO: u8 = 9;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Vers {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct PLimit {
    version: u16,
    length: u16,
    cpu: u32,
    gpu: u32,
    mem: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyInfo {
    data_size: u32,
    data_type: u32,
    data_attributes: u8,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyData {
    key: u32,
    vers: Vers,
    plimit: PLimit,
    key_info: KeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

const _: () = assert!(std::mem::size_of::<KeyData>() == 80);

pub struct Key {
    pub bytes: Vec<u8>,
}

pub struct Smc {
    conn: u32,
}

fn code(key: &str) -> u32 {
    let b = key.as_bytes();
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

impl Smc {
    pub fn open() -> Result<Smc, String> {
        unsafe {
            let svc = IOServiceGetMatchingService(0, IOServiceMatching(c"AppleSMC".as_ptr()));
            if svc == 0 {
                return Err("AppleSMC service not found".into());
            }
            let mut conn = 0;
            let kr = IOServiceOpen(svc, mach_task_self_, 0, &mut conn);
            IOObjectRelease(svc);
            if kr != 0 {
                return Err(format!("IOServiceOpen failed: {kr:#x}"));
            }
            Ok(Smc { conn })
        }
    }

    fn call(&self, input: &KeyData) -> Result<KeyData, String> {
        let mut out = KeyData::default();
        let mut size = std::mem::size_of::<KeyData>();
        let kr = unsafe {
            IOConnectCallStructMethod(
                self.conn,
                KERNEL_INDEX_SMC,
                input as *const _ as *const c_void,
                std::mem::size_of::<KeyData>(),
                &mut out as *mut _ as *mut c_void,
                &mut size,
            )
        };
        if kr != 0 {
            return Err(format!("IOConnectCallStructMethod: {kr:#x}"));
        }
        if out.result != 0 {
            return Err(format!("SMC result {}", out.result));
        }
        Ok(out)
    }

    fn info(&self, key: &str) -> Result<KeyInfo, String> {
        let input = KeyData { key: code(key), data8: CMD_READ_KEYINFO, ..Default::default() };
        Ok(self.call(&input)?.key_info)
    }

    pub fn read(&self, key: &str) -> Result<Key, String> {
        let info = self.info(key)?;
        let input = KeyData { key: code(key), key_info: info, data8: CMD_READ_BYTES, ..Default::default() };
        let out = self.call(&input)?;
        let n = (info.data_size as usize).min(32);
        Ok(Key { bytes: out.bytes[..n].to_vec() })
    }


    pub fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let info = self.info(key)?;
        if info.data_size as usize != bytes.len() {
            return Err(format!("{key}: expected {} bytes, got {}", info.data_size, bytes.len()));
        }
        let mut input = KeyData { key: code(key), key_info: info, data8: CMD_WRITE_BYTES, ..Default::default() };
        input.bytes[..bytes.len()].copy_from_slice(bytes);
        self.call(&input).map(|_| ())
    }
}

impl Drop for Smc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.conn) };
    }
}
