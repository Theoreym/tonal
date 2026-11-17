//! This module is the only place allowed to create a WebView. No persistent web UI.
use crate::{auth_navigation::allowed as allowed_navigation, session::Session};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub fn login(
    cancelled: Arc<AtomicBool>,
    progress: impl Fn(&'static str) + Send + Sync + 'static,
) -> Result<Session, String> {
    #[cfg(windows)]
    {
        windows_login(cancelled, Arc::new(progress))
    }
    #[cfg(not(windows))]
    {
        let _ = (cancelled, progress);
        Err("La connexion WebView2 nécessite Windows.".into())
    }
}

fn login_phase(text: &str) -> &'static str {
    let Ok(url) = url::Url::parse(text) else {
        return "Ouverture de la fenêtre Google…";
    };
    match url.host_str() {
        Some("accounts.google.com") if url.path().contains("/challenge/") =>
            "Valide ton identité dans la fenêtre Google : clé d’accès, mot de passe ou méthode proposée par Google.",
        Some("consent.google.com" | "consent.youtube.com") =>
            "Confirme les choix demandés dans la fenêtre Google.",
        Some("music.youtube.com") => "Récupération de la session YouTube Music…",
        _ => "Connecte-toi dans la fenêtre Google avec ta clé d’accès ou ton mot de passe.",
    }
}

