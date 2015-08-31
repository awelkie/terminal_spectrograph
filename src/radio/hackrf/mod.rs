use std::sync::{Once, ONCE_INIT};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::ptr;
use std::mem;
use std::vec;
use std::slice;
use libc::c_int;
use num::Complex;

mod ffi {
    use libc::{c_void, c_int};

    pub type hackrf_device = c_void;
    pub type callback = unsafe extern "C" fn(*mut Transfer) -> c_int;

    #[repr(C)]
    pub enum Return {
        SUCCESS = 0,
        TRUE = 1,
        ERROR_INVALID_PARAM = -2,
        ERROR_NOT_FOUND = -5,
        ERROR_BUSY = -6,
        ERROR_NO_MEM = -11,
        ERROR_LIBUSB = -1000,
        ERROR_THREAD = -1001,
        ERROR_STREAMING_THREAD_ERR = -1002,
        ERROR_STREAMING_STOPPED = -1003,
        ERROR_STREAMING_EXIT_CALLED = -1004,
        ERROR_OTHER = -9999,
    }

    #[repr(C)]
    pub struct Transfer {
        pub device: *mut hackrf_device,
        pub buffer: *mut u8,
        pub buffer_length: c_int,
        pub valid_length: c_int,
        pub rx_ctx: *mut c_void,
        pub tx_ctx: *mut c_void,
    }

    #[link(name = "hackrf")]
    extern "C" {
        pub fn hackrf_init() -> Return;
        pub fn hackrf_exit() -> Return;
        pub fn hackrf_open(dev: *mut *mut hackrf_device) -> Return;
        pub fn hackrf_close(dev: *mut *mut hackrf_device) -> Return;
        pub fn hackrf_set_freq(dev: *mut hackrf_device, freq_hz: u64) -> Return;
        pub fn hackrf_set_sample_rate(dev: *mut hackrf_device, freq_hz: f64) -> Return;
        pub fn hackrf_start_rx(dev: *mut hackrf_device, callback: callback,
                               rx_ctx: *mut c_void) -> Return;
    }
}

fn init() -> Result<(), ()> {
    //TODO how do I call hackrf_exit()?
    static mut INIT: Once = ONCE_INIT;
    static mut result: ffi::Return = ffi::Return::SUCCESS;
    unsafe {
        INIT.call_once(|| {
            result = ffi::hackrf_init();
        });

        match result {
            ffi::Return::SUCCESS => Ok(()),
            _ => Err(()),
        }
    }
}

unsafe extern "C" fn rx_callback(transfer: *mut ffi::Transfer) -> c_int {
    let sender: &Option<Sender<Vec<Complex<i8>>>> = mem::transmute((*transfer).rx_ctx);

    match sender {
        &Some(ref rx_send) => {
            assert_eq!((*transfer).valid_length & 0x01, 0);
            let buffer = slice::from_raw_parts(
                mem::transmute((*transfer).buffer),
                (*transfer).valid_length as usize / 2
            ).to_vec();
            match rx_send.send(buffer) {
                Ok(()) => 0,
                Err(_) => -1,
            }
        },
        &None => -1,
    }
}


pub struct HackRF {
    dev: *mut ffi::hackrf_device,
    rx: Option<Sender<Vec<Complex<i8>>>>,
}

impl HackRF {
    pub fn open() -> Result<Self, ()> {
        try!(init());

        let mut dev: *mut ffi::hackrf_device = ptr::null_mut();
        unsafe {
            match ffi::hackrf_open(&mut dev) {
                ffi::Return::SUCCESS => Ok(HackRF{dev: dev, rx: None}),
                _ => Err(()),
            }
        }
    }

    pub fn set_frequency(&mut self, freq_hz: u64) -> Result<(), ()> {
        unsafe {
            match ffi::hackrf_set_freq(self.dev, freq_hz) {
                ffi::Return::SUCCESS => Ok(()),
                _ => Err(()),
            }
        }
    }

    pub fn set_sample_rate(&mut self, freq_hz: f64) -> Result<(), ()> {
        unsafe {
            match ffi::hackrf_set_sample_rate(self.dev, freq_hz) {
                ffi::Return::SUCCESS => Ok(()),
                _ => Err(()),
            }
        }
    }

    pub fn start_rx(&mut self) -> Receiver<Vec<Complex<i8>>> {
        let (rx_send, rx_rec) = channel::<Vec<Complex<i8>>>();
        self.rx = Some(rx_send);
        unsafe {
            ffi::hackrf_start_rx(self.dev, rx_callback, mem::transmute(&self.rx));
        };
        return rx_rec;
    }
}

impl Drop for HackRF {
    fn drop(&mut self) {
        unsafe {
            ffi::hackrf_close(&mut self.dev);
        }
    }
}
