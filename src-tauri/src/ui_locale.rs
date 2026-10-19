//! Global interface language only. This state never changes a bot profile,
//! its spoken language, permissions or any runtime instruction document.
use std::sync::Mutex;
use tauri::{menu::{Menu, MenuItem}, Emitter};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiLocale { De, #[default] En, Fr, Es }

impl UiLocale {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "de" => Ok(Self::De), "en" => Ok(Self::En),
            "fr" => Ok(Self::Fr), "es" => Ok(Self::Es),
            _ => Err("Ungültige Oberflächensprache. Erlaubt sind de, en, fr und es.".into()),
        }
    }

    fn code(self) -> &'static str {
        match self { Self::De => "de", Self::En => "en", Self::Fr => "fr", Self::Es => "es" }
    }

    fn labels(self) -> [&'static str; 4] {
        match self {
            Self::De => ["Studio anzeigen", "Charakter anzeigen", "Sprechen abbrechen", "OrbitPal beenden"],
            Self::En => ["Show studio", "Show character", "Stop speaking", "Quit OrbitPal"],
            Self::Fr => ["Afficher le studio", "Afficher le personnage", "Arrêter la parole", "Quitter OrbitPal"],
            Self::Es => ["Mostrar estudio", "Mostrar personaje", "Detener voz", "Salir de OrbitPal"],
        }
    }
}

/// The frontend restores the validated localStorage preference at startup.
/// Until then, native menus use English. Nothing is persisted by this module.
#[derive(Default)]
pub struct UiLocaleState(Mutex<UiLocale>);

pub fn tray_menu(app: &tauri::AppHandle, language: UiLocale) -> tauri::Result<Menu<tauri::Wry>> {
    let [studio, character, stop, quit] = language.labels();
    let studio = MenuItem::with_id(app, "studio", studio, true, None::<&str>)?;
    let character = MenuItem::with_id(app, "character", character, true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", stop, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    Menu::with_items(app, &[&studio, &character, &stop, &quit])
}

fn studio(label: &str) -> Result<(), String> {
    if label == "main" { Ok(()) }
    else { Err("Die Oberflächensprache wird ausschließlich im Studio geändert".into()) }
}

#[tauri::command]
pub fn set_ui_locale(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: tauri::State<'_, UiLocaleState>,
    language: String,
) -> Result<(), String> {
    studio(window.label())?;
    let language = UiLocale::parse(&language)?;
    // Serialize menu replacement and the published value. A failed menu update
    // must not announce a locale which the native controls did not adopt.
    let mut current = state.0.lock().map_err(|_| "Die Oberflächensprache ist nicht verfügbar".to_owned())?;
    if let Some(tray) = app.tray_by_id("cohost") {
        let menu = tray_menu(&app, language).map_err(|_| "Das Sprachmenü konnte nicht erstellt werden".to_owned())?;
        tray.set_menu(Some(menu)).map_err(|_| "Die Sprache des Systemmenüs konnte nicht aktualisiert werden".to_owned())?;
    }
    *current = language;
    app.emit("cohost://ui-locale", language.code()).map_err(|_| "Die Oberflächensprache konnte nicht an die Fenster übertragen werden".to_owned())
}

/// A newly opened native overlay can adopt the current global UI language.
/// Reading it grants no capture, microphone, update or bot permissions.
#[tauri::command]
pub fn get_ui_locale(state: tauri::State<'_, UiLocaleState>) -> Result<String, String> {
    let language = state.0.lock().map_err(|_| "Die Oberflächensprache ist nicht verfügbar".to_owned())?;
    Ok(language.code().to_owned())
}
