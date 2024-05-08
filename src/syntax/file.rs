use std::{
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Debug, Eq, PartialEq)]
pub struct File {
    pub path: PathBuf,
    pub text: String,
}

impl File {
    pub fn mem(s: &str) -> Rc<File> {
        Rc::new(File {
            path: "<memory>".into(),
            text: s.to_owned(),
        })
    }

    pub fn read(path: &Path) -> io::Result<Rc<File>> {
        Ok(Rc::new(File {
            text: fs::read_to_string(path)?,
            path: path.to_path_buf(),
        }))
    }
}
