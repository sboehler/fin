use std::{error::Error, result};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub fn seq_serial<T, F>(ts: Vec<T>, fs: Vec<F>) -> Result<Vec<T>>
where
    F: Fn(T) -> Result<T>,
{
    let mut res = Vec::new();
    for mut t in ts {
        for f in &fs {
            t = f(t)?;
        }
        res.push(t)
    }
    Ok(res)
}

#[derive(Eq, PartialEq, Debug)]
struct Foo {
    value: usize,
}

#[cfg(test)]
mod tests {
    use crate::process::cpr::{seq_serial, Foo};

    #[test]
    fn test_seq_serial() {
        let f = |mut f: Foo| {
            f.value = f.value + 1;
            Ok(f)
        };

        assert_eq!(
            vec![Foo { value: 4 }, Foo { value: 5 }, Foo { value: 6 }],
            seq_serial(
                vec![Foo { value: 1 }, Foo { value: 2 }, Foo { value: 3 }],
                vec![f, f, f]
            )
            .unwrap()
        )
    }
}
