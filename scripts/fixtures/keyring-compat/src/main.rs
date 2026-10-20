// Runs only through the guarded CI launcher in an ephemeral hosted runner.
#[path = "../../../../src-tauri/src/credentials.rs"]
mod credentials;

fn run() -> Result<(), &'static str> {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || std::env::var("RUNNER_ENVIRONMENT").as_deref() != Ok("github-hosted")
        || std::env::var("ORBITPAL_KEYRING_COMPAT_DISPOSABLE").as_deref() != Ok("1")
    {
        return Err("native fixture requires the disposable GitHub-hosted CI launcher");
    }
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 { return Err("invalid fixture arguments"); }
    let id = args[2].strip_prefix("orbitpal-keyring-compat-").ok_or("invalid fixture id")?;
    uuid::Uuid::parse_str(id).map_err(|_| "invalid fixture UUID")?;
    let id = &args[2];
    let expected = &args[3];
    // Every value is a public synthetic fixture marker. Do not render returned
    // backend errors or mismatched data, even when the assertion fails.
    let old = || legacy_keyring::Entry::new(credentials::SERVICE, id).map_err(|_| "legacy entry failed");
    match args[1].as_str() {
        "old-write" => old()?.set_password(expected).map_err(|_| "legacy write failed")?,
        "new-write" => credentials::write(id, expected).map_err(|_| "new write failed")?,
        "old-read" => {
            if old()?.get_password().map_err(|_| "legacy read failed")? != *expected {
                return Err("legacy value mismatch");
            }
        }
        "new-read" => {
            if credentials::read(id).map_err(|_| "new read failed")?.as_deref() != Some(expected.as_str()) {
                return Err("new value mismatch");
            }
        }
        "old-missing" => {
            if !matches!(old()?.get_password(), Err(legacy_keyring::Error::NoEntry)) {
                return Err("legacy entry unexpectedly exists or failed");
            }
        }
        "new-missing" => {
            if credentials::read(id).map_err(|_| "new missing read failed")?.is_some() {
                return Err("new entry unexpectedly exists");
            }
        }
        "old-delete" => old()?.delete_credential().map_err(|_| "legacy delete failed")?,
        "new-delete" => credentials::delete(id).map_err(|_| "new delete failed")?,
        _ => return Err("unknown fixture operation"),
    }
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("Credential compatibility fixture: {message}");
        std::process::exit(1);
    }
}
