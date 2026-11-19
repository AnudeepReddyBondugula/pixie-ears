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

// Live Kit wake word model for detecting wake words in audio streams.
use livekit_wakeword::WakeWordModel;

// Number of BYTES we read at a time.
// 2560 bytes / 2 bytes per sample = 1280 samples.
// At 16,000 samples per second, 1280 samples = 0.08 s = 80 ms of audio.
const BUCKET_SIZE: usize = 2560;

// Number of SAMPLES in one bucket (each i16 sample takes 2 bytes).
const SAMPLES_PER_CHUNK: usize = BUCKET_SIZE / 2;


fn main() {
    // Fixed-size buffer that holds one chunk of raw bytes.
    // Rust infers the type [u8; 2560] because read_exact() needs bytes.
    let mut bucket = [0; BUCKET_SIZE];

    // Lock stdin once and reuse the handle.
    // Locking every loop would be slower; locking once is the usual pattern.
    let mut handle = io::stdin().lock();

    // Loading the model with input sample rate
    let mut model = match WakeWordModel::new(&["hey_livekit.onnx"], 16000) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error initializing wake word model: {}", e);
            return;
        }
    };

    // Keep reading chunks until the stream ends or an error happens.
    loop {
        // read_exact() waits until the WHOLE bucket (2560 bytes) is filled.
        // It never gives us a half-filled bucket.
        match handle.read_exact(&mut bucket) {
            // Success: we have exactly one full chunk of audio.
            Ok(()) => {               
                
                let samples: Vec<i16> = bucket.chunks_exact(2).map(|chunk| {
                    let num: i16 = i16::from_le_bytes(chunk.try_into().unwrap());
                    return num;
                }).collect();

                let scores = match model.predict(&samples){
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error during wake word prediction: {}", e);
                        return;
                    }
                };

                if scores["hey_livekit"] > 0.5 {
                    println!("Wake word detected!");
                    match model.reset() {
                        Ok(_) => (),
                        Err(e) => {
                            eprintln!("Error resetting wake word model: {}", e);
                            return;
                        }
                    }
                }
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