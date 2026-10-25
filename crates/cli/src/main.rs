use std::io::Write;
use std::path::PathBuf;

use anyhow::{Result, bail};
use calls_core::{CallId, Event, Library, Settings, Status, settings, transcript};
use calls_engines::{Doctor, default_pipeline};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "calls", about = "Transcribe call recordings locally")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import recordings and transcribe them
    Import { files: Vec<PathBuf> },
    /// Resume or retry processing a call
    Process { id: String },
    /// List calls in the library
    List,
    /// Print a call's transcript
    Show { id: String },
    /// Check that ffmpeg, whisper-cli and the models are installed
    Doctor,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings = Settings::load_or_default(&settings::settings_path())?;
    let library = Library::open(&settings.library_dir)?;

    match cli.command {
        Command::Import { files } => {
            if files.is_empty() {
                bail!("pass at least one file");
            }
            let ids = files.iter().map(|f| library.import(f).map(|c| c.id)).collect::<Result<Vec<_>>>()?;
            for id in ids {
                process(&library, &settings, &id)?;
            }
        }
        Command::Process { id } => process(&library, &settings, &CallId(id))?,
        Command::List => {
            for call in library.list()? {
                let status = match &call.status {
                    Status::Recording => "recording".to_string(),
                    Status::Pending => "pending".to_string(),
                    Status::Processing { step } => format!("processing: {step}"),
                    Status::Ready => "ready".to_string(),
                    Status::Failed { step, .. } => format!("failed: {step}"),
                };
                println!("{:<48} {status}", call.id);
            }
        }
        Command::Show { id } => {
            let path = library.dir(&CallId(id)).join(transcript::TRANSCRIPT_MD);
            print!("{}", std::fs::read_to_string(&path)?);
        }
        Command::Doctor => {
            let doctor = Doctor::check(&settings);
            println!("{doctor:#?}");
            if !doctor.is_ready() {
                bail!("setup incomplete, see README");
            }
        }
    }
    Ok(())
}

fn process(library: &Library, settings: &Settings, id: &CallId) -> Result<()> {
    let pipeline = default_pipeline(settings)?;
    eprintln!("{id}");
    pipeline.run(library, id, true, &|event| match event {
        Event::StepStarted { step, .. } => eprint!("  {step}..."),
        Event::StepProgress { step, fraction, .. } => {
            eprint!("\r  {step}... {:>3.0}%", fraction * 100.0);
            let _ = std::io::stderr().flush();
        }
        Event::StepPostponed { step, .. } => eprintln!("\r  {step}... postponed"),
        Event::StepFinished { step, .. } => eprintln!("\r  {step}... done"),
        Event::Failed { step, error, .. } => eprintln!("\r  {step}... failed\n{error}"),
        Event::Finished { .. } => eprintln!("  -> {}", library.dir(id).join(transcript::TRANSCRIPT_MD).display()),
    })
}
