//! Explicit native credential stores, retaining the pre-OrbitPal namespace.
//! Construct stores directly: changing a process-global default can redirect
//! another bot's in-flight credential access.
use keyring_core::{api::CredentialStoreApi, Entry, Error};

pub(crate) const SERVICE: &str = "streamer-ai-cohost";

#[cfg(not(test))]
pub(crate) fn entry(id: &str) -> keyring_core::Result<Entry> {
    #[cfg(target_os = "macos")]
    return apple_native_keyring_store::keychain::Store::new()?.build(SERVICE, id, None);

    #[cfg(target_os = "windows")]
    return windows_native_keyring_store::Store::new()?.build(SERVICE, id, None);

    #[cfg(target_os = "linux")]
    {
        // keyring 3 wrote target=default, not merely service/username. Preserve
        // that attribute for both forward access and rollback to an older app.
        let modifiers = std::collections::HashMap::from([("target", "default")]);
        dbus_secret_service_keyring_store::Store::new()?.build(SERVICE, id, Some(&modifiers))
    }
}

// Ordinary tests never open the developer's OS store. The opt-in, disposable
// three-OS fixture compiles this module as normal code and checks native stores.
#[cfg(test)]
pub(crate) fn entry(id: &str) -> keyring_core::Result<Entry> {
    thread_local! {
        static STORE: std::sync::Arc<keyring_core::mock::Store> =
            keyring_core::mock::Store::new().expect("in-memory test store");
    }
    STORE.with(|store| store.build(SERVICE, id, None))
}

pub(crate) fn read(id: &str) -> Result<Option<String>, String> {
    read_entry(&entry(id).map_err(safe_error)?)
}

fn read_entry(entry: &Entry) -> Result<Option<String>, String> {
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(safe_error(error)),
    }
}

pub(crate) fn write(id: &str, value: &str) -> Result<(), String> {
    entry(id).map_err(safe_error)?.set_password(value).map_err(safe_error)
}

pub(crate) fn delete(id: &str) -> Result<(), String> {
    match entry(id).map_err(safe_error)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(safe_error(error)),
    }
}

fn safe_error(error: Error) -> String {
    // Backend errors can contain raw secret bytes, arbitrary platform messages
    // or debug output from ambiguous entries. Never format their payloads.
    let reason = match error {
        Error::NoStorageAccess(_) => "Zugriff verweigert oder Speicher gesperrt",
        Error::NoEntry => "Eintrag nicht gefunden",
        Error::BadEncoding(_) | Error::BadDataFormat(_, _) => "Ungültiges Datenformat",
        Error::BadStoreFormat(_) => "Ungültiges Speicherformat",
        Error::TooLong(_, _) => "Plattformgrenze überschritten",
        Error::Invalid(_, _) => "Ungültige Eintragsparameter",
        Error::Ambiguous(_) => "Mehrere passende Einträge gefunden",
        _ => "Plattformzugriff fehlgeschlagen",
    };
    format!("Sicherer Schlüsselspeicher: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_namespace_ids_and_unicode_survive_mock_roundtrip() {
        let id = "credential-unicode";
        assert_eq!(entry(id).unwrap().get_specifiers(), Some((SERVICE.into(), id.into())));
        assert_eq!(read(id).unwrap(), None);
        write(id, "synthetic-ä漢字🪐").unwrap();
        assert_eq!(read(id).unwrap().as_deref(), Some("synthetic-ä漢字🪐"));
        write(id, "replacement").unwrap();
        assert_eq!(read(id).unwrap().as_deref(), Some("replacement"));
        delete(id).unwrap();
        delete(id).unwrap();
        assert_eq!(read(id).unwrap(), None);
    }

    #[test]
    fn credential_ids_do_not_share_values() {
        write("first-id", "first").unwrap();
        write("second-id", "second").unwrap();
        delete("first-id").unwrap();
        assert_eq!(read("second-id").unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn malformed_secret_and_platform_errors_never_disclose_payloads() {
        let entry = entry("malformed-fixture").unwrap();
        let credential = entry.as_any().downcast_ref::<keyring_core::mock::Cred>().unwrap();
        let marker = "DO-NOT-DISCLOSE-synthetic-secret";
        for error in [
            Error::BadEncoding(marker.as_bytes().to_vec()),
            Error::BadDataFormat(marker.as_bytes().to_vec(), Box::new(std::io::Error::other(marker))),
            Error::PlatformFailure(Box::new(std::io::Error::other(marker))),
            Error::NoStorageAccess(Box::new(std::io::Error::other(marker))),
            Error::BadStoreFormat(marker.into()),
            Error::Invalid(marker.into(), marker.into()),
            Error::TooLong(marker.into(), 42),
            Error::Ambiguous(vec![super::entry("ambiguous-fixture").unwrap()]),
            Error::NotSupportedByStore(marker.into()),
        ] {
            credential.set_error(error);
            let message = read_entry(&entry).unwrap_err();
            assert!(!message.contains(marker));
        }
    }
}
