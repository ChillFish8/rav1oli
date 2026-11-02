use crate::analysis::check_frame_counts;

#[test]
fn matching_frame_counts_are_accepted() {
    check_frame_counts(48, 48).unwrap();
}

#[test]
fn a_short_encode_is_an_error() {
    let error = check_frame_counts(48, 47).unwrap_err();

    assert_eq!(
        error.to_string(),
        "the first pass has 47 frames but the source has 48"
    );
}

#[cfg(feature = "gpu-tests")]
#[test]
fn the_fixture_scores_every_frame() {
    use rav1oli_table::{MapReader, MapWriter};

    use crate::analysis::{map_metadata, metric_config, score_frames};
    use crate::commands::options::{AnalysisOptions, EncoderOptions, Metric};
    use crate::decode::Decoder;
    use crate::decode::tests::copy_fixture;
    use crate::metrics::FrameDescription;
    use crate::metrics::tests::gpu_id;
    use crate::video::{colour_description, describe_source, encode_video, encoder_settings};

    let directory = tempfile::tempdir().unwrap();
    let video = copy_fixture(directory.path());
    let first_pass = directory.path().join("first.ivf");
    let map_path = directory.path().join("map.parquet");
    let mut decoder = Decoder::open_uncached(&video).unwrap();
    let source = describe_source(decoder.properties()).unwrap();
    let first_frame = decoder.frames().next().unwrap().unwrap();
    let description = FrameDescription::from_frame(&first_frame);
    let colour = colour_description(&description).unwrap();
    let options = EncoderOptions {
        preset: 12,
        crf: 40,
        svt_args: vec![],
    };
    let settings = encoder_settings(&source, colour, &options);
    encode_video(
        &mut decoder,
        &source,
        &settings,
        &first_pass,
        "first pass",
        |_| Ok(None),
    )
    .unwrap();

    let analysis = AnalysisOptions {
        metric: Metric::Butteraugli,
        no_temporal: false,
        gpu: 0,
        display_model: None,
        display_config: None,
        intensity_target: None,
        floor: None,
    };
    let config = metric_config(&analysis, &source).unwrap();
    let metadata = map_metadata(&source, "hash".to_string(), &options, &config);
    let mut writer = MapWriter::create(&map_path, &metadata).unwrap();
    let mut reference = Decoder::open_uncached(&video).unwrap();
    let mut encoded = Decoder::open_uncached(&first_pass).unwrap();

    let gpu = gpu_id();
    let summary = score_frames(&mut reference, &mut encoded, config, gpu, &mut writer).unwrap();
    writer.finish(&summary).unwrap();

    let reader = MapReader::open(&map_path).unwrap();
    assert_eq!(reader.count(), source.frame_count as usize);
}
