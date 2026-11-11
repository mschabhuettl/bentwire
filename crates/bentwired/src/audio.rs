//! Sound output through rodio. One sound at a time: starting a sound stops the previous
//! one. Failures are logged and reported to the caller, never fatal.

use bentwire_character::Pcm;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamBuilder, Sink};

pub struct Audio {
    stream: OutputStream,
    sink: Option<Sink>,
}

impl Audio {
    /// None when no audio device can be opened (the daemon runs silently).
    pub fn open() -> Option<Audio> {
        match OutputStreamBuilder::open_default_stream() {
            Ok(mut stream) => {
                stream.log_on_drop(false);
                Some(Audio { stream, sink: None })
            }
            Err(e) => {
                log::warn!("no audio output: {e}");
                None
            }
        }
    }

    /// Returns false if the sound cannot be played.
    pub fn play(&mut self, pcm: &Pcm) -> bool {
        self.stop();
        if pcm.channels == 0 || pcm.rate == 0 {
            return false;
        }
        let samples: Vec<f32> = pcm.to_i16().iter().map(|s| *s as f32 / 32768.0).collect();
        let sink = Sink::connect_new(self.stream.mixer());
        sink.append(SamplesBuffer::new(pcm.channels, pcm.rate, samples));
        self.sink = Some(sink);
        true
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }
}
