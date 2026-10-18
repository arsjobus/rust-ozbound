//! Ambient sound, generated in code so the game ships no audio files.
//!
//! Pure Rust (no Bevy). [`meadow_loop_wav`] returns a complete 16-bit mono WAV file: a soft
//! breathing wind with a few distant bird calls. It is deterministic (no random-number
//! dependency; a fixed pseudo-random sequence), quiet by construction (peak well below clipping),
//! and fades in and out at its edges so playing it on repeat never clicks.

/// Samples per second.
pub const SAMPLE_RATE: u32 = 22_050;
/// The loudest sample the generator produces, out of `i16::MAX`.
pub const PEAK: i16 = 9_000;
const FADE_SECONDS: f32 = 0.4;

/// Fixed pseudo-random sequence in `[-1, 1)` (xorshift), so the sound is the same every run.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Raw samples of the meadow ambience, `seconds` long (at least 1), each within `[-PEAK, PEAK]`.
pub fn meadow_samples(seconds: u32) -> Vec<i16> {
    let seconds = seconds.max(1);
    let total = (SAMPLE_RATE * seconds) as usize;
    let mut noise = Noise(0x9E37_79B9);
    let mut low = 0.0f32;
    let mut out = Vec::with_capacity(total);
    // Bird calls: short falling chirps at fixed fractions of the loop.
    let chirps: Vec<usize> = (0..seconds.div_ceil(5)).map(|i| ((i as f32 * 5.0 + 2.3) * SAMPLE_RATE as f32) as usize).collect();
    let fade = (FADE_SECONDS * SAMPLE_RATE as f32) as usize;
    for i in 0..total {
        let t = i as f32 / SAMPLE_RATE as f32;
        // Wind: low-passed noise, swelling a whole number of times over the loop.
        low += 0.02 * (noise.next() - low);
        let swell = 0.6 + 0.4 * (std::f32::consts::TAU * (seconds as f32 / 6.0).max(1.0).round() * t / seconds as f32).sin();
        let mut value = low * swell * 3.2;
        for &start in &chirps {
            if i >= start && i < start + SAMPLE_RATE as usize / 5 {
                let local = (i - start) as f32 / SAMPLE_RATE as f32;
                let freq = 3_200.0 - 5_000.0 * local;
                let envelope = (1.0 - local * 5.0).max(0.0) * (local * 60.0).min(1.0);
                value += 0.16 * envelope * (std::f32::consts::TAU * freq * local).sin();
            }
        }
        // Fade the edges so a repeat never clicks.
        let edge = i.min(total - 1 - i);
        let gain = if edge < fade { edge as f32 / fade as f32 } else { 1.0 };
        let sample = (value * gain).clamp(-1.0, 1.0) * f32::from(PEAK);
        out.push(sample as i16);
    }
    out
}

/// The same sound as a complete WAV file (RIFF header plus 16-bit little-endian mono PCM).
pub fn meadow_loop_wav(seconds: u32) -> Vec<u8> {
    wav_bytes(&meadow_samples(seconds))
}

/// Wraps mono 16-bit samples in a WAV container.
pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    #[test]
    fn the_wav_header_describes_the_data() {
        let wav = meadow_loop_wav(3);
        let samples = (SAMPLE_RATE * 3) as usize;
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32_at(&wav, 4) as usize, wav.len() - 8);
        assert_eq!(u16_at(&wav, 20), 1, "PCM");
        assert_eq!(u16_at(&wav, 22), 1, "mono");
        assert_eq!(u32_at(&wav, 24), SAMPLE_RATE);
        assert_eq!(u32_at(&wav, 28), SAMPLE_RATE * 2);
        assert_eq!(u16_at(&wav, 32), 2);
        assert_eq!(u16_at(&wav, 34), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32_at(&wav, 40) as usize, samples * 2);
        assert_eq!(wav.len(), 44 + samples * 2);
    }

    #[test]
    fn the_sound_is_deterministic() {
        assert_eq!(meadow_loop_wav(2), meadow_loop_wav(2));
    }

    #[test]
    fn the_sound_is_audible_but_quiet() {
        let samples = meadow_samples(10);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 500, "peak {peak} is too quiet to hear");
        assert!(peak <= PEAK.unsigned_abs(), "peak {peak} exceeds the limit");
        assert!(i32::from(PEAK) < i32::from(i16::MAX) / 3, "headroom against clipping");
        let mean_square = samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64;
        assert!(mean_square.sqrt() > 100.0, "the loop is mostly silence");
    }

    #[test]
    fn the_loop_starts_and_ends_silently_so_repeating_never_clicks() {
        let samples = meadow_samples(4);
        assert_eq!(samples[0], 0);
        assert!(samples[samples.len() - 1].abs() < 50, "last sample {}", samples[samples.len() - 1]);
        // And it is not silent in the middle.
        assert!(samples[samples.len() / 2..samples.len() / 2 + 2_000].iter().any(|s| s.abs() > 200));
    }

    #[test]
    fn the_length_follows_the_requested_seconds_and_never_collapses_to_nothing() {
        assert_eq!(meadow_samples(2).len(), (SAMPLE_RATE * 2) as usize);
        assert_eq!(meadow_samples(0).len(), SAMPLE_RATE as usize);
    }

    #[test]
    fn wav_bytes_round_trips_the_samples() {
        let wav = wav_bytes(&[0, 1, -1, i16::MAX, i16::MIN]);
        assert_eq!(wav.len(), 44 + 10);
        let read: Vec<i16> = wav[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(read, vec![0, 1, -1, i16::MAX, i16::MIN]);
        assert_eq!(wav_bytes(&[]).len(), 44);
    }
}
