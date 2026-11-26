//! Records two seconds from the default input, plays it back and saves it.
//!     cargo run -p yoin-engine --example smoke -- out.wav
use std::time::Duration;

fn main() -> Result<(), String> {
    let devices = yoin_engine::list_devices();
    println!(
        "inputs:  {:?}",
        devices.inputs.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    println!(
        "outputs: {:?}",
        devices.outputs.iter().map(|d| &d.name).collect::<Vec<_>>()
    );

    let mut engine = yoin_engine::Engine::new(None, None)?;
    println!("{:?}", engine.status());

    println!("recording 2s…");
    engine.record()?;
    std::thread::sleep(Duration::from_secs(2));
    engine.stop();
    let s = engine.status();
    println!("recorded {:.2}s, peak {}", s.length_secs, s.input_peak);

    println!("playing…");
    engine.play()?;
    while engine.status().transport == yoin_engine::Transport::Playing {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("done, {:?}", engine.status().error);

    if let Some(path) = std::env::args().nth(1) {
        engine.export_wav(path.as_ref())?;
        println!("saved {path}");
    }
    Ok(())
}
