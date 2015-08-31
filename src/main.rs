extern crate libc;
extern crate num;
extern crate rustfft;
extern crate drawille;
extern crate terminal_size;

mod radio;

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use num::Complex;
use rustfft::FFT;
use drawille::braille::Canvas;
use terminal_size::{Width, terminal_size};

use radio::hackrf::HackRF;

const TICKS: &'static str = "▁▂▃▄▅▆▇█";

//fn map_intensity(val: f32) -> term::color::Color {
    //let sorted_colors = [BLACK, BRIGHT_BLACK, MAGENTA, BRIGHT_MAGENTA, BLUE,
                         //BRIGHT_BLUE, CYAN, BRIGHT_CYAN, GREEN, BRIGHT_GREEN, YELLOW,
                         //BRIGHT_YELLOW, RED, BRIGHT_RED, WHITE, BRIGHT_WHITE,];
    //let num_colors = sorted_colors.len();
    //if val >= 1.0 {
        //sorted_colors[num_colors - 1]
    //} else if val <= 0.0 {
        //sorted_colors[0]
    //} else {
        //sorted_colors[(val * num_colors as f32).floor() as usize]
    //}
//}

fn process_buffer(recv: Receiver<Vec<Complex<i8>>>, send: Sender<Vec<Complex<i8>>>, fft_len: usize) {
    let mut fft = FFT::new(fft_len, false);
    let mut spectrum = vec![Complex::new(0, 0); fft_len];
    for buff in recv.iter() {
        let (num_full_ffts, num_remaining) = (buff.len() / fft_len, buff.len() % fft_len);
        for i in 0..num_full_ffts {
            fft.process(&buff[(i * fft_len) .. ((i + 1) * fft_len)], &mut spectrum[..]);
            send.send(spectrum.clone());
        }
    }
}

fn draw_spectrum(spec: Vec<Complex<i8>>, canvas: &mut Canvas, w: usize, h: usize) {
    canvas.clear();
    for i in 0..w {
        let re = spec[i].re as f32;
        let im = spec[i].im as f32;
        let height = (re * re + im * im).sqrt();
        let max_height = 128;
        //canvas.set((h as f32 * height / 128.0) as usize, i);
        canvas.set(i, (h as f32 * height / 128.0) as usize);
    }
    println!("{}", canvas.frame());
}

fn main() {
    let (Width(w), _) = terminal_size().unwrap();
    let (canvas_width, canvas_height) = ((w * 2) as usize, 20);
    let mut canvas = Canvas::new(canvas_width, canvas_height);
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
        draw_spectrum(spec, &mut canvas, canvas_width, canvas_height);
    }

    child.join().unwrap();
}
