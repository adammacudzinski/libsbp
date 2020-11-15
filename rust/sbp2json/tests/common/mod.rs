use std::{
    env,
    fs::{self, File},
    ops::Drop,
    path::PathBuf,
    process::{Command, Stdio},
    vec::Vec,
};

use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
use sha2::{Digest, Sha256};

pub fn run_sbp2json(reader: File, writer: File) {
    run_bin("sbp2json", reader, writer)
}

pub fn run_json2sbp(reader: File, writer: File) {
    run_bin("json2sbp", reader, writer)
}

pub fn run_json2json(reader: File, writer: File) {
    run_bin("json2json", reader, writer)
}

fn run_bin(name: &str, reader: File, writer: File) {
    let mut cmd = Command::cargo_bin(name).unwrap();

    cmd.stdin(reader);
    cmd.stdout(writer);
    cmd.stderr(Stdio::inherit());

    cmd.assert().success();
}

pub fn find_project_root() -> Option<PathBuf> {
    let exe = env::current_exe();
    assert!(exe.is_ok());
    let mut path = exe.unwrap();
    loop {
        let parent = path.parent();
        if let Some(parent) = parent {
            let git_dir = parent.join(".git");
            if git_dir.exists() {
                return Some(parent.to_path_buf());
            } else {
                path = parent.to_path_buf();
            }
        } else {
            break;
        }
    }
    return None;
}

pub struct DeleteTestOutput {
    files: Vec<PathBuf>,
}

impl Drop for DeleteTestOutput {
    fn drop(&mut self) {
        let skip_delete =
            env::var("RUST_SKIP_DELETE_TEST_DATA").map_or(false, |var| !var.is_empty());
        if skip_delete {
            return;
        }
        for file in &self.files {
            if file.as_path().exists() {
                let _ = fs::remove_file(file).map_err(|e| format!("could not delete file: {}", e));
            }
        }
    }
}

impl DeleteTestOutput {
    pub fn new() -> DeleteTestOutput {
        DeleteTestOutput { files: vec![] }
    }
    pub fn add_test_output(&mut self, file_path: &PathBuf) {
        self.files.push(file_path.clone());
    }
}

pub struct ThirdTransform<F>
where
    F: FnMut(File, File),
{
    pub transform: F,
    pub expected_output: String,
}

#[macro_export]
macro_rules! make_none_transform {
    () => {{
        let empty_closure = |_: File, _: File| ();
        let s = Some(ThirdTransform {
            transform: empty_closure,
            expected_output: "".into(),
        });
        s.filter(|_| false)
    }};
}

pub fn test_round_trip<F1, F2, F3>(
    mut first_transform: F1,
    mut second_transform: F2,
    test_name: &str,
    input_filename: &str,
    mut third_transform: Option<ThirdTransform<F3>>,
) where
    F1: FnMut(File, File),
    F2: FnMut(File, File),
    F3: FnMut(File, File),
{
    let mut del_test_output = DeleteTestOutput::new();

    let root = find_project_root().unwrap();
    let root = root.as_path();
    let input_path = root.join(format!("test_data/{}", input_filename));
    let first_transform_output = format!("test_data/test_{}.output.first_transform", test_name);
    let second_transform_output = format!("test_data/test_{}.output.second_transform", test_name);
    let third_transform_output = format!("test_data/test_{}.output.third_transform", test_name);
    let output_path = root.join(&first_transform_output);

    {
        del_test_output.add_test_output(&output_path);

        let input_file =
            File::open(&input_path).expect("could not open first transform input file");
        let output_file =
            File::create(&output_path).expect("could not create first transform output file");

        first_transform(input_file, output_file);

        let input_path = root.join(&first_transform_output);
        let output_path = root.join(&second_transform_output);

        del_test_output.add_test_output(&output_path);

        let input_file =
            File::open(&input_path).expect("could not open second transform input file");
        let output_file =
            File::create(&output_path).expect("could not create second transform output file");

        second_transform(input_file, output_file);
    }

    if let Some(third_transform) = &mut third_transform {
        let input_path = root.join(&second_transform_output);
        let output_path = root.join(&third_transform_output);

        eprintln!("{}", input_path.display());

        del_test_output.add_test_output(&output_path);

        let input_file = File::open(input_path).expect("could not open third transform input file");
        let output_file =
            File::create(output_path).expect("could not create third transform output file");

        (third_transform.transform)(input_file, output_file);
    }

    let (input_path, output_path) = if let Some(third_transform) = third_transform {
        (
            root.join(format!("test_data/{}", third_transform.expected_output)),
            root.join(third_transform_output),
        )
    } else {
        (root.join(input_path), root.join(second_transform_output))
    };

    del_test_output.add_test_output(&output_path);

    let mut input_file = File::open(&input_path).unwrap();
    let mut output_file = File::open(&output_path).unwrap();

    let mut input_file_hash = Sha256::new();
    let mut output_file_hash = Sha256::new();

    std::io::copy(&mut input_file, &mut input_file_hash)
        .map(|_| ())
        .unwrap();
    std::io::copy(&mut output_file, &mut output_file_hash)
        .map(|_| ())
        .unwrap();

    let input_digest = input_file_hash.result();
    let output_digest = output_file_hash.result();

    let input_hex_digest = hex::encode(&input_digest[..]);
    let output_hex_digest = hex::encode(&output_digest[..]);

    assert_eq!(input_hex_digest, output_hex_digest);
}
