use log::warn;
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
};

use ini::Ini;
use relative_path::RelativePathBuf;
use thiserror::Error;

pub type ResourceMap = HashMap<RelativePathBuf, PathBuf>;

pub struct Theme {
    name: String,
    rtconfig: String,
    config: Ini,
    resources: ResourceMap,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: Default::default(),
            rtconfig: Default::default(),
            config: Default::default(),
            resources: Default::default(),
        }
    }
}

impl Theme {
    pub fn new(
        name: &str,
        rtconfig: &str,
        config: Ini,
        resources: HashMap<RelativePathBuf, PathBuf>,
    ) -> Self {
        let name = name.to_string();
        let rtconfig = rtconfig.to_string();

        Self {
            name,
            rtconfig,
            config,
            resources,
        }
    }

    fn reapertheme(&self) -> String {
        let mut buf = Vec::new();
        self.config.write_to(&mut buf).unwrap();
        let result = std::str::from_utf8(buf.as_slice()).unwrap().to_string();
        result
    }
}

pub struct BuildOptions {
    overwrite: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self { overwrite: false }
    }
}

impl BuildOptions {
    pub fn overwrite(mut self, x: bool) -> Self {
        self.overwrite = x;
        self
    }
}

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("the path `{0}` already exists")]
    PathExistsError(PathBuf),
}

impl Theme {
    pub fn build(&self, path: &Path, opt: &BuildOptions) -> Result<(), BuildError> {
        if (path.is_file() && !opt.overwrite) || path.is_dir() {
            return Err(BuildError::PathExistsError(path.to_path_buf()));
        }

        // emit warnings
        {
            let path_stem = path
                .file_stem()
                .unwrap_or(Default::default())
                .to_string_lossy();
            let extension = path
                .extension()
                .unwrap_or(Default::default())
                .to_string_lossy();
            if path_stem != self.name {
                warn!("Output theme file has a different name than the theme; REAPER may not load the theme correctly!");
            }
            if extension.to_ascii_lowercase() != "reaperthemezip" {
                warn!("Output theme file does not end with '.ReaperThemeZip'; REAPER may not be able to load the theme!");
            }
        }

        // create ZIP file
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let file_options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6));

        // write .ReaperTheme
        {
            let reapertheme_path = {
                let mut x = RelativePathBuf::from(&self.name);
                x.set_extension("ReaperTheme");
                x
            };

            zip.start_file(reapertheme_path.as_str(), file_options)
                .expect(&format!(
                    "failed to write theme .ReaperTheme file: {}",
                    &reapertheme_path
                ));

            zip.write_all(self.reapertheme().as_bytes())
                .expect(&format!(
                    "failed to write theme .ReaperTheme file: {}",
                    &reapertheme_path
                ));
        }

        // write rtconfig.txt
        {
            let rtconfig_path = RelativePathBuf::from(&self.name).join("rtconfig.txt");

            zip.start_file(rtconfig_path.as_str(), file_options)
                .expect(&format!(
                    "failed to write theme rtconfig.txt: {}",
                    &rtconfig_path
                ));
            zip.write_all(self.rtconfig.as_bytes()).expect(&format!(
                "failed to write theme rtconfig.txt: {}",
                &rtconfig_path
            ));
        }

        // write resources
        {
            let resource_root = RelativePathBuf::from(&self.name);

            for (archive_path, os_path) in self.resources.iter() {
                let archive_path = resource_root.join(archive_path);

                let mut resource = std::fs::File::open(os_path.as_path())
                    .expect(&format!("failed to read resource {}", os_path.display()));

                zip.start_file(archive_path.as_str(), file_options)
                    .expect(&format!(
                        "failed to write theme resource: {}",
                        &archive_path
                    ));
                std::io::copy(&mut resource, &mut zip).expect(&format!(
                    "failed to write theme resource: {}",
                    os_path.display()
                ));
            }
        }

        zip.finish().expect("failed to write archive");

        Ok(())
    }
}
