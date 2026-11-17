//! Exercise only Tonal's own Windows SMTC session, with a silent fixture.
#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    let manager = Manager::RequestAsync()?.join()?;
    let started = Instant::now();
    let session = loop {
        let sessions = manager.GetSessions()?;
        let mut found = None;
        for index in 0..sessions.Size()? {
            let session = sessions.GetAt(index)?;
            if session
                .SourceAppUserModelId()?
                .to_string_lossy()
                .to_lowercase()
                .contains("tonal")
            {
                let properties = session.TryGetMediaPropertiesAsync()?.join()?;
                if !properties.Title()?.is_empty() {
                    found = Some(session);
                    break;
                }
            }
        }
        if let Some(session) = found {
            break session;
        }
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "Tonal must publish a Windows media session"
        );
        thread::sleep(Duration::from_millis(100));
    };
    let title = session.TryGetMediaPropertiesAsync()?.join()?.Title()?;
    assert!(session.TryPauseAsync()?.join()?);
    let start = Instant::now();
    while session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Paused {
        assert!(start.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(50));
    }
    assert!(session.TryChangePlaybackPositionAsync(30_000_000)?.join()?);
    let start = Instant::now();
    while session
        .GetTimelineProperties()?
        .Position()?
        .Duration
        .abs_diff(30_000_000)
        > 2_000_000
    {
        assert!(start.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(50));
    }
    assert!(session.TryPlayAsync()?.join()?);
    let start = Instant::now();
    while session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Playing {
        assert!(start.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(50));
    }
    assert!(session.TrySkipNextAsync()?.join()?);
    let start = Instant::now();
    while session.TryGetMediaPropertiesAsync()?.join()?.Title()? == title {
        assert!(start.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(50));
    }
    assert!(session.TryPauseAsync()?.join()?);
    thread::sleep(Duration::from_secs(7));
    assert_eq!(
        session.GetPlaybackInfo()?.PlaybackStatus()?,
        Status::Paused,
        "A pause while loading must survive completion"
    );
    println!("Windows SMTC: metadata, pause, seek, play and next passed.");
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
