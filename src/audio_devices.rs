//! Event-driven Windows default-output changes. No device selector or periodic enumeration.
use eframe::egui;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct Watcher {
    revision: Arc<AtomicU64>,
    observed: u64,
    due: Option<Instant>,
    attempts: u8,
    #[cfg(windows)]
    _stop: std::sync::mpsc::Sender<()>,
}
impl Watcher {
    #[cfg(test)]
    pub fn notify_test_change(&self) {
        self.revision.fetch_add(1, Ordering::Release);
    }
    pub fn new(ctx: &egui::Context) -> Option<Self> {
        let revision = Arc::new(AtomicU64::new(0));
        #[cfg(windows)]
        let stop = platform::watch(ctx.clone(), revision.clone())?;
        #[cfg(not(windows))]
        let _ = ctx;
        Some(Self {
            revision,
            observed: 0,
            due: None,
            attempts: 0,
            #[cfg(windows)]
            _stop: stop,
        })
    }
    pub fn ready(&mut self, ctx: &egui::Context) -> bool {
        let revision = self.revision.load(Ordering::Acquire);
        if revision != self.observed {
            self.observed = revision;
            self.attempts = 0;
            self.due = Some(Instant::now() + Duration::from_millis(180));
        }
        if let Some(due) = self.due {
            if Instant::now() >= due {
                self.due = None;
                return true;
            }
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        false
    }
    pub fn retry(&mut self, ctx: &egui::Context) -> bool {
        self.attempts += 1;
        if self.attempts >= 5 {
            return false;
        }
        let delay = Duration::from_millis(250 * (1 << self.attempts));
        self.due = Some(Instant::now() + delay);
        ctx.request_repaint_after(delay);
        true
    }
    pub fn succeeded(&mut self) {
        self.attempts = 0;
        self.due = None;
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::{
        core::{implement, Result, PCWSTR},
        Win32::{
            Foundation::PROPERTYKEY,
            Media::Audio::{
                eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
                IMMNotificationClient_Impl, MMDeviceEnumerator, DEVICE_STATE,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
            },
        },
    };

    #[implement(IMMNotificationClient)]
    struct Notification {
        revision: Arc<AtomicU64>,
        ctx: egui::Context,
    }
    impl IMMNotificationClient_Impl for Notification_Impl {
        fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> Result<()> {
            // CPAL WASAPI opens eRender/eConsole, not the communications endpoint.
            if flow == eRender && role == eConsole {
                self.revision.fetch_add(1, Ordering::Release);
                self.ctx.request_repaint();
            }
            Ok(())
        }
        fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
            Ok(())
        }
        fn OnDeviceAdded(&self, _id: &PCWSTR) -> Result<()> {
            Ok(())
        }
        fn OnDeviceRemoved(&self, _id: &PCWSTR) -> Result<()> {
            Ok(())
        }
        fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> Result<()> {
            Ok(())
        }
    }
    pub fn watch(
        ctx: egui::Context,
        revision: Arc<AtomicU64>,
    ) -> Option<std::sync::mpsc::Sender<()>> {
        let (stop, receiver) = std::sync::mpsc::channel();
        let (ready, started) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                let _ = ready.send(false);
                return;
            }
            let registration = (|| -> Result<(IMMDeviceEnumerator, IMMNotificationClient)> {
                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let notification: IMMNotificationClient = Notification { revision, ctx }.into();
                enumerator.RegisterEndpointNotificationCallback(&notification)?;
                Ok((enumerator, notification))
            })();
            let _ = ready.send(registration.is_ok());
            if let Ok((enumerator, notification)) = registration {
                // A disconnected sender wakes this parked thread on shutdown.
                let _ = receiver.recv();
                let _ = enumerator.UnregisterEndpointNotificationCallback(&notification);
            }
            CoUninitialize();
        });
        started
            .recv_timeout(Duration::from_secs(2))
            .ok()
            .filter(|ready| *ready)
            .map(|_| stop)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn only_the_default_playback_role_wakes_the_output_router() {
            use windows::Win32::Media::Audio::{eCapture, eCommunications, eMultimedia};
            let revision = Arc::new(AtomicU64::new(0));
            let notification: IMMNotificationClient = Notification {
                revision: revision.clone(),
                ctx: egui::Context::default(),
            }
            .into();
            unsafe {
                notification
                    .OnDefaultDeviceChanged(eCapture, eConsole, PCWSTR::null())
                    .unwrap();
                notification
                    .OnDefaultDeviceChanged(eRender, eCommunications, PCWSTR::null())
                    .unwrap();
                notification
                    .OnDefaultDeviceChanged(eRender, eMultimedia, PCWSTR::null())
                    .unwrap();
                assert_eq!(revision.load(Ordering::Acquire), 0);
                notification
                    .OnDefaultDeviceChanged(eRender, eConsole, PCWSTR::null())
                    .unwrap();
                assert_eq!(revision.load(Ordering::Acquire), 1);
            }
        }
        #[test]
        #[ignore = "Windows interactif : inscrit puis libère la notification réelle, sans changer le périphérique Windows."]
        fn register_and_release_real_endpoint_notifications() {
            let watcher = Watcher::new(&egui::Context::default())
                .expect("Core Audio notifications must register");
            let weak = Arc::downgrade(&watcher.revision);
            drop(watcher);
            let deadline = Instant::now() + Duration::from_secs(2);
            while weak.upgrade().is_some() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                weak.upgrade().is_none(),
                "Notification registration must release on shutdown"
            );
        }
    }
}
