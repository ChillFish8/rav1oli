use std::path::Path;

use clap::error::ErrorKind;

use super::parse_run_args as parse_args;

fn path_text(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

#[test]
fn run_paths_must_all_differ() {
    let cases: [(&[&str], &str); 7] = [
        (&["-i", "a.mkv", "-o", "a.mkv"], "'--input' and '--output'"),
        (&["-i", "a.mkv", "-o", "./a.mkv"], "'--input' and '--output'"),
        (
            &["-i", "a.mkv", "-o", "b.ivf", "-m", "a.mkv"],
            "'--input' and '--map'",
        ),
        (
            &["-i", "a.mkv", "-o", "b.ivf", "-m", "b.ivf"],
            "'--output' and '--map'",
        ),
        (
            &["-i", "a.mkv", "-o", "b.ivf", "--keep-encode", "./a.mkv"],
            "'--input' and '--keep-encode'",
        ),
        (
            &["-i", "a.mkv", "-o", "b.ivf", "--keep-encode", "b.ivf"],
            "'--output' and '--keep-encode'",
        ),
        (
            &[
                "-i",
                "a.mkv",
                "-o",
                "b.ivf",
                "-m",
                "m.parquet",
                "--keep-encode",
                "./m.parquet",
            ],
            "'--map' and '--keep-encode'",
        ),
    ];

    for (flags, expected) in cases {
        let mut arguments = vec!["test", "run"];
        arguments.extend_from_slice(flags);
        let args = parse_args(&arguments);

        let error = args.validate_paths().expect_err("validation should fail");

        assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{flags:?}");
        assert!(error.to_string().contains(expected), "{flags:?}: {error}");
    }
}

#[test]
fn run_outputs_in_a_missing_directory_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing");
    let missing_file = path_text(&missing.join("file"));
    let existing_file = path_text(&directory.path().join("out.ivf"));
    let cases = [
        ("--output", [missing_file.as_str(), "", ""]),
        ("--map", [existing_file.as_str(), missing_file.as_str(), ""]),
        (
            "--keep-encode",
            [existing_file.as_str(), "", missing_file.as_str()],
        ),
    ];

    for (flag, [output, map, keep_encode]) in cases {
        let mut arguments = vec!["test", "run", "-i", "a.mkv", "-o", output];
        if !map.is_empty() {
            arguments.extend_from_slice(&["-m", map]);
        }
        if !keep_encode.is_empty() {
            arguments.extend_from_slice(&["--keep-encode", keep_encode]);
        }
        let args = parse_args(&arguments);

        let error = args.validate_paths().expect_err("validation should fail");

        assert_eq!(error.kind(), ErrorKind::ValueValidation, "{flag}");
        let expected = format!("'{flag}' needs an existing directory");
        assert!(error.to_string().contains(&expected), "{flag}: {error}");
    }
}

#[test]
fn the_output_and_map_cannot_be_directories() {
    let directory = tempfile::tempdir().unwrap();
    let directory_text = path_text(directory.path());
    let as_output = parse_args(&["test", "run", "-i", "a.mkv", "-o", &directory_text]);
    let as_map = parse_args(&["test", "run", "-i", "a.mkv", "-o", "b.ivf", "-m", &directory_text]);

    let output_error = as_output.validate_paths().expect_err("validation should fail");
    let map_error = as_map.validate_paths().expect_err("validation should fail");

    assert_eq!(output_error.kind(), ErrorKind::ValueValidation);
    assert!(output_error.to_string().contains("'--output'"));
    assert_eq!(map_error.kind(), ErrorKind::ValueValidation);
    assert!(map_error.to_string().contains("'--map'"));
}

#[test]
fn a_table_for_another_metric_than_the_metric_flag_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let tables = directory.path().join("tables.json");
    std::fs::write(&tables, r#"{"version": 1, "metric": "butteraugli"}"#).unwrap();
    let tables = path_text(&tables);
    let arguments = [
        "test", "run", "-i", "a.mkv", "-o", "b.ivf", "--metric", "cvvdp", "--tables", &tables,
    ];
    let args = parse_args(&arguments);

    let error = args.validate_paths().expect_err("validation should fail");

    assert_eq!(error.kind(), ErrorKind::ValueValidation);
    assert!(error.to_string().contains("'--tables'"));
    assert!(error.to_string().contains("butteraugli"));
    assert!(error.to_string().contains("cvvdp"));
}

