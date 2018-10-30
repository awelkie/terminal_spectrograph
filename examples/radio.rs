extern crate docopt;
extern crate rustc_serialize;
extern crate terminal_spectrograph;

use docopt::Docopt;
use terminal_spectrograph::{Canvas, SignalProcessor, Complex, Event};

use std::ptr;
use std::mem;
use std::slice;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::time::Duration;
use std::sync::{Arc, Mutex, Once, ONCE_INIT};
use std::sync::mpsc::{channel, Sender, sync_channel};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};


const USAGE: &'static str = "
Terminal Spectrograph

Usage:
  terminal_spectrograph <freq-hz> <bandwidth-hz> [options]
  terminal_spectrograph (-h | --help)
  terminal_spectrograph --version

Options:
  -h --help          Show this screen.
  --version          Show version.
  --fft-rate=<rate>  Number of FFTs per second. [default: 10].
";
const VERSION: &'static str = env!("CARGO_PKG_VERSION");


#[derive(Debug, RustcDecodable)]
struct Args {
    arg_freq_hz: Option<u64>,
    arg_bandwidth_hz: Option<f64>,
    flag_fft_rate: u32,
    flag_version: bool,
}

pub fn process_signal(recv: Receiver<Vec<i16>>, send: SyncSender<Vec<Complex<f32>>>,
                      fft_len: Arc<Mutex<usize>>, fft_rate: u32, sample_rate_hz: u32) {
    let mut processor = {
        let len = fft_len.lock().unwrap();
        SignalProcessor::new(sample_rate_hz, fft_rate, *len)
    };

    for buff in recv.iter() {
        {
            let len = fft_len.lock().unwrap();
            if *len != processor.fft_len {
                processor.new_fft_len(*len);
            }
        }

        let samples = buff.iter()
            .map(|sample| unsafe { mem::transmute::<i16, Complex<i8>>(*sample) } )
            .map(|sample| Complex::new(sample.re as f32, sample.im as f32) )
            .collect::<Vec<Complex<f32>>>();

        let spectra = processor.add_signal_buffer(samples);

        for spectrum in spectra {
            // This will implicitly drop spectra when the printing end of the channel
            // isn't ready.
            // TODO should notify the user that we're dropping frames.
            if let Err(TrySendError::Disconnected(_)) = send.try_send(spectrum) {
                return;
            }
        }
    }
}

fn main () {
    let args: Args = Docopt::new(USAGE)
                                .and_then(|d| d.decode())
                                .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        println!("{}", VERSION);
        return;
    }

    let mut radio = HackRF::open().expect("Error opening HackRF");
    let mut canvas = Canvas::new().expect("Error opening terminal");

    let fft_len = Arc::new(Mutex::new(canvas.get_spectrum_width()));

    radio.set_frequency(args.arg_freq_hz.unwrap()).unwrap();
    radio.set_sample_rate(args.arg_bandwidth_hz.unwrap()).unwrap();
    let (spec_send, spec_recv) = sync_channel(1);
    let recv: Receiver<Vec<i16>> = radio.start_rx();

    let fft_len_clone = fft_len.clone();
    std::thread::spawn(move || {
        process_signal(recv, spec_send, fft_len_clone, args.flag_fft_rate,
                       args.arg_bandwidth_hz.unwrap() as u32);
    });

    for spec in spec_recv.iter() {
        canvas.add_spectrum(spec);
        if let Ok(Some(Event::Key('q'))) = canvas.get_term().get_event(Duration::from_secs(0)) {
            break;
        }

        *fft_len.lock().unwrap() = canvas.get_spectrum_width();
    }

    radio.stop_rx().expect("Couldn't stop receiving");
}


#[allow(dead_code, non_camel_case_types)]
mod ffi {
    use super::{c_void, c_int};

    pub type hackrf_device = c_void;
    pub type callback = unsafe extern "C" fn(*mut Transfer) -> c_int;

    #[repr(C)]
    #[derive(Debug)]
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
    #[derive(Debug)]
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
        pub fn hackrf_close(dev: *mut hackrf_device) -> Return;
        pub fn hackrf_set_freq(dev: *mut hackrf_device, freq_hz: u64) -> Return;
        pub fn hackrf_set_sample_rate(dev: *mut hackrf_device, freq_hz: f64) -> Return;
        pub fn hackrf_start_rx(dev: *mut hackrf_device, callback: callback,
                               rx_ctx: *mut c_void) -> Return;
        pub fn hackrf_stop_rx(dev: *mut hackrf_device) -> Return;
    }
}

fn init() -> Result<(), ()> {
    //TODO how do I call hackrf_exit()?
    static mut INIT: Once = ONCE_INIT;
    static mut RESULT: ffi::Return = ffi::Return::SUCCESS;
    unsafe {
        INIT.call_once(|| {
            RESULT = ffi::hackrf_init();
        });

        match RESULT {
            ffi::Return::SUCCESS => Ok(()),
            _ => Err(()),
        }
    }
}

unsafe extern "C" fn rx_callback(transfer: *mut ffi::Transfer) -> c_int {
    let sender: &Option<Sender<Vec<i16>>> = mem::transmute((*transfer).rx_ctx);

    match sender {
        &Some(ref rx_send) => {
            assert_eq!((*transfer).valid_length & 0x01, 0);
            let buffer = slice::from_raw_parts((*transfer).buffer, (*transfer).valid_length as usize)
                            .to_vec()
                            .windows(2)
                            .map(|bytes: &[u8]| {
                                assert_eq!(bytes.len(), 2);
                                let bytes = [ bytes[0], bytes[1] ];
                                mem::transmute::<[u8; 2], i16>(bytes)
                            })
                            .collect::<Vec<i16>>();

            // let buffer = slice::from_raw_parts(
            //     mem::transmute((*transfer).buffer),
            //     (*transfer).valid_length as usize / 2
            // ).to_vec();
            match rx_send.send(buffer) {
                Ok(()) => 0,
                Err(_) => -1,
            }
        },
        &None => -1,
    }
}

#[derive(Debug)]
pub struct HackRF {
    dev: *mut ffi::hackrf_device,
    rx: Option<Sender<Vec<i16>>>,
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

    pub fn start_rx(&mut self) -> Receiver<Vec<i16>> {
        let (rx_send, rx_rec) = channel::<Vec<i16>>();
        self.rx = Some(rx_send);
        unsafe {
            // TODO this can return an error
            ffi::hackrf_start_rx(self.dev, rx_callback, mem::transmute(&self.rx));
        };
        return rx_rec;
    }

    pub fn stop_rx(&mut self) -> Result<(), ()> {
        unsafe {
            match ffi::hackrf_stop_rx(self.dev) {
                ffi::Return::SUCCESS => {
                    //self.rx = None;
                    Ok(())
                },
                _ => Err(()),
            }
        }
    }
}

impl Drop for HackRF {
    fn drop(&mut self) {
        unsafe {
            match ffi::hackrf_close(self.dev) {
                ffi::Return::SUCCESS => (),
                e => panic!("Couldn't close radio: {:?}", e),
            }
        }
    }
}