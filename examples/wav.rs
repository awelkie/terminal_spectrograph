extern crate cpal;
extern crate hound;
extern crate terminal_spectrograph;

use terminal_spectrograph::{Canvas, SignalProcessor, Complex, Event};

use std::env;
use std::mem;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{sync_channel};


struct PlayBack<R: Read> {
    spec: hound::WavSpec,
    samples: hound::WavIntoSamples<R, i32>,
}

impl<R: Read> Iterator for PlayBack<R> {
    // type Item = Result<i32, hound::Error>;
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.spec.channels == 1 {
            match self.samples.next() {
                Some(Ok(sample)) => Some(sample),
                Some(Err(_)) => None,
                None => None,
            }
        } else if self.spec.channels > 1 {
            let mut buffer: Vec<i32> = vec![0i32; self.spec.channels as usize];
            for _ in 0..self.spec.channels {
                match self.samples.next() {
                    Some(Ok(sample)) => buffer.push(sample),
                    Some(Err(_)) => return None,
                    None => return None,
                }
            }
            
            let sample = buffer.iter().sum::<i32>() / self.spec.channels as i32;

            Some(sample)
        } else {
            unreachable!()
        }
    }
}

fn main () {
    println!("

Usage:

# Remix
$ ffmpeg -i input.mp4 -ac 1 output.wav

# Raw
ffmpeg -i input.mp4 output.wav

");
    let filename = env::args().nth(1).expect("play ./sample.wav");

    let wav_reader = hound::WavReader::open(filename).expect("Only Support WAV Format Audio!");
    let wav_spec = wav_reader.spec();
    println!("WAV Reader Spec: {:?}", wav_spec );
    
    assert_eq!(wav_spec.channels == 1 || wav_spec.channels == 2, true);
    assert_eq!(wav_spec.bits_per_sample, 16);
    assert_eq!(wav_spec.sample_format, hound::SampleFormat::Int);

    let device = cpal::default_output_device().expect("Failed to get default output device");
    let format = device.default_output_format().expect("Failed to get default output format");
    let event_loop = cpal::EventLoop::new();
    let sample_rate: u32 = format.sample_rate.0;

    println!("device name: {:?}", device.name() );
    println!("device default output format: {:?}", format);
    println!("supported_output_formats: {:?}", device.supported_output_formats().unwrap().collect::<Vec<cpal::SupportedFormat>>() );

    let stream_id = event_loop.build_output_stream(&device, &format).unwrap();
    event_loop.play_stream(stream_id.clone());

    let (spec_send, spec_recv) = sync_channel::<i32>(1024);
    let stop = Arc::new(Mutex::new(false));
    let stop_clone = stop.clone();

    let mut playback = PlayBack {
        spec: wav_spec,
        samples: wav_reader.into_samples::<i32>(),
    };

    // let mut samples = wav_reader.into_samples::<i16>()
    //             .map(|sample| sample.expect("failed to decode WAV stream"));

    std::thread::spawn(move || {
        event_loop.run(move |_stream_id, stream_data| {
            match stream_data {
                cpal::StreamData::Output { buffer: cpal::UnknownTypeOutputBuffer::F32(mut buffer) } => {
                    for channels in buffer.chunks_mut(format.channels as usize) {
                        let sample: i32 = match playback.next() {
                            Some(sample) => sample,
                            None => {
                                *stop_clone.lock().unwrap() = true;
                                0
                            }
                        };

                        let _ = spec_send.try_send(sample);

                        let mut sample: f64 = sample as f64 / (std::i16::MAX as f64);

                        for channel in channels.iter_mut() {
                            *channel = sample as f32;
                        }
                    }
                },
                _ => unreachable!(),
            };
        });
    });
    
    let stop_clone2 = stop.clone();
    std::thread::spawn(move || {
        let mut canvas = Canvas::new().expect("Error opening terminal");
        let mut fft_len = canvas.get_spectrum_width();
        
        let fft_rate = 25;
        let mut sp = SignalProcessor::new(sample_rate, fft_rate, fft_len);

        for sample in spec_recv.iter() {
            let sample = unsafe { mem::transmute::<i32, Complex<i16>>(sample) };
            let sample = Complex::new(sample.re as f32, sample.im as f32);
            let samples = vec![ sample ];

            let spectra = sp.add_signal_buffer(samples);

            #[allow(unused_assignments)]
            for spectrum in spectra {
                canvas.add_spectrum(spectrum);

                if let Ok(Some(Event::Key('q'))) = canvas.get_term().get_event(std::time::Duration::from_secs(0)) {
                    *stop_clone2.lock().unwrap() = true;
                    break;
                }

                fft_len = canvas.get_spectrum_width();
            }
        }
    });

    loop {

        if *stop.lock().unwrap() == true {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}