use std::char;
use std::cmp::{max, min};
use num::Complex;
use rustty::{Terminal, Cell, Style, Attr};

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
    for (i, x) in spec.iter().map(|x| x.norm()).enumerate() {
        if (i * dest.len() / spec.len()) as isize > last_idx {
            last_idx += 1;
            dest[last_idx as usize] = x;
        }
    }

    //TODO unnecessary allocation
    fft_shift(dest);
}

fn char_to_cell(c: char) -> Cell {
    Cell::new(c, Style::with_attr(Attr::Bold), Style::with_attr(Attr::Default))
}

fn draw_pixel_pair(term: &mut Terminal, col_idx: usize, p1: usize, p2: usize) {
    let max_pixel_height = 4 * term.rows();

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
    for row_idx in max(c1, c2)..term.rows() {
        term[(col_idx, row_idx)] = char_to_cell(full_cell_char);
    }

    let left_fill_cell_char = pixel_nums_to_braille(Some(0), None);
    for row_idx in min(c1, c2)..c2 {
        term[(col_idx, row_idx)] = char_to_cell(left_fill_cell_char);
    }

    let right_fill_cell_char = pixel_nums_to_braille(None, Some(0));
    for row_idx in min(c1, c2)..c1 {
        term[(col_idx, row_idx)] = char_to_cell(right_fill_cell_char);
    }

    // Now fill in partial height cells.
    if c1 == c2 {
        // top pixels are in the same cell
        term[(col_idx, c1)] = char_to_cell(
            pixel_nums_to_braille(Some((p1 % 4) as u8), Some((p2 % 4) as u8)));
    } else if c1 > c2 {
        // right pixel is in a higher cell.
        term[(col_idx, c1)] = char_to_cell(
            pixel_nums_to_braille(Some((p1 % 4) as u8), Some(0)));
        term[(col_idx, c2)] = char_to_cell(
            pixel_nums_to_braille(None, Some((p2 % 4) as u8)));
    } else {
        // left pixel is in a higher cell.
        term[(col_idx, c1)] = char_to_cell(
            pixel_nums_to_braille(Some((p1 % 4) as u8), None));
        term[(col_idx, c2)] = char_to_cell(
            pixel_nums_to_braille(Some(0), Some((p2 % 4) as u8)));
    }
}

pub fn draw_spectrum(term: &mut Terminal, spec: Vec<Complex<f32>>) {
    term.clear().unwrap();
    let (num_cols, num_rows) = term.size();
    let pixel_height = num_rows * 4;
    let pixel_width = num_cols * 2;
    // TODO what should this value be?
    let max_height = 500.0;

    let mut bins = vec![0.0; pixel_width];
    spectrum_to_bin_heights(&spec[..], &mut bins[..]);

    for col_idx in 0..num_cols {
        // height in float between 0 and 1.
        let h1 = bins[col_idx * 2] / max_height;
        let h2 = bins[col_idx * 2 + 1] / max_height;

        // The "pixel" height of each point.
        let p1 = (h1 * pixel_height as f32).floor() as usize;
        let p2 = (h2 * pixel_height as f32).floor() as usize;

        draw_pixel_pair(term, col_idx, p1, p2);
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
