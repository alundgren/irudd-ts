use anyhow::Result;
use std::path::{Path, PathBuf};

// Compiler path parsers interpret the '?' in Windows canonical prefixes as a
// query or UNC component. Keep canonical filesystem identities elsewhere.
pub(crate) fn compatible(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        use anyhow::{Context, bail};
        use std::{
            ffi::OsString,
            path::{Component, Prefix},
        };
        let mut components = path.components();
        let mut converted = match components.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", drive as char)),
                Prefix::VerbatimUNC(server, share) => {
                    let mut root = OsString::from("\\\\");
                    root.push(server);
                    root.push("\\");
                    root.push(share);
                    root.push("\\");
                    PathBuf::from(root)
                }
                Prefix::Verbatim(_) => {
                    bail!("compiler does not support this Windows path namespace")
                }
                _ => return Ok(path.to_owned()),
            },
            _ => return Ok(path.to_owned()),
        };
        for component in components {
            if component != Component::RootDir {
                converted.push(component.as_os_str());
            }
        }
        if converted
            .canonicalize()
            .context("checking compiler-compatible Windows path")?
            != path
                .canonicalize()
                .context("checking original Windows path")?
        {
            bail!("compiler-compatible Windows path identifies a different filesystem entry");
        }
        Ok(converted)
    }
    #[cfg(not(windows))]
    Ok(path.to_owned())
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use crate::compiler_path::compatible;
    use std::{fs, path::Path};

    #[test]
    fn canonical_paths_keep_the_same_file_and_reject_other_namespaces() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("space and é.ts");
        fs::write(&file, "export {};").unwrap();
        let canonical = file.canonicalize().unwrap();
        let converted = compatible(&canonical).unwrap();
        assert!(!converted.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(converted.canonicalize().unwrap(), canonical);
        assert_eq!(compatible(&converted).unwrap(), converted);
        assert!(compatible(Path::new(r"\\?\GLOBALROOT\Device\invalid")).is_err());

        let mut deep = root.path().canonicalize().unwrap();
        for _ in 0..10 {
            deep.push("long compiler directory component");
        }
        fs::create_dir_all(&deep).unwrap();
        let long = deep.join("source.ts");
        fs::write(&long, "export {};").unwrap();
        assert!(long.as_os_str().len() > 260);
        assert_eq!(compatible(&long).unwrap().canonicalize().unwrap(), long);

        // Removing the namespace must not redirect a literal trailing-dot name.
        let dotted = root.path().canonicalize().unwrap().join("literal.");
        fs::write(&dotted, "first").unwrap();
        fs::write(root.path().join("literal"), "different").unwrap();
        assert!(compatible(&dotted).is_err());
    }
}