#[test]
fn a_table_for_the_metric_flag_is_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let tables = directory.path().join("tables.json");
    std::fs::write(&tables, r#"{"version": 1, "metric": "butteraugli"}"#).unwrap();
    let tables = path_text(&tables);
    let arguments = [
        "test",
        "run",
        "-i",
        "a.mkv",
        "-o",
        "b.ivf",
        "--metric",
        "butteraugli",
        "--tables",
        &tables,
    ];
    let args = parse_args(&arguments);

    args.validate_paths().unwrap();
}

#[test]
fn tables_and_keep_encode_are_optional() {
    let args = parse_args(&["test", "run", "-i", "a.mkv", "-o", "b.ivf"]);

    args.validate_paths().unwrap();

    assert!(args.tables.is_none());
    assert!(args.keep_encode.is_none());
    assert!(args.map.is_none());
}

#[cfg(feature = "gpu-tests")]
mod gpu {
    use std::path::Path;

    use rav1oli_table::MapReader;

    use super::{parse_args, path_text};
    use crate::commands::run::run;
    use crate::decode::Decoder;
    use crate::decode::tests::copy_fixture;
    use crate::metrics::tests::gpu_id;

    fn file_names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn run_args(video: &Path, output: &Path, extra: &[&str]) -> crate::commands::run::RunArgs {
        let input = path_text(video);
        let output = path_text(output);
        let gpu = gpu_id().to_string();
        let mut arguments = vec![
            "test",
            "run",
            "-i",
            &input,
            "-o",
            &output,
            "--metric",
            "butteraugli",
            "--preset",
            "12",
            "--gpu",
            &gpu,
        ];
        arguments.extend_from_slice(extra);

        parse_args(&arguments)
    }

    #[test]
    fn run_without_a_map_leaves_only_the_output() {
        let directory = tempfile::tempdir().unwrap();
        let video = copy_fixture(directory.path());
        let output = directory.path().join("out.ivf");
        let args = run_args(&video, &output, &[]);

        let outcome = run(&args).unwrap();

        assert!(outcome.frames > 0);
        let names = file_names(directory.path());
        assert_eq!(names, [".rav1oli", "out.ivf", "yuv420p.mkv"]);
    }

    #[test]
    fn run_with_a_map_keeps_it() {
        let directory = tempfile::tempdir().unwrap();
        let video = copy_fixture(directory.path());
        let output = directory.path().join("out.ivf");
        let map = directory.path().join("map.parquet");
        let map_text = path_text(&map);
        let args = run_args(&video, &output, &["--map", &map_text]);

        let outcome = run(&args).unwrap();

        let reader = MapReader::open(&map).unwrap();
        let fixture = Decoder::open_uncached(&video).unwrap();
        let fixture_frames = u64::from(fixture.properties().num_frames as u32);
        assert_eq!(u64::from(reader.metadata().source.frame_count), fixture_frames);
        assert_eq!(outcome.frames, fixture_frames);
        let names = file_names(directory.path());
        assert_eq!(names, [".rav1oli", "map.parquet", "out.ivf", "yuv420p.mkv"]);
    }

    #[test]
    fn run_with_keep_encode_keeps_the_first_pass() {
        let directory = tempfile::tempdir().unwrap();
        let video = copy_fixture(directory.path());
        let output = directory.path().join("out.ivf");
        let kept = directory.path().join("kept.ivf");
        let kept_text = path_text(&kept);
        let args = run_args(&video, &output, &["--keep-encode", &kept_text]);

        run(&args).unwrap();

        assert!(kept.exists());
        let names = file_names(directory.path());
        assert_eq!(names, [".rav1oli", "kept.ivf", "out.ivf", "yuv420p.mkv"]);
    }
}
