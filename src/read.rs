use anyhow::Ok;
use std::path::Path;
use walkdir::DirEntry;

type FilterFn = Box<dyn Fn(&str) -> bool + 'static>;

pub enum Exclude {
    Contains(String),
    Prefix(String),
    Suffix(String),
}

impl Exclude {
    fn into_filter(self) -> FilterFn {
        match self {
            Exclude::Contains(value) => Box::new(move |e: &str| -> bool { !e.contains(&value) }),
            Exclude::Prefix(value) => Box::new(move |e: &str| -> bool { !e.starts_with(&value) }),
            Exclude::Suffix(value) => Box::new(move |e: &str| -> bool { !e.ends_with(&value) }),
        }
    }
}

pub enum IncludeOnly {
    Suffix(String),
}

impl IncludeOnly {
    fn into_filter(self) -> FilterFn {
        match self {
            IncludeOnly::Suffix(value) => Box::new(move |e: &str| -> bool { e.ends_with(&value) }),
        }
    }
}

fn merge_predicates(predicates: Vec<FilterFn>) -> FilterFn {
    if predicates.is_empty() {
        return Box::new(|_: &str| true);
    }

    Box::new(move |e: &str| -> bool {
        for ex in &predicates {
            if ex(e) {
                return true;
            }
        }
        false
    })
}

/// one possible implementation of walking a directory only visiting files
pub fn visit_dirs<P, F>(
    dir: P,
    includes: Vec<IncludeOnly>,
    excludes: Vec<Exclude>,
    mut closure: F,
) -> anyhow::Result<()>
where
    P: AsRef<Path>,
    F: FnMut(DirEntry),
{
    let include_filter =
        merge_predicates(includes.into_iter().map(|io| io.into_filter()).collect());
    let exclude_filter =
        merge_predicates(excludes.into_iter().map(|ex| ex.into_filter()).collect());

    let wd = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            exclude_filter(e.path().to_str().unwrap()) && include_filter(e.path().to_str().unwrap())
        });

    for entry in wd {
        closure(entry)
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Ok;

    #[test]
    fn test_visit_dirs() -> anyhow::Result<()> {
        super::visit_dirs(
            "/home/kamil/programming/rust/rpotlight",
            vec![super::IncludeOnly::Suffix(".rs".to_string())],
            vec![super::Exclude::Contains("target/debug".to_string())],
            |entry| println!("{}", entry.path().to_str().unwrap()),
        )?;
        Ok(())
    }
}
