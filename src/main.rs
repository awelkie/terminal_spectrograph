extern crate libc;
extern crate num;
extern crate rustfft;
extern crate rustty;
extern crate rustc_serialize;
extern crate docopt;

mod radio;

use std::sync::mpsc::{Receiver, Sender, channel};
use std::char;
use num::Complex;
use rustfft::FFT;
use rustty::{Terminal, Cell, Event};
use docopt::Docopt;

use radio::hackrf::HackRF;

const USAGE: &'static str = "
Terminal Spectrograph

Usage:
  terminal_spectrograph <freq-hz> <bandwidth-hz> [--fft-len=<len>]
  terminal_spectrograph (-h | --help)
  terminal_spectrograph --version

Options:
  -h --help        Show this screen.
  --version        Show version.
  --fft-len=<len>  Length of the FFT [default: 4096].
";
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_freq_hz: Option<u64>,
    arg_bandwidth_hz: Option<f64>,
    flag_fft_len: usize,
    flag_version: bool,
}

fn process_buffer(recv: Receiver<Vec<Complex<i8>>>, send: Sender<Vec<Complex<f32>>>, fft_len: usize) {
    let mut fft = FFT::new(fft_len, false);
    let mut spectrum = vec![Complex::new(0.0, 0.0); fft_len];
    let mut signal = Vec::with_capacity(fft_len);
    for buff in recv.iter() {
        for x in buff {
            signal.push(Complex::new(x.re as f32, x.im as f32));

            if signal.len() >= signal.capacity() {
                fft.process(&signal[..], &mut spectrum[..]);
                if let Err(_) = send.send(spectrum.clone()) {
                    return;
                }
                signal.clear();
            }
        }
    }
}

fn pixel_nums_to_braille(p1: Option<u8>, p2: Option<u8>) -> char {
    let pixel_map = [[0x01, 0x08],
                     [0x02, 0x10],
                     [0x04, 0x20],
                     [0x40, 0x80]];

    let mut c = 0;
    if let Some(p) = p1 {
        c |= pixel_map[p as usize][0];
    }

    if let Some(p) = p2 {
        c |= pixel_map[p as usize][1];
    }

    char::from_u32((0x2800 + c) as u32).unwrap()
}

fn bin_heights(source: &[Complex<f32>], dest: &mut [f32]) {
    let samples_per_bin = source.len() / dest.len();
    let mut height = 0.0;
    let mut i = 0;
    let mut j = 0;
    for x in source {
        height += x.norm();
        if i >= samples_per_bin {
            dest[j] = height / samples_per_bin as f32;
            i = 0;
            j += 1;
            height = 0.0;
        } else {
            i += 1;
        }
    }

    if i != 0 {
        dest[j] = height / i as f32;
    }
}

fn draw_spectrum(term: &mut Terminal, spec: Vec<Complex<f32>>) {
    term.clear().unwrap();
    let (num_cols, num_rows) = term.size();
    // TODO what should this max height be?
    let num_rows = if num_rows > 20 { 20 } else { num_rows };
    let pixel_height = num_rows * 4;
    let pixel_width = num_cols * 2;
    // TODO what should this value be?
    let max_height = 500.0;

    let mut bins = vec![0.0; pixel_width];
    bin_heights(&spec[..], &mut bins[..]);

    for col_idx in 0..num_cols {
        // height in float between 0 and 1.
        let h1 = bins[col_idx * 2] / max_height;
        let h2 = bins[col_idx * 2 + 1] / max_height;

        // The "pixel" height of each point.
        let p1 = (h1 * pixel_height as f32).floor() as usize;
        let p2 = (h2 * pixel_height as f32).floor() as usize;
        let p1 = if p1 >= pixel_height { pixel_height - 1 } else { p1 };
        let p2 = if p2 >= pixel_height { pixel_height - 1 } else { p2 };

        // Reverse it, since the terminal indexing is from the top
        let p1 = pixel_height - p1;
        let p2 = pixel_height - p2;

        let c1 = p1 / 4;
        let c2 = p2 / 4;
        //let c1 = num_rows - p1 / 4;
        //let c2 = num_rows - p2 / 4;

        if c1 == c2 {
            term[(col_idx, c1)] = Cell::with_char(
                pixel_nums_to_braille(Some((p1 % 4) as u8), Some((p2 % 4) as u8)));
        } else {
            term[(col_idx, c1)] = Cell::with_char(
                pixel_nums_to_braille(Some((p1 % 4) as u8), None));
            term[(col_idx, c2)] = Cell::with_char(
                pixel_nums_to_braille(None, Some((p2 % 4) as u8)));
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

    let child = std::thread::spawn(move || {
        process_buffer(recv, spec_send, args.flag_fft_len);
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

    child.join().unwrap_or_else(|_| {
        panic!("Error joining with FFT thread");
    });
}
