// pixie-ears, Step 1: a simple volume meter.
// Reads raw audio from stdin (16 kHz, mono, 16-bit little-endian, "s16le"),
// cuts it into 80 ms chunks, measures how loud each chunk is (RMS),
// and prints one line per chunk with a bar of '#' characters.
//
// Data flow:
//   mic -> recorder (FFmpeg / parec) -> stdin -> [this program] -> terminal
//
// One chunk of bytes in memory:
//   [lo hi][lo hi][lo hi] ...  -> 2560 bytes
//    smp 0  smp 1  smp 2   ...  -> 1280 samples (each sample = 2 bytes)

// io        = input/output tools (stdin)
// UnexpectedEof = the error kind for "stream ended before the buffer was full"
// Read      = the trait that gives us .read_exact()
use std::io::{self, ErrorKind::UnexpectedEof, Read};

// Number of BYTES we read at a time.
// 2560 bytes / 2 bytes per sample = 1280 samples.
// At 16,000 samples per second, 1280 samples = 0.08 s = 80 ms of audio.
const BUCKET_SIZE: usize = 2560;

// Number of SAMPLES in one bucket (each i16 sample takes 2 bytes).
const SAMPLES_PER_CHUNK: usize = BUCKET_SIZE / 2;

// Scales loudness down to a bar length: every 200 units of RMS = one '#'.
// Make it smaller for a more sensitive meter, bigger for a calmer one.
const BAR_DIVISOR: usize = 200;

// Longest bar we ever print, so loud sounds don't flood the terminal.
const MAX_BAR_LENGTH: usize = 60;

fn main() {
    // Fixed-size buffer that holds one chunk of raw bytes.
    // Rust infers the type [u8; 2560] because read_exact() needs bytes.
    let mut bucket = [0; BUCKET_SIZE];

    // Counts how many chunks we have processed (used as a line number).
    let mut chunk_count = 0;

    // Lock stdin once and reuse the handle.
    // Locking every loop would be slower; locking once is the usual pattern.
    let mut handle = io::stdin().lock();

    // Keep reading chunks until the stream ends or an error happens.
    loop {
        // read_exact() waits until the WHOLE bucket (2560 bytes) is filled.
        // It never gives us a half-filled bucket.
        match handle.read_exact(&mut bucket) {
            // Success: we have exactly one full chunk of audio.
            Ok(()) => {
                chunk_count += 1;

                // Running total of (sample * sample) for this chunk.
                // f64 is used because the squares get very large
                // (32767 * 32767 is about 1 billion).
                let mut sum_sq: f64 = 0.0;

                // Walk through the bytes two at a time: each pair is one sample.
                for chunk in bucket.chunks_exact(2) {
                    // Turn 2 bytes into one signed 16-bit number.
                    // "le" = little-endian: low byte first, then high byte.
                    // try_into() converts the &[u8] slice into a [u8; 2] array;
                    // unwrap() is safe here because chunks_exact(2) always gives 2 bytes.
                    let num: i16 = i16::from_le_bytes(chunk.try_into().unwrap());

                    // Convert to f64 so we can square it without overflow.
                    let s = num as f64;

                    // Square it: negative and positive values both become positive,
                    // so the wave going up and down doesn't cancel out.
                    sum_sq = sum_sq + s * s;
                }

                // RMS = Root Mean Square = "average loudness" of the chunk:
                //   1. Square every sample   (done in the loop above)
                //   2. Take the Mean         (divide by the number of samples)
                //   3. Take the square Root  (bring it back to the sample scale)
                let mean_sq = sum_sq / SAMPLES_PER_CHUNK as f64;
                let rms = mean_sq.sqrt();

                // Turn loudness into a bar length (number of '#'), then cap it.
                let mut bar_len = (rms / BAR_DIVISOR as f64) as usize;
                bar_len = bar_len.min(MAX_BAR_LENGTH);

                // Print: chunk number (5 wide), RMS value (6 wide, no decimals), bar.
                // Example line:   42    1830 : #########
                println!("{:5.0} {:6.0} : {}", chunk_count, rms, "#".repeat(bar_len));
            }

            // The stream ended before a full bucket could be read.
            // This is what happens normally when the recorder stops or you press Ctrl+C.
            // Any leftover bytes (less than 2560) are dropped.
            Err(e) if e.kind() == UnexpectedEof => {
                eprintln!("Unexpected EOF reached. Stream ended before reading {} bytes.", BUCKET_SIZE);
                break;
            }

            // Any other read error (for example, a broken pipe). Report it and stop.
            // eprintln! writes to stderr, so error messages don't mix with the meter output.
            Err(e) => {
                eprintln!("Unknown error reading from stdin: {}", e);
                break;
            }
        }
    }
}