use std::env;
use std::fs;
use std::io::{Error, Write};
use std::path::Path;
use std::collections::HashMap;


// View loc folder with subfolders, get listings
// Returns touple (folder_map, file_list)
// folder_map keys are folders, and values are lists of direct childs
// file_list is a vector of all detected files with full path
fn scan_folder(loc: &Path) -> (HashMap<String, Vec<String>>, Vec<String>) {
    let mut folders: HashMap<String, Vec<String>> = HashMap::new();
    let mut files: Vec<String> = Vec::new();
    let mut current = Vec::new();

    if loc.is_dir() {
        for entry in fs::read_dir(loc).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let path_str = String::from(path.to_str().unwrap()).replace("\\", "/");

            current.push(path_str.clone());

            // if folder then scan recursively
            if path.is_dir() {
                let (d, mut f) = scan_folder(&path);
                for (key, value) in d.into_iter() {
                    folders.insert(key, value);
                }

                files.append(&mut f);
            } else {
                files.push(path_str);
            }
        }

        current.sort();
        folders
            .entry(String::from(loc.to_str().unwrap()).replace("\\", "/"))
            .or_insert(current);
    } else {
        panic!("{:?} is not a folder!", loc);
    }

    (folders, files)
}

// Write folder/file information to output file
fn fill_from_location(f: &mut fs::File, loc: &Path) -> Result<(), (Error)> {
    let (_, mut files) = scan_folder(loc);

    let loc_str = loc.to_str().unwrap();
    let mut idx = loc_str.len();
    files.sort();

    for name in files.iter() {
        let (_, strip) = name.split_at(idx);
        write!(
            f,
            "        b\"{}\" => Some(include_bytes!(\"{}\")),\n",
            strip,
            name
        )?;
    }

    Ok(())
}

fn main() {
    println!("cargo:rustc-env=TARGET={}", env::var("TARGET").unwrap());

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("gen.rs");
    let mut f = fs::File::create(&dest_path).unwrap();
    let src = env::var("INITFS_FOLDER");

    // Write header
    f.write_all(
        b"
    pub fn initfs_get_file(name: &'static [u8]) -> Option<&'static [u8]> {
        match name {
",
    ).unwrap();

    match src {
        Ok(v) => fill_from_location(&mut f, Path::new(&v)).unwrap(),
        Err(e) => {
            println!(
                "cargo:warning=location not found: {}, please set proper INITFS_FOLDER.",
                e
            );
        }
    }

    f.write_all(
        b"
        _ => None
    }
}
",
    ).unwrap();
}
