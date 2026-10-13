//! Opt-in AppKit/MediaPlayer fixture, without audio or server access.
#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::args().any(|arg| arg == "--native"),
        "Pass --native to publish synthetic Now Playing metadata for this check"
    );
    textamp::media::system_controls::verify_native()?;
    println!("Native metadata, duration, progress, playback rate, playing/paused state, Pause callback and stop cleanup passed");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("This fixture requires macOS")
}
