use rustfft::{ FFT, FFTplanner };
use rustfft::num_complex::Complex;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};


struct SignalProcessor {
    fft: Arc<FFT<f32>>,
    signal: Vec<Complex<f32>>,
    fft_rate_hz: u32,
    sample_rate_hz: u32,
    pub fft_len: usize,
    num_samples_discarded: u32,
}

impl SignalProcessor {
    fn new(sample_rate_hz: u32, fft_rate_hz: u32, fft_len: usize) -> Self {
        let mut planner = FFTplanner::new(false);
        let fft = planner.plan_fft(fft_len);

        SignalProcessor {
            fft: fft,
            signal: Vec::with_capacity(fft_len),
            fft_rate_hz: fft_rate_hz,
            sample_rate_hz: sample_rate_hz,
            fft_len: fft_len,
            num_samples_discarded: 0,
        }
    }

    fn new_fft_len(&mut self, fft_len: usize) {
        let mut planner = FFTplanner::new(false);
        self.fft = planner.plan_fft(fft_len);

        self.signal.reserve(fft_len);
        self.fft_len = fft_len;
    }

    fn add_signal_buffer(&mut self, buff: Vec<Complex<i8>>) -> Vec<Vec<Complex<f32>>> {
        let num_samples_to_discard = (self.sample_rate_hz -
            self.fft_rate_hz * self.fft_len as u32) / self.fft_rate_hz;
        let mut spectra = Vec::new();
        for x in buff {
            if self.num_samples_discarded >= num_samples_to_discard {
                self.signal.push(Complex::new(x.re as f32, x.im as f32));

                if self.signal.len() >= self.fft_len {
                    let mut spectrum = vec![Complex::new(0.0, 0.0); self.fft_len];
                    self.fft.process(&mut self.signal[..], &mut spectrum[..]);
                    self.signal.clear();
                    self.num_samples_discarded = 0;
                    spectra.push(spectrum);
                }
            } else {
                // discard these samples to maintain the desired FFT rate.
                self.num_samples_discarded += 1;
            }
        }
        spectra
    }
}

pub fn process_signal(recv: Receiver<Vec<Complex<i8>>>, send: SyncSender<Vec<Complex<f32>>>,
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

        let spectra = processor.add_signal_buffer(buff);

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
