use std::char;
use std::cmp::{max, min};
use std::collections::VecDeque;
use num::{Complex, Float};
use rustty;
use rustty::{Attr, Color, Terminal, Cell, CellAccessor, HasSize};
use rustty::ui::{Alignable, Widget, VerticalAlign};
use itertools::{Itertools, EitherOrBoth};

pub struct Canvas {
    term: Terminal,
    spectrum: Widget,
    waterfall: Widget,
    history: VecDeque<Vec<f32>>,
}

impl Canvas {
    pub fn new() -> Result<Self, rustty::Error> {
        let term = try!(Terminal::new());
        let (cols, rows) = term.size();

        let spectrum_height = rows / 2;
        let waterfall_height = if rows % 2 != 0 { rows / 2 } else { rows / 2 + 1 };

        let mut spectrum = Widget::new(cols, spectrum_height);
        spectrum.valign(&term, VerticalAlign::Top, 0);

        let mut waterfall = Widget::new(cols, waterfall_height);
        waterfall.valign(&term, VerticalAlign::Bottom, 0);

        Ok(Canvas {
            term: term,
            spectrum: spectrum,
            waterfall: waterfall,
            history: VecDeque::with_capacity(waterfall_height * 2),
        })
    }

    /// Adds a spectrum to the history and draws it on the waterfall
    /// and the spectrum view.
    pub fn add_spectrum(&mut self, spec: Vec<Complex<f32>>) {
        draw_spectrum(&mut self.spectrum, &spec);

        let normalized = normalize_spectrum(&spec, 26.0);
        let averaged = normalized.chunks(2).map(|v| (v[0] + v[1]) / 2.0).collect();
        self.history.push_front(averaged);

        draw_waterfall(&mut self.waterfall, &self.history);

        self.spectrum.draw_into(&mut self.term);
        self.waterfall.draw_into(&mut self.term);
        self.term.swap_buffers().unwrap();
    }

    pub fn get_term(&mut self) -> &mut Terminal {
        &mut self.term
    }
}

fn draw_waterfall<T: CellAccessor + HasSize>(canvas: &mut T, spectra: &VecDeque<Vec<f32>>) {
    let (cols, rows) = canvas.size();
    for (row, mut specs) in (0..rows).zip(&spectra.iter().chunks_lazy(2)) {
        if let Some(upper_heights) = specs.next().map(|vec| vec.iter()) {
            match specs.next().map(|vec| vec.iter()) {
                Some(lower_heights) => {
                    for (c, heights) in (0..cols).zip(upper_heights.zip_longest(lower_heights)) {
                        let (u, l) = match heights {
                            EitherOrBoth::Both(&upper, &lower) => (upper, lower),
                            EitherOrBoth::Left(&upper) => (upper, 0.0),
                            EitherOrBoth::Right(&lower) => (0.0, lower),
                        };
                        *canvas.get_mut(c, row).unwrap() = spectrum_heights_to_waterfall_cell(u, l);
                    }
                },
                None => {
                    for (c, u) in (0..cols).zip(upper_heights) {
                        *canvas.get_mut(c, row).unwrap() = spectrum_heights_to_waterfall_cell(*u, 0.0);
                    }
                }
            }
        }
    }
}

fn spectrum_heights_to_waterfall_cell(upper: f32, lower: f32) -> Cell {
    Cell::new('▀',
              Color::Byte(color_mapping(upper)),
              Color::Byte(color_mapping(lower)),
              Attr::Default)
}

/// Assumes `f` is between 0 and 1. Anything outside of this range
/// will be clamped.
fn color_mapping(f: f32) -> u8 {
    //let lower = 16.0;
    //let upper = 231.0;
    let lower = 232.0;
    let upper = 255.0;
    let mapped = f * (upper - lower) + lower;
    if mapped < lower {
        lower as u8
    } else if mapped > upper {
        upper as u8
    } else {
        mapped as u8
    }
}

