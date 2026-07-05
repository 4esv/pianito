//! pianito - CLI Piano Tuner
//!
//! A terminal-based piano tuning application with guided coaching.

use std::time::Duration;

use clap::Parser;

use pianito::audio::{
    AudioOutput, AudioSource, MedianFilter, MicCapture, PitchDetector, WavAudioSource,
};
use pianito::config::{Args, Command, Config};
use pianito::tuning::notes::Note;
use pianito::tuning::session::Session;
use pianito::tuning::temperament::Temperament;
use pianito::ui::{self, App};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = Config::load();
    let effective = config.merge_with_args(&args);

    match args.command {
        Some(Command::Analyze { file }) => analyze_file(&file, effective.a4)?,
        Some(Command::Reference { note, duration }) => {
            play_reference(&note, duration, effective.a4)?
        }
        Some(Command::History) => show_history()?,
        Some(Command::Reset) => reset_sessions()?,
        None => run_interactive(effective)?,
    }

    Ok(())
}

/// Analyze a WAV file for pitch content.
fn analyze_file(path: &str, a4: f32) -> anyhow::Result<()> {
    println!("Analyzing {}...", path);

    let file = std::fs::File::open(path)?;
    let mut source = WavAudioSource::new(file)?;
    let sample_rate = source.sample_rate();

    let detector = PitchDetector::new(sample_rate);
    let temperament = Temperament::with_a4(a4);

    // Read samples in chunks and detect pitch
    let chunk_size = (sample_rate as usize) / 4; // 250ms chunks
    let mut buffer = vec![0.0f32; chunk_size];
    let mut detections = Vec::new();
    // NOTE: `nearest_note` returns `None` outside the 88-key span (A0-C8) -
    // report these instead of silently dropping them (issue #16); the
    // future partial analyzer will surface exactly this kind of content.
    let mut out_of_range = Vec::new();

    loop {
        let read = source.read_samples(&mut buffer);
        if read == 0 {
            break;
        }

        if let Ok(result) = detector.detect(&buffer[..read]) {
            match temperament.nearest_note(result.frequency) {
                Some((midi, cents)) => {
                    if let Some(note) = Note::from_midi(midi) {
                        detections.push((
                            result.frequency,
                            note.display_name(),
                            cents,
                            result.confidence,
                        ));
                    }
                }
                None => out_of_range.push(result.frequency),
            }
        }
    }

    if detections.is_empty() && out_of_range.is_empty() {
        println!("No pitch detected in file.");
    } else {
        if !detections.is_empty() {
            println!("\nDetected pitches:");
            println!(
                "{:<10} {:<8} {:<12} {:<10}",
                "Freq (Hz)", "Note", "Cents", "Confidence"
            );
            println!("{}", "-".repeat(42));

            for (freq, note, cents, confidence) in &detections {
                println!(
                    "{:<10.1} {:<8} {:+<12.1} {:<10.2}",
                    freq, note, cents, confidence
                );
            }

            // Summary
            let avg_freq: f32 =
                detections.iter().map(|(f, _, _, _)| f).sum::<f32>() / detections.len() as f32;
            if let Some((midi, cents)) = temperament.nearest_note(avg_freq) {
                if let Some(note) = Note::from_midi(midi) {
                    println!(
                        "\nAverage: {:.1} Hz ({} {:+.1} cents)",
                        avg_freq,
                        note.display_name(),
                        cents
                    );
                }
            }
        }

        if !out_of_range.is_empty() {
            println!(
                "\n{} detection(s) outside the 88-key range (A0-C8) were not scored:",
                out_of_range.len()
            );
            for freq in &out_of_range {
                println!("  {:.1} Hz", freq);
            }
        }
    }

    Ok(())
}

/// Play a reference tone for a given note.
fn play_reference(note_name: &str, duration: f32, a4: f32) -> anyhow::Result<()> {
    let note =
        Note::from_name(note_name).ok_or_else(|| anyhow::anyhow!("Unknown note: {}", note_name))?;

    let temperament = Temperament::with_a4(a4);
    let frequency = temperament.frequency(note.midi);

    println!(
        "Playing {} ({:.1} Hz) for {:.1}s...",
        note.display_name(),
        frequency,
        duration
    );

    let output = AudioOutput::new()?;
    output.play_sine(frequency, duration)?;

    // Wait for playback to complete
    std::thread::sleep(Duration::from_secs_f32(duration + 0.1));

    // Stream errors are parked (not printed) by the callback; report them
    // here where stderr is safe
    if let Some(err) = output.take_error() {
        eprintln!("Warning: audio output error: {}", err);
    }

    println!("Done.");
    Ok(())
}

/// Show tuning session history.
fn show_history() -> anyhow::Result<()> {
    let sessions = Session::list_all()?;

    if sessions.is_empty() {
        println!("No tuning sessions found.");
        return Ok(());
    }

    println!("Tuning History:");
    println!(
        "{:<24} {:<10} {:<12} {:<10}",
        "Date", "Mode", "Progress", "Avg. Cents"
    );
    println!("{}", "-".repeat(58));

    for session in sessions {
        let date = session.created_at.format("%Y-%m-%d %H:%M").to_string();
        let mode = format!("{:?}", session.mode);
        let progress = format!("{:.0}%", session.progress_percent());
        let avg_cents = format!("{:.1}", session.average_deviation());

        println!(
            "{:<24} {:<10} {:<12} {:<10}",
            date, mode, progress, avg_cents
        );
    }

    Ok(())
}

