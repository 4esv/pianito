//! onkey - CLI Piano Tuner
//!
//! A terminal-based piano tuning application with guided coaching.

use std::time::Duration;

use clap::Parser;

use onkey::audio::{AudioOutput, AudioSource, MicCapture, PitchDetector, WavAudioSource};
use onkey::config::{Args, Command, Config};
use onkey::tuning::notes::Note;
use onkey::tuning::session::Session;
use onkey::tuning::temperament::Temperament;
use onkey::ui::{self, App};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = Config::load();
    let effective = config.merge_with_args(&args);

    match args.command {
        Some(Command::Analyze { file }) => analyze_file(&file)?,
        Some(Command::Reference { note, duration }) => play_reference(&note, duration)?,
        Some(Command::History) => show_history()?,
        Some(Command::Reset) => reset_sessions()?,
        None => run_interactive(effective)?,
    }

    Ok(())
}

/// Analyze a WAV file for pitch content.
fn analyze_file(path: &str) -> anyhow::Result<()> {
    println!("Analyzing {}...", path);

    let file = std::fs::File::open(path)?;
    let mut source = WavAudioSource::new(file)?;
    let sample_rate = source.sample_rate();

    let detector = PitchDetector::new(sample_rate);
    let temperament = Temperament::new();

    // Read samples in chunks and detect pitch
    let chunk_size = (sample_rate as usize) / 4; // 250ms chunks
    let mut buffer = vec![0.0f32; chunk_size];
    let mut detections = Vec::new();

    loop {
        let read = source.read_samples(&mut buffer);
        if read == 0 {
            break;
        }

        if let Some(result) = detector.detect(&buffer[..read]) {
            let (midi, cents) = temperament.nearest_note(result.frequency);
            if let Some(note) = Note::from_midi(midi) {
                detections.push((
                    result.frequency,
                    note.display_name(),
                    cents,
                    result.confidence,
                ));
            }
        }
    }

    if detections.is_empty() {
        println!("No pitch detected in file.");
    } else {
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
        if !detections.is_empty() {
            let avg_freq: f32 =
                detections.iter().map(|(f, _, _, _)| f).sum::<f32>() / detections.len() as f32;
            let (midi, cents) = temperament.nearest_note(avg_freq);
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

    Ok(())
}

/// Play a reference tone for a given note.
fn play_reference(note_name: &str, duration: f32) -> anyhow::Result<()> {
    let note =
        Note::from_name(note_name).ok_or_else(|| anyhow::anyhow!("Unknown note: {}", note_name))?;

    let temperament = Temperament::new();
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
fn run_interactive(config: onkey::config::EffectiveConfig) -> anyhow::Result<()> {
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
                App::with_session(session)
            }
            None => {
                println!("No incomplete session found. Starting new session.");
                std::thread::sleep(Duration::from_millis(500));
                App::new()
            }
        }
    } else {
        App::new()
    };

    // Initialize terminal
    let mut terminal = ui::init()?;

    // Audio output for reference tones
    let audio_output = AudioOutput::new().ok();

    // Main loop
    let mut audio_buffer = vec![0.0f32; sample_rate as usize / 10]; // 100ms buffer

    let result = loop {
        // Read audio and detect pitch
        let read = mic.read_samples(&mut audio_buffer);
        if read > 0 {
            if let Some(pitch_result) = detector.detect(&audio_buffer[..read]) {
                app.update_pitch(pitch_result.frequency, pitch_result.confidence);
            } else {
                app.clear_pitch();
            }
        }

        // Handle reference tone
        if app.is_playing_reference() {
            if let (Some(ref output), Some(freq)) = (&audio_output, app.current_target_freq()) {
                // Play short burst of reference tone
                let _ = output.play_sine(freq, 0.1);
            }
        }

        // Render UI
        terminal.draw(|frame| {
            app.render(frame);
        })?;

        // Handle input (non-blocking)
        if let Some(event) = ui::poll_event(Duration::from_millis(50))? {
            if let Some(key) = ui::is_key_press(&event) {
                app.handle_key(key);
            }
        }

        // Check for quit
        if app.should_quit() {
            break Ok(());
        }
    };

    // Restore terminal
    ui::restore()?;

    result
}
