extern crate libc;
extern crate num;
extern crate rustfft;
extern crate rustty;

mod radio;

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use num::Complex;
use rustfft::FFT;
use rustty::{Terminal, Cell};

use radio::hackrf::HackRF;

fn process_buffer(recv: Receiver<Vec<Complex<i8>>>, send: Sender<Vec<Complex<f32>>>, fft_len: usize) {
    let mut fft = FFT::new(fft_len, false);
    let mut signal = vec![Complex::new(0.0, 0.0); fft_len];
    let mut spectrum = vec![Complex::new(0.0, 0.0); fft_len];
    //TODO wrap around
    for buff in recv.iter() {
        let (num_full_ffts, num_remaining) = (buff.len() / fft_len, buff.len() % fft_len);
        for i in 0..num_full_ffts {
            for (s, x) in signal.iter_mut().zip(buff[i * fft_len .. (i + 1) * fft_len].iter()) {
                *s = Complex::new(x.re as f32, x.im as f32);
            }
            fft.process(&signal[..], &mut spectrum[..]);
            send.send(spectrum.clone());
        }
    }
}

// TODO
fn pixel_nums_to_braille(p1: Option<u8>, u2: Option<u8>) -> char {
    '⡀'
}

fn draw_spectrum(term: &mut Terminal, spec: Vec<Complex<f32>>) {
    term.clear();
    let (num_cols, num_rows) = term.size();
    let max_height = 1000.0;
    for col_idx in 0..num_cols {
        //TODO binning
        // height in float between 0 and 1.
        let (h1, h2) = (spec[col_idx * 2].norm(), spec[col_idx * 2 + 1].norm());
        let (h1, h2) = (h1 / max_height, h2 / max_height);
        let h1 = if h1 > 1.0 { 1.0 } else { h1 };
        let h2 = if h2 > 1.0 { 1.0 } else { h2 };

        // which character the pixel will be in
        let c1 = (h1 * num_rows as f32).floor() as usize;
        let c2 = (h2 * num_rows as f32).floor() as usize;
        let c1 = if c1 == num_rows { c1 - 1 } else { c1 };
        let c2 = if c2 == num_rows { c2 - 1 } else { c2 };

        // which pixel in that character
        // TODO
        let p1 = 1;
        let p2 = 1;

        if c1 == c2 {
            term[(col_idx, c1)] = Cell::with_char(pixel_nums_to_braille(Some(p1), Some(p2)));
        } else {
            term[(col_idx, c1)] = Cell::with_char(pixel_nums_to_braille(Some(p1), None));
            term[(col_idx, c2)] = Cell::with_char(pixel_nums_to_braille(None, Some(p2)));
        }
    }
}

fn main() {
    let mut term = Terminal::new().unwrap();

    let mut radio = HackRF::open().unwrap();
    let freq_hz = 914000000;
    let sample_rate = 1e6;
    let fft_len = 2048;
    radio.set_frequency(freq_hz).unwrap();
    radio.set_sample_rate(sample_rate).unwrap();
    let (spec_send, spec_recv) = channel();
    let recv = radio.start_rx();

    let child = std::thread::spawn(move || {
        process_buffer(recv, spec_send, fft_len);
    });

    for spec in spec_recv.iter() {
        draw_spectrum(&mut term, spec);
        term.swap_buffers().unwrap();
        thread::sleep_ms(500);
    }

    child.join().unwrap();
}