fn normalize_spectrum(spec: &[Complex<f32>], max_db: f32) -> Vec<f32> {
    // FFT shift
    let (first_half, last_half) = spec.split_at((spec.len() + 1) / 2);
    let shifted_spec = last_half.iter().chain(first_half.iter());

    // normalize and take the log
    shifted_spec.map(Complex::norm)
                .map(Float::log10)
                .map(|x| 10.0 * x)
                .map(|x| x / max_db)
                .collect()
}

// indexing is from the top of the cell
fn pixel_nums_to_braille(p1: Option<u8>, p2: Option<u8>) -> char {
    let pixel_map = [[0x01, 0x08],
                     [0x02, 0x10],
                     [0x04, 0x20],
                     [0x40, 0x80]];

    let mut c = 0;
    if let Some(p) = p1 {
        for i in p..4 {
            c |= pixel_map[i as usize][0];
        }
    }

    if let Some(p) = p2 {
        for i in p..4 {
            c |= pixel_map[i as usize][1];
        }
    }

    char::from_u32((0x2800 + c) as u32).unwrap()
}

fn fft_shift<T: Clone>(spec: &mut [T]) {
    let spec_copy = spec.to_owned();

    let (first_half, last_half) = spec_copy.split_at((spec_copy.len() + 1) / 2);
    let shifted_spec = last_half.iter().chain(first_half.iter());
    for (x, y) in spec.iter_mut().zip(shifted_spec) {
        *x = y.clone();
    }
}

fn spectrum_to_bin_heights(spec: &[Complex<f32>], dest: &mut [f32]) {
    //TODO should be plotting in log scale

    // subsample
    let mut last_idx = -1isize;
    for (i, x) in spec.iter().map(Complex::norm).enumerate() {
        if (i * dest.len() / spec.len()) as isize > last_idx {
            last_idx += 1;
            dest[last_idx as usize] = x;
        }
    }

    //TODO unnecessary allocation
    fft_shift(dest);
}

fn char_to_cell(c: char) -> Cell {
    let mut cell = Cell::with_char(c);
    cell.set_attrs(Attr::Bold);
    cell
}

fn draw_pixel_pair<T>(canvas: &mut T, col_idx: usize, p1: usize, p2: usize)
    where T: CellAccessor + HasSize
{
    let (_, rows) = canvas.size();
    let max_pixel_height = 4 * rows;

    // clamp heights
    let p1 = if p1 >= max_pixel_height { max_pixel_height - 1} else { p1 };
    let p2 = if p2 >= max_pixel_height { max_pixel_height - 1} else { p2 };

    // Reverse it, since the terminal indexing is from the top
    let p1 = max_pixel_height - p1 - 1;
    let p2 = max_pixel_height - p2 - 1;

    // cell indices
    let c1 = p1 / 4;
    let c2 = p2 / 4;

    // Fill in full height cells.
    let full_cell_char = pixel_nums_to_braille(Some(0), Some(0));
    for row_idx in max(c1, c2)..rows {
        *canvas.get_mut(col_idx, row_idx).unwrap() = char_to_cell(full_cell_char);
    }

    let left_fill_cell_char = pixel_nums_to_braille(Some(0), None);
    for row_idx in min(c1, c2)..c2 {
        *canvas.get_mut(col_idx, row_idx).unwrap() = char_to_cell(left_fill_cell_char);
    }

    let right_fill_cell_char = pixel_nums_to_braille(None, Some(0));
    for row_idx in min(c1, c2)..c1 {
        *canvas.get_mut(col_idx, row_idx).unwrap() = char_to_cell(right_fill_cell_char);
    }

    // Now fill in partial height cells.
    if c1 == c2 {
        // top pixels are in the same cell
        *canvas.get_mut(col_idx, c1).unwrap() = char_to_cell(
            pixel_nums_to_braille(Some((p1 % 4) as u8), Some((p2 % 4) as u8)));
    } else if c1 > c2 {
        // right pixel is in a higher cell.
        *canvas.get_mut(col_idx, c1).unwrap() = char_to_cell(
            pixel_nums_to_braille(Some((p1 % 4) as u8), Some(0)));
        *canvas.get_mut(col_idx, c2).unwrap() = char_to_cell(
            pixel_nums_to_braille(None, Some((p2 % 4) as u8)));
    } else {
        // left pixel is in a higher cell.
        *canvas.get_mut(col_idx, c1).unwrap() = char_to_cell(
            pixel_nums_to_braille(Some((p1 % 4) as u8), None));
        *canvas.get_mut(col_idx, c2).unwrap() = char_to_cell(
            pixel_nums_to_braille(Some(0), Some((p2 % 4) as u8)));
    }
}