#[cfg(windows)]
fn windows_login(
    cancelled: Arc<AtomicBool>,
    progress: Arc<dyn Fn(&'static str) + Send + Sync>,
) -> Result<Session, String> {
    use std::time::{Duration, Instant};
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        platform::{
            run_return::EventLoopExtRunReturn,
            windows::{EventLoopBuilderExtWindows, WindowBuilderExtWindows},
        },
        window::WindowBuilder,
    };
    use wry::{NewWindowResponse, WebContext, WebViewBuilder, WebViewBuilderExtWindows};

    let diagnostics = crate::auth_diagnostics::Diagnostics::new();

    // This function runs on a dedicated STA thread, never on egui's event loop.
    // Unique data directory prevents reuse of the user's Edge/Chrome profile.
    let profile = tempfile::Builder::new()
        .prefix("tonal-login-")
        .tempdir()
        .map_err(|_| "Impossible de créer le profil de connexion temporaire.")?;
    let mut builder = EventLoopBuilder::new();
    builder.with_any_thread(true);
    let mut events = builder.build();
    let window = WindowBuilder::new()
        // Tao and Winit both default to "Window Class". Reusing Winit's registered
        // procedure with Tao's window data causes an access violation in-process.
        .with_window_classname("Tonal.GoogleLogin.Tao")
        .with_title("Tonal — Connexion Google (fermer pour annuler)")
        .with_inner_size(tao::dpi::LogicalSize::new(520.0, 720.0))
        .build(&events)
        .map_err(|_| "Impossible d'ouvrir la fenêtre de connexion.")?;
    let mut context = WebContext::new(Some(profile.path().to_path_buf()));
    let navigation_progress = progress.clone();
    let navigation_diagnostics = diagnostics.clone();
    let popup_diagnostics = diagnostics.clone();
    let popup_progress = progress.clone();
    let page_diagnostics = diagnostics.clone();
    let blocked_transfer = Arc::new(AtomicBool::new(false));
    let navigation_blocked_transfer = blocked_transfer.clone();
    progress("Ouverture de la fenêtre Google…");
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url("https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F")
        .with_background_color((0, 0, 0, 255))
        // A fresh temporary profile already isolates the session. InPrivate can
        // restrict passkey providers; keep normal WebAuthn behavior instead.
        .with_incognito(false)
        .with_autoplay(false)
        // Override Wry's defaults so Microsoft SmartScreen stays enabled.
        .with_additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI")
        .with_devtools(false)
        .with_general_autofill_enabled(false)
        .with_navigation_handler(move |url| {
            let allowed = allowed_navigation(&url);
            navigation_diagnostics.navigation(if allowed { "navigation-allowed" } else { "navigation-blocked" }, &url);
            if allowed { navigation_progress(login_phase(&url)); }
            else {
                navigation_progress("Une redirection a été bloquée. Le diagnostic de connexion permet d’en identifier la cause.");
                if url::Url::parse(&url).is_ok_and(|url| crate::auth_navigation::is_session_transfer(&url)) {
                    navigation_blocked_transfer.store(true, Ordering::Relaxed);
                }
            }
            allowed
        })
        .with_new_window_req_handler(move |url, _| {
            popup_diagnostics.navigation("popup-blocked", &url);
            popup_progress("Google a demandé une fenêtre supplémentaire, bloquée par Tonal. Diagnostic enregistré.");
            NewWindowResponse::Deny
        })
        .with_on_page_load_handler(move |event, url| {
            page_diagnostics.navigation(match event {
                wry::PageLoadEvent::Started => "page-started",
                wry::PageLoadEvent::Finished => "page-finished",
            }, &url);
        })
        .with_download_started_handler(|_, _| false)
        .build(&window)
        .map_err(|_| "Impossible de démarrer WebView2. Vérifie le runtime Microsoft Edge WebView2.")?;
    diagnostics.event("webview-ready");

    let started = Instant::now();
    let mut next_check = Instant::now();
    let mut next_health_check = Instant::now() + Duration::from_secs(10);
    let mut result = Err("Connexion annulée.".to_owned());
    events.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        if cancelled.load(Ordering::Relaxed) {
            diagnostics.event("login-cancelled");
            *flow = ControlFlow::Exit;
            return;
        }
        if blocked_transfer.load(Ordering::Relaxed) {
            diagnostics.event("session-transfer-blocked");
            result = Err("Une destination du transfert de session Google n’est pas autorisée par Tonal. Le diagnostic a été enregistré.".into());
            *flow = ControlFlow::Exit;
            return;
        }
        if started.elapsed() > Duration::from_secs(10 * 60) {
            result = Err("La connexion a expiré après 10 minutes. Réessaie.".into());
            *flow = ControlFlow::Exit;
            return;
        }
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *flow = ControlFlow::Exit,
            Event::MainEventsCleared if Instant::now() >= next_check => {
                next_check = Instant::now() + Duration::from_secs(1);
                if Instant::now() >= next_health_check {
                    next_health_check = Instant::now() + Duration::from_secs(15);
                    diagnostics.event("event-loop-alive");
                    if let Ok(url) = webview.url() { diagnostics.navigation("current-page", &url); }
                    let health_diagnostics = diagnostics.clone();
                    let _ = webview.evaluate_script_with_callback(
                        "({ready:document.readyState, online:navigator.onLine, secure:window.isSecureContext, webauthn:typeof PublicKeyCredential !== 'undefined'})",
                        move |result| health_diagnostics.script_health(&result));
                }
                // Only extract after returning to Music; never inspect the password DOM.
                let on_music = webview
                    .url()
                    .ok()
                    .and_then(|text| url::Url::parse(&text).ok())
                    .is_some_and(|url| {
                        url.scheme() == "https" && url.host_str() == Some("music.youtube.com")
                    });
                if on_music {
                    diagnostics.event("session-read-start");
                    match webview.cookies_for_url("https://music.youtube.com/") {
                        Ok(cookies) => {
                            diagnostics.event("session-read-complete");
                            if let Ok(session) =
                                Session::from_cookies(cookies.into_iter().map(|cookie| {
                                    (cookie.name().to_owned(), cookie.value().to_owned())
                                }))
                            {
                                result = Ok(session);
                                diagnostics.event("session-ready");
                                *flow = ControlFlow::Exit;
                            }
                        }
                        Err(_) => {
                            result = Err("Impossible de lire la session WebView2.".into());
                            *flow = ControlFlow::Exit;
                        }
                    }
                }
            }
            _ => {}
        }
    });
    diagnostics.event("event-loop-ended");
    if result.is_ok() {
        progress("Finalisation de la connexion Google…");
    }
    // Wry's Drop explicitly calls ICoreWebView2Controller::Close. Release all COM
    // references before waiting for the isolated profile's files to be unlocked.
    let _ = webview.clear_all_browsing_data();
    diagnostics.event("browsing-data-cleared");
    drop(webview);
    drop(context);
    drop(window);
    drop(events);
    let path = profile.keep();
    // WebView2 browser termination is asynchronous. Never kill shared Edge processes.
    for _ in 0..20 {
        if std::fs::remove_dir_all(&path).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    diagnostics.event("login-ended");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    #[test]
    #[ignore = "Validation manuelle Google ; saisie par l'utilisateur, aucune session remplacée ou enregistrée."]
    fn manual_google_identity_validation_without_replacing_saved_session() {
        let session = std::thread::spawn(|| {
            login(Arc::new(AtomicBool::new(false)), |phase| {
                eprintln!("{phase}")
            })
        })
        .join()
        .expect("Google login thread")
        .expect("Google login completed");
        let provider = crate::provider::YouTubeMusic::new(&session).expect("Native Music client");
        provider
            .validate_session(&session)
            .expect("Google session validated by Music");
        eprintln!("Connexion validée côté Music, sans modifier la session enregistrée.");
    }
    #[test]
    fn identity_progress_accepts_password_and_passkey_without_exposing_url_data() {
        let key = login_phase("https://accounts.google.com/v3/signin/challenge/pk?secret=test");
        let password =
            login_phase("https://accounts.google.com/v3/signin/challenge/pwd?secret=test");
        assert_eq!(key, password);
        assert!(!key.contains("secret"));
        assert!(key.contains("clé d’accès"));
        assert!(login_phase("https://music.youtube.com/").contains("session"));
    }
    #[test]
    fn login_navigation_stays_on_google_and_youtube_https_origins() {
        assert!(allowed_navigation("https://accounts.google.com/v3/signin"));
        assert!(allowed_navigation("https://music.youtube.com/"));
        for bad in [
            "http://accounts.google.com/",
            "https://accounts.google.com.evil.test/",
            "https://evil.test/?next=accounts.google.com",
            "file:///C:/",
            "https://user@accounts.google.com/",
        ] {
            assert!(!allowed_navigation(bad));
        }
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "Ouvre brièvement WebView2 puis annule, sans saisir d'identifiants."]
    fn temporary_webview_can_be_cancelled_twice() {
        for _ in 0..2 {
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let timer = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                flag.store(true, Ordering::Relaxed);
            });
            let result = std::thread::spawn(move || login(cancel, |_| {}))
                .join()
                .expect("Le thread WebView2 doit se fermer");
            timer.join().unwrap();
            match result {
                Ok(_) => panic!("Aucun compte ne doit être connecté dans un nouveau profil"),
                Err(error) => assert_eq!(error, "Connexion annulée."),
            }
        }
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "Ouvre l'interface native et deux connexions WebView2 annulées, sans identifiants."]
    fn login_coexists_with_native_window() {
        use eframe::egui;
        use std::{
            sync::{mpsc, Mutex},
            time::{Duration, Instant},
        };
        use winit::platform::windows::EventLoopBuilderExtWindows;

        type Outcome = Arc<Mutex<Option<Result<usize, String>>>>;
        struct SmokeApp {
            receiver: mpsc::Receiver<Result<usize, String>>,
            outcome: Outcome,
            started: Instant,
        }
        impl eframe::App for SmokeApp {
            fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
                let ctx = &root.ctx().clone();
                if self.outcome.lock().unwrap().is_some() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                egui::CentralPanel::default().show(root, |ui| {
                    ui.label("Test Tonal : ouverture et annulation de la connexion Google.");
                });
                let result = match self.receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("Thread de connexion interrompu".into()))
                    }
                    Err(mpsc::TryRecvError::Empty)
                        if self.started.elapsed() > Duration::from_secs(30) =>
                    {
                        Some(Err("Connexion bloquée après 30 secondes".into()))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                };
                if let Some(result) = result {
                    *self.outcome.lock().unwrap() = Some(result);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ctx.request_repaint_after(Duration::from_millis(25));
            }
        }
        let outcome: Outcome = Arc::default();
        let saved = outcome.clone();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([560.0, 120.0]),
            event_loop_builder: Some(Box::new(|builder| {
                builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "Tonal — test connexion",
            options,
            Box::new(move |_| {
                // The eframe/Winit window already exists before the auth thread starts.
                let (sender, receiver) = mpsc::channel();
                std::thread::spawn(move || {
                    let run = || -> Result<usize, String> {
                        for _ in 0..2 {
                            let cancelled = Arc::new(AtomicBool::new(false));
                            let timer_flag = cancelled.clone();
                            let timer = std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_secs(3));
                                timer_flag.store(true, Ordering::Relaxed);
                            });
                            let result = std::thread::spawn(move || login(cancelled, |_| {}))
                                .join()
                                .map_err(|_| "Panique dans le thread de connexion".to_owned())?;
                            timer
                                .join()
                                .map_err(|_| "Minuterie interrompue".to_owned())?;
                            match result {
                                Err(error) if error == "Connexion annulée." => {}
                                Err(error) => return Err(error),
                                Ok(_) => {
                                    return Err("Session inattendue dans un profil neuf".into())
                                }
                            }
                        }
                        Ok(2)
                    };
                    let _ = sender.send(run());
                });
                Ok(Box::new(SmokeApp {
                    receiver,
                    outcome,
                    started: Instant::now(),
                }))
            }),
        )
        .expect("L'interface native doit rester opérationnelle");
        assert_eq!(saved.lock().unwrap().as_ref(), Some(&Ok(2)));
    }
}
