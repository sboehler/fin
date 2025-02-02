use std::{
    fs, io,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct File {
    pub path: Option<PathBuf>,
    pub text: String,
}

impl File {
    pub fn read(path: &Path) -> io::Result<File> {
        Ok(File {
            text: fs::read_to_string(path)?,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn context(&self, range: Range<usize>) -> Vec<(usize, &str)> {
        let (start_line, _) = self.position(range.start);
        let (end_line, _) = self.position(range.end);

        self.text
            .lines()
            .enumerate()
            .skip(start_line - 1)
            .take(end_line - start_line + 1)
            .map(|(i, l)| (i + 1, l))
            .collect()
    }

    pub fn position(&self, pos: usize) -> (usize, usize) {
        let lines = self.text[..pos].split('\n').collect::<Vec<_>>();
        let line = lines.len();
        let col = lines.last().iter().flat_map(|s| s.chars()).count() + 1;
        (line, col)
    }

    pub fn fmt_range(&self, f: &mut std::fmt::Formatter, range: Range<usize>) -> std::fmt::Result {
        self.context(range)
            .iter()
            .try_for_each(|(i, l)| writeln!(f, "{:5} |{}", i, l))
    }
}