fn draw_spectrum<T: CellAccessor + HasSize>(canvas: &mut T, spec: &[Complex<f32>]) {
    canvas.clear(Cell::default());
    let (num_cols, num_rows) = canvas.size();
    let pixel_height = num_rows * 4;
    let pixel_width = num_cols * 2;
    // TODO what should this value be?
    let max_height = 500.0;

    let mut bins = vec![0.0; pixel_width];
    spectrum_to_bin_heights(spec, &mut bins[..]);

    for col_idx in 0..num_cols {
        // height in float between 0 and 1.
        let h1 = bins[col_idx * 2] / max_height;
        let h2 = bins[col_idx * 2 + 1] / max_height;

        // The "pixel" height of each point.
        let p1 = (h1 * pixel_height as f32).floor() as usize;
        let p2 = (h2 * pixel_height as f32).floor() as usize;

        draw_pixel_pair(canvas, col_idx, p1, p2);
    }
}

#[cfg(test)]
mod tests {
    use super::{pixel_nums_to_braille, fft_shift, draw_pixel_pair};
    use rustty::Terminal;

    #[test]
    fn test_fft_shift_dc() {
        let len = 9;
        let mut spec = vec![0; len];
        spec[0] = 1;
        fft_shift(&mut spec[..]);
        assert_eq!(spec[len / 2], 1);
    }

    #[test]
    fn test_fft_shift_even() {
        let mut before: Vec<usize> = (0..10).collect();
        let after = vec![5, 6, 7, 8, 9, 0, 1, 2, 3, 4];
        fft_shift(&mut before[..]);
        assert_eq!(before, after);
    }

    #[test]
    fn test_pixel_nums() {
        assert_eq!(pixel_nums_to_braille(Some(0), Some(0)), '⣿');
        assert_eq!(pixel_nums_to_braille(Some(1), Some(2)), '⣦');
        assert_eq!(pixel_nums_to_braille(None, Some(3)), '⢀');
        assert_eq!(pixel_nums_to_braille(Some(2), None), '⡄');
        assert_eq!(pixel_nums_to_braille(None, None), '⠀');
    }

    #[test]
    fn test_draw_pixel_pair() {
        let mut term = Terminal::new().unwrap();

        // Test drawing with the same top cell
        draw_pixel_pair(&mut term, 0, 4, 6);
        assert_eq!(term[(0, term.rows() - 3)].ch(), ' ');
        assert_eq!(term[(0, term.rows() - 2)].ch(), '⣰');
        assert_eq!(term[(0, term.rows() - 1)].ch(), '⣿');
        term.clear().unwrap();

        // Test drawing with the top pixel in each column being in
        // different cells
        draw_pixel_pair(&mut term, 0, 4, 8);
        assert_eq!(term[(0, term.rows() - 4)].ch(), ' ');
        assert_eq!(term[(0, term.rows() - 3)].ch(), '⢀');
        assert_eq!(term[(0, term.rows() - 2)].ch(), '⣸');
        assert_eq!(term[(0, term.rows() - 1)].ch(), '⣿');
        term.clear().unwrap();

        draw_pixel_pair(&mut term, 1, 13, 2);
        assert_eq!(term[(1, term.rows() - 5)].ch(), ' ');
        assert_eq!(term[(1, term.rows() - 4)].ch(), '⡄');
        assert_eq!(term[(1, term.rows() - 3)].ch(), '⡇');
        assert_eq!(term[(1, term.rows() - 2)].ch(), '⡇');
        assert_eq!(term[(1, term.rows() - 1)].ch(), '⣷');
        term.clear().unwrap();
    }
}
