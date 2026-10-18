//! Direct process launch for native programs and known Windows npm shims.
//! Batch files are never interpreted as a shell command string.
use std::path::PathBuf;
use tokio::process::Command;

fn search_paths()->Vec<PathBuf>{
    let mut paths=std::env::var_os("PATH").map(|value|std::env::split_paths(&value).collect::<Vec<_>>()).unwrap_or_default();
    #[cfg(target_os="macos")]{paths.push("/opt/homebrew/bin".into());paths.push("/usr/local/bin".into());}
    #[cfg(windows)]{
        if let Some(value)=std::env::var_os("APPDATA"){paths.push(PathBuf::from(value).join("npm"));}
        if let Some(value)=std::env::var_os("USERPROFILE"){paths.push(PathBuf::from(value).join(".local").join("bin"));}
    }
    paths
}

#[cfg(any(windows,test))]
fn npm_entrypoint(shim:&std::path::Path)->Option<PathBuf>{
    let root=shim.parent()?;
    let relative=match shim.file_stem()?.to_str()?.to_ascii_lowercase().as_str(){
        "codex"=>"node_modules/@openai/codex/bin/codex.js",
        "claude"=>"node_modules/@anthropic-ai/claude-code/cli.js",
        "npm"=>"node_modules/npm/bin/npm-cli.js",
        "npx"=>"node_modules/npm/bin/npx-cli.js",
        _=>return None,
    };
    let path=root.join(relative);path.is_file().then_some(path)
}

#[cfg(windows)]
fn resolve_windows(program:&str,paths:&[PathBuf])->Result<Command,String>{
    use std::path::Path;
    let requested=Path::new(program);
    let names=if requested.extension().is_some(){vec![PathBuf::from(program)]}else{vec![PathBuf::from(format!("{program}.exe")),PathBuf::from(program),PathBuf::from(format!("{program}.cmd")),PathBuf::from(format!("{program}.bat"))]};
    let has_path=requested.is_absolute()||requested.components().count()>1;
    let found=if has_path{names.into_iter().find(|path|path.is_file())}else{paths.iter().find_map(|root|names.iter().map(|name|root.join(name)).find(|path|path.is_file()))};
    let path=found.ok_or_else(||format!("Startprogramm {program} wurde nicht gefunden. Installation oder absoluten Programmpfad prüfen."))?;
    let extension=path.extension().and_then(|value|value.to_str()).unwrap_or_default().to_ascii_lowercase();
    if extension=="cmd"||extension=="bat"{
        let entry=npm_entrypoint(&path).ok_or("Dieser Windows-Batchwrapper kann nicht direkt gestartet werden. Einen nativen .exe-Pfad verwenden oder node.exe als Programm und die CLI-JavaScript-Datei als erstes Argument eintragen.")?;
        let adjacent=path.parent().unwrap().join("node.exe");
        let node=if adjacent.is_file(){Some(adjacent)}else{paths.iter().map(|root|root.join("node.exe")).find(|path|path.is_file())}.ok_or("Für diesen npm-Startbefehl wurde node.exe nicht gefunden. Node.js installieren oder einen nativen CLI-Programmpfad verwenden.")?;
        let mut command=Command::new(node);command.arg(entry);Ok(command)
    }else{Ok(Command::new(path))}
}

pub fn command(program:&str)->Result<Command,String>{
    if program.trim().is_empty()||program.chars().any(char::is_control){return Err("Ungültiger Startbefehl".into());}
    let paths=search_paths();
    #[cfg(windows)] let mut command=resolve_windows(program,&paths)?;
    #[cfg(not(windows))] let mut command=Command::new(program);
    if let Ok(path)=std::env::join_paths(paths){command.env("PATH",path);}
    Ok(command)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn known_npm_shims_map_to_fixed_entrypoints_without_reading_batch_commands(){
        let root=std::env::temp_dir().join(format!("cohost-shim-test-{}",uuid::Uuid::new_v4()));
        let entry=root.join("node_modules/@openai/codex/bin/codex.js");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();std::fs::write(&entry,"fixture").unwrap();
        std::fs::write(root.join("codex.cmd"),"this must never execute & arbitrary batch syntax").unwrap();
        assert_eq!(npm_entrypoint(&root.join("codex.cmd")),Some(entry));
        assert!(npm_entrypoint(&root.join("unknown.cmd")).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_command_is_rejected_before_process_creation(){assert!(command("codex\nother-command").is_err());}
    #[cfg(windows)]
    #[test]
    fn windows_npx_shim_launches_node_with_separate_literal_arguments(){
        let root=std::env::temp_dir().join(format!("cohost-npx-test-{}",uuid::Uuid::new_v4()));
        let entry=root.join("node_modules/npm/bin/npx-cli.js");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();std::fs::write(&entry,"fixture").unwrap();
        std::fs::write(root.join("npx.cmd"),"not executed").unwrap();std::fs::write(root.join("node.exe"),"not executed").unwrap();
        let mut command=resolve_windows("npx",std::slice::from_ref(&root)).unwrap();command.arg("literal & not-a-shell | argument");
        assert_eq!(command.as_std().get_program(),root.join("node.exe").as_os_str());
        let arguments=command.as_std().get_args().collect::<Vec<_>>();
        assert_eq!(arguments[0],entry.as_os_str());assert_eq!(arguments[1],"literal & not-a-shell | argument");
        std::fs::remove_dir_all(root).unwrap();
    }
}
