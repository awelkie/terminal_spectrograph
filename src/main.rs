extern crate libc;
extern crate rustty;
extern crate docopt;
extern crate rustfft;
extern crate itertools;
extern crate rustc_serialize;


mod radio;
mod drawing;
mod processing;


use docopt::Docopt;
use rustty::Event;

use drawing::Canvas;
use radio::hackrf::HackRF;
use processing::process_signal;

use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::sync_channel;


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

fn main() {
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
    let recv = radio.start_rx();

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