/// Reset (clear) all saved sessions.
fn reset_sessions() -> anyhow::Result<()> {
    print!("This will delete all saved tuning sessions. Continue? [y/N] ");
    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("y") {
        Session::reset_all()?;
        println!("All sessions cleared.");
    } else {
        println!("Cancelled.");
    }

    Ok(())
}

/// Run the interactive tuning application.
fn run_interactive(config: pianito::config::EffectiveConfig) -> anyhow::Result<()> {
    // Initialize audio capture
    let mut mic = match MicCapture::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: Could not access microphone: {}", e);
            eprintln!("Please ensure a microphone is connected and permissions are granted.");
            return Err(e.into());
        }
    };

    let sample_rate = mic.sample_rate();
    let detector = PitchDetector::new(sample_rate);

    // Create or resume app
    let mut app = if config.resume {
        match Session::load_recent()? {
            Some(session) => {
                println!(
                    "Resuming session from {}...",
                    session.created_at.format("%Y-%m-%d %H:%M")
                );
                std::thread::sleep(Duration::from_millis(500));
                App::with_session(session, &config)
            }
            None => {
                println!("No incomplete session found. Starting new session.");
                std::thread::sleep(Duration::from_millis(500));
                App::with_config(&config)
            }
        }
    } else {
        App::with_config(&config)
    };

    // Audio output for the lock beep (config/--beep). Opened before the TUI
    // so a missing output device degrades to a status-line warning instead
    // of failing mid-session.
    let beep_output = if config.beep {
        match AudioOutput::new() {
            Ok(output) => Some(output),
            Err(e) => {
                app.set_audio_warning(format!("Beep disabled (no audio output): {}", e));
                None
            }
        }
    } else {
        None
    };

    // NOTE: restore the terminal before the default panic handler runs,
    // otherwise the panic message prints into the vanishing alternate screen
    // and the shell is left in raw mode.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ui::restore();
        original_hook(info);
    }));

    // Initialize terminal
    let mut terminal = ui::init()?;

    // Main loop. Size the buffer to the largest (bass) analysis window; the
    // guided detector trims it to the current note's register per frame.
    let mut audio_buffer = vec![0.0f32; PitchDetector::max_window_samples(sample_rate)];

    // Reject isolated octave-off / glitch frames before they reach the meter.
    let mut pitch_filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);

    let result = loop {
        // Surface mic stream errors (e.g. device unplugged) in the UI
        // status line instead of writing to stderr while the terminal is
        // in raw mode
        if let Some(err) = mic.take_error() {
            app.set_audio_warning(format!("Audio input error: {}", err));
        }
        if let Some(output) = &beep_output {
            if let Some(err) = output.take_error() {
                app.set_audio_warning(format!("Audio output error: {}", err));
            }
        }

        // Read audio and detect pitch. When the app is guiding a specific
        // note, use the target-aware detector: it picks the register window
        // and clamps the search to +/-2 semitones. Otherwise (mode select,
        // calibration, profiling) fall back to the full-range detector.
        let read = mic.read_samples(&mut audio_buffer);
        if read > 0 {
            let slice = &audio_buffer[..read];
            let detection = match app.current_target_freq() {
                Some(target) => detector.detect_for_target(slice, target),
                None => detector.detect(slice),
            };
            match detection {
                Ok(pitch_result) => {
                    let smoothed = pitch_filter.push(pitch_result.frequency);
                    app.update_pitch(smoothed, pitch_result.confidence);
                }
                Err(_) => {
                    // Silence/lost detection re-arms the window so the next strike
                    // starts clean instead of blending across the gap.
                    pitch_filter.clear();
                    app.clear_pitch();
                }
            }
        }

        // Lock beep: the app requests at most one per strike
        if app.take_beep() {
            if let Some(output) = &beep_output {
                // Short 1 kHz blip - clearly a beep, not a piano partial
                let _ = output.play_sine(1000.0, 0.08);
            }
        }

        // Render UI (break instead of `?` so every exit path restores the
        // terminal below)
        if let Err(e) = terminal.draw(|frame| {
            app.render(frame);
        }) {
            break Err(e.into());
        }

        // Handle input (non-blocking)
        match ui::poll_event(Duration::from_millis(50)) {
            Ok(Some(event)) => {
                if let Some(key) = ui::is_key_press(&event) {
                    app.handle_key(key);
                }
            }
            Ok(None) => {}
            Err(e) => break Err(e.into()),
        }

        // Check for quit
        if app.should_quit() {
            break Ok(());
        }
    };

    // Restore terminal
    ui::restore()?;

    // Report any save failure now that stderr is usable again
    if let Some(err) = app.save_error() {
        eprintln!("Warning: {}", err);
    }

    result
}
