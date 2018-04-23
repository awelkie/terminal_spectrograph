use std::char;
use std::cmp::{max, min};
use std::collections::VecDeque;
use num::{Complex, Float};
use rustty::{Attr, Color, Terminal, Cell, CellAccessor, HasSize};
use rustty::ui::{Alignable, Widget, VerticalAlign, HorizontalAlign};
use itertools::{Itertools, EitherOrBoth};
use std::io;

pub struct Canvas {
    term: Terminal,
    spectrum: Widget,
    waterfall: Widget,
    history: VecDeque<Vec<f32>>,
}

impl Canvas {
    pub fn new() -> Result<Self, io::Error> {
        let term = try!(Terminal::new());

        let mut canvas = Canvas {
            term: term,
            spectrum: Widget::new(0, 0),
            waterfall: Widget::new(0, 0),
            history: VecDeque::new(),
        };

        canvas.resize();

        Ok(canvas)
    }

    fn resize(&mut self) {
        let (cols, rows) = self.term.size();
        let spectrum_height = rows / 2;
        let waterfall_height = if rows % 2 == 0 { rows / 2 } else { rows / 2 + 1 };

        self.spectrum = Widget::new(cols, spectrum_height);
        self.spectrum.align(&self.term, HorizontalAlign::Middle, VerticalAlign::Top, 0);

        self.waterfall = Widget::new(cols, waterfall_height);
        self.waterfall.align(&self.term, HorizontalAlign::Middle, VerticalAlign::Bottom, 0);

        self.history.reserve(waterfall_height * 2);
    }

    fn check_and_resize(&mut self) {
        let (cols, rows) = self.term.size();
        let (spectrum_cols, spectrum_rows) = self.spectrum.size();
        let (waterfall_cols, waterfall_rows) = self.waterfall.size();
        // if the terminal size has changed...
        if cols != spectrum_cols || cols != waterfall_cols ||
            rows != (spectrum_rows + waterfall_rows) {
            self.resize();
        }
    }

    /// Adds a spectrum to the history and draws it on the waterfall
    /// and the spectrum view.
    pub fn add_spectrum(&mut self, spec: Vec<Complex<f32>>) {
        let normalized = normalize_spectrum(&spec, 50.0);

        draw_spectrum(&mut self.spectrum, &normalized);

        // Since the waterfall has half the horizontal resolution of the spectrum view,
        // average every two values and store the averaged spectrum.
        let averaged = normalized.chunks(2).map(|v| (v[0] + v[1]) / 2.0).collect();

        // push spectrum onto the history
        self.history.push_front(averaged);
        let (_, rows) = self.waterfall.size();
        if self.history.len() >= rows * 2 {
            self.history.pop_back();
        }

        draw_waterfall(&mut self.waterfall, &self.history);

        self.spectrum.draw_into(&mut self.term);
        self.waterfall.draw_into(&mut self.term);
        self.term.swap_buffers().unwrap();

        self.check_and_resize();
    }

    pub fn get_term(&mut self) -> &mut Terminal {
        &mut self.term
    }

    pub fn get_spectrum_width(&self) -> usize {
        2 * self.term.cols()
    }
}

fn draw_waterfall<T: CellAccessor + HasSize>(canvas: &mut T, spectra: &VecDeque<Vec<f32>>) {
    let (cols, rows) = canvas.size();
    for (row, mut specs) in (0..rows).zip(&spectra.iter().chunks_lazy(2)) {
        let upper_heights = specs.next().into_iter().flat_map(|x| x);
        let lower_heights = specs.next().into_iter().flat_map(|x| x);
        for (c, heights) in (0..cols).zip(upper_heights.zip_longest(lower_heights)) {
            let (u, l) = match heights {
                EitherOrBoth::Both(&upper, &lower) => (upper, lower),
                EitherOrBoth::Left(&upper) => (upper, 0.0),
                EitherOrBoth::Right(&lower) => (0.0, lower),
            };
            *canvas.get_mut(c, row).unwrap() = spectrum_heights_to_waterfall_cell(u, l);
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
    let mapping = [16, 17, 18, 19, 21, 27, 33, 39, 45, 51,
                   50, 49, 48, 47, 46, 82, 118, 154, 190, 226];
    let idx = (f * (mapping.len() as f32)) as i32;
    if idx < 0 {
        mapping[0]
    } else if idx >= mapping.len() as i32 {
        mapping[mapping.len() - 1]
    } else {
        mapping[idx as usize]
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

fn draw_spectrum<T: CellAccessor + HasSize>(canvas: &mut T, spec: &[f32]) {
    canvas.clear(Cell::default());
    let (num_cols, num_rows) = canvas.size();
    let pixel_height = num_rows * 4;

    for (col_idx, chunk) in (0..num_cols).zip(spec.chunks(2)) {
        // height in float between 0 and 1.
        let h1 = chunk[0];
        let h2 = chunk[1];

        // The "pixel" height of each point.
        let p1 = (h1 * pixel_height as f32).floor().max(0.0) as usize;
        let p2 = (h2 * pixel_height as f32).floor().max(0.0) as usize;

        draw_pixel_pair(canvas, col_idx, p1, p2);
    }
}

#[cfg(test)]
mod tests {
    use super::{pixel_nums_to_braille, draw_pixel_pair};
    use rustty::Terminal;

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
