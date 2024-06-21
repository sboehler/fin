use std::{
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Debug, Eq, PartialEq)]
pub struct File {
    pub path: Option<PathBuf>,
    pub text: String,
}

impl File {
    pub fn mem(s: &str) -> Rc<File> {
        Rc::new(File {
            path: None,
            text: s.to_owned(),
        })
    }

    pub fn read(path: &Path) -> io::Result<Rc<File>> {
        Ok(Rc::new(File {
            text: fs::read_to_string(path)?,
            path: Some(path.to_path_buf()),
        }))
    }

    pub fn position(&self, pos: usize) -> (usize, usize) {
        let lines = self.text[..pos].split(|c| c == '\n').collect::<Vec<_>>();
        let line = lines.len();
        let col = lines.last().iter().flat_map(|s| s.chars()).count() + 1;
        (line, col)
    }
}

#[cfg(test)]
mod tests {
    use super::File;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_position() {
        let f = File::mem(&["foo", "bar", ""].join("\n"));
        assert_eq!(f.position(0), (1, 1));
        assert_eq!(f.position(1), (1, 2));
        assert_eq!(f.position(2), (1, 3));
        assert_eq!(f.position(3), (1, 4));
        assert_eq!(f.position(4), (2, 1));
        assert_eq!(f.position(5), (2, 2));
        assert_eq!(f.position(6), (2, 3));
        assert_eq!(f.position(7), (2, 4));
        assert_eq!(f.position(8), (3, 1));
    }
}
