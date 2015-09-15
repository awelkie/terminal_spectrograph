extern crate libc;
extern crate num;
extern crate rustfft;
extern crate rustty;
extern crate rustc_serialize;
extern crate docopt;

mod radio;
mod drawing;

use std::sync::mpsc::{Receiver, Sender, channel};
use num::Complex;
use rustfft::FFT;
use rustty::{Terminal, Event};
use docopt::Docopt;

use radio::hackrf::HackRF;
use drawing::draw_spectrum;

const USAGE: &'static str = "
Terminal Spectrograph

Usage:
  terminal_spectrograph <freq-hz> <bandwidth-hz> [options]
  terminal_spectrograph (-h | --help)
  terminal_spectrograph --version

Options:
  -h --help          Show this screen.
  --version          Show version.
  --fft-len=<len>    Length of the FFT [default: 4096].
  --fft-rate=<rate>  Number of FFTs per second. [default: 30].
";
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_freq_hz: Option<u64>,
    arg_bandwidth_hz: Option<f64>,
    flag_fft_len: usize,
    flag_fft_rate: usize,
    flag_version: bool,
}

fn process_buffer(recv: Receiver<Vec<Complex<i8>>>, send: Sender<Vec<Complex<f32>>>,
                  fft_len: usize, fft_rate: usize, sample_rate_hz: usize) {
    let mut fft = FFT::new(fft_len, false);
    let mut spectrum = vec![Complex::new(0.0, 0.0); fft_len];
    let mut signal = Vec::with_capacity(fft_len);
    let num_samples_to_discard = (sample_rate_hz - fft_rate * fft_len) / fft_rate;
    let mut samples_discarded = 0;
    for buff in recv.iter() {
        for x in buff {
            if samples_discarded >= num_samples_to_discard {
                signal.push(Complex::new(x.re as f32, x.im as f32));

                if signal.len() >= signal.capacity() {
                    fft.process(&signal[..], &mut spectrum[..]);
                    if let Err(_) = send.send(spectrum.clone()) {
                        return;
                    }
                    signal.clear();
                    samples_discarded = 0;
                }
            } else {
                // discard these samples to maintain the desired FFT rate.
                samples_discarded += 1;
            }
        }
    }
}

fn main() {
    let args: Args = Docopt::new(USAGE)
                                .and_then(|d| d.decode())
                                .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        println!("{}", VERSION);
        return;
    }

    let mut radio = HackRF::open().unwrap_or_else(|_| {
        panic!("Couldn't open HackRF radio.");
    });

    let mut term = Terminal::new().unwrap_or_else(|e| {
        panic!("Couldn't open terminal: {}", e);
    });

    radio.set_frequency(args.arg_freq_hz.unwrap()).unwrap();
    radio.set_sample_rate(args.arg_bandwidth_hz.unwrap()).unwrap();
    let (spec_send, spec_recv) = channel();
    let recv = radio.start_rx();

    std::thread::spawn(move || {
        process_buffer(recv, spec_send, args.flag_fft_len, args.flag_fft_rate,
                       args.arg_bandwidth_hz.unwrap() as usize);
    });

    for spec in spec_recv.iter() {
        draw_spectrum(&mut term, spec);
        term.swap_buffers().unwrap();
        if let Ok(Some(Event::Key('q'))) = term.get_event(0) {
            break;
        }
    }
    drop(spec_recv);

    radio.stop_rx().unwrap_or_else(|_| {
        panic!("Couldn't stop receiving");
    });
}
