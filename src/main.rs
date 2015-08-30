extern crate libc;
extern crate num;
extern crate term;
extern crate terminal_size;

mod radio;

use std::thread::sleep_ms;
use term::color::*;
use terminal_size::{Width, Height, terminal_size};

use radio::hackrf::HackRF;

const TICKS: &'static str = "▁▂▃▄▅▆▇█";

fn map_intensity(val: f32) -> term::color::Color {
    let sorted_colors = [BLACK, BRIGHT_BLACK, MAGENTA, BRIGHT_MAGENTA, BLUE,
                         BRIGHT_BLUE, CYAN, BRIGHT_CYAN, GREEN, BRIGHT_GREEN, YELLOW,
                         BRIGHT_YELLOW, RED, BRIGHT_RED, WHITE, BRIGHT_WHITE,];
    let num_colors = sorted_colors.len();
    if val >= 1.0 {
        sorted_colors[num_colors - 1]
    } else if val <= 0.0 {
        sorted_colors[0]
    } else {
        sorted_colors[(val * num_colors as f32).floor() as usize]
    }
}

fn main() {
    let mut radio = HackRF::open().unwrap();
    let mut t = term::stdout().unwrap();
    if let Some((Width(w), Height(_))) = terminal_size() {
        let mut j = 0;
        t.attr(term::Attr::Bold).unwrap();
        loop {
            for i in 0..w {
                t.fg(map_intensity((i + j) as f32 / 20.0)).unwrap();
                write!(t, "█").unwrap();
            }
            println!("");
            sleep_ms(500);
            j = j + 1;
        }
    } else {
        println!("Unable to get terminal size");
    }
}
