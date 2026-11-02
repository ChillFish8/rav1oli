use std::path::Path;

use anyhow::{Context, bail};
use rav1oli_table::DisplaySettings;
use rav1oli_vship::DisplayModel;

type Resolution = (u32, u32);

/// The lowest similarity at which a model name is suggested for a typo.
const SUGGESTION_THRESHOLD: f64 = 0.5;

const FHD: Resolution = (1920, 1080);
const UHD: Resolution = (3840, 2160);

const BUILT_IN_DISPLAYS: [(&str, DisplayModel, Resolution); 9] = [
    ("trix_fhd", DisplayModel::TrixFhd, FHD),
    ("standard_fhd", DisplayModel::StandardFhd, FHD),
    ("trix_uhd_sdr", DisplayModel::TrixUhdSdr, UHD),
    ("trix_uhd_hdr", DisplayModel::TrixUhdHdr, UHD),
    ("standard_4k", DisplayModel::Standard4k, UHD),
    ("standard_hdr_pq", DisplayModel::StandardHdrPq, UHD),
    ("standard_hdr_hlg", DisplayModel::StandardHdrHlg, UHD),
    ("standard_hdr_dark", DisplayModel::StandardHdrDark, UHD),
    (
        "standard_hdr_linear_zoom",
        DisplayModel::StandardHdrLinearZoom,
        UHD,
    ),
];

/// The display CVVDP will model, with the settings to record in the map.
#[derive(Debug, Clone)]
pub struct SelectedDisplay {
    pub model: DisplayModel,
    pub width: u32,
    pub height: u32,
    pub settings: DisplaySettings,
}

/// Picks the display model for video of the given size.
///
/// Without a model, 1920x1080 uses `trix_fhd` and 3840x2160 uses `trix_uhd_hdr`.
pub fn select_display(
    model: Option<&str>,
    config: Option<&Path>,
    width: u32,
    height: u32,
) -> anyhow::Result<SelectedDisplay> {
    let selected = match (config, model) {
        (Some(_), None) => bail!("a display config needs `--display-model` to name its model"),
        (Some(path), Some(name)) => custom_display(path, name)?,
        (None, _) => built_in_display(model, width, height)?,
    };

    let chosen = (selected.width, selected.height);
    if chosen != (width, height) {
        let name = &selected.settings.model;
        let (chosen_width, chosen_height) = chosen;
        let message = format!(
            "display model `{name}` is {chosen_width}x{chosen_height} \
             but the video is {width}x{height}"
        );
        bail!("{message}");
    }

    Ok(selected)
}

fn custom_display(path: &Path, name: &str) -> anyhow::Result<SelectedDisplay> {
    let shown = path.display();
    let context = format!("failed to read display config {shown}");
    let contents = std::fs::read_to_string(path).with_context(|| context)?;

    let context = format!("display config {shown} is not valid JSON");
    let parsed: serde_json::Value = serde_json::from_str(&contents).with_context(|| context)?;
    let Some(object) = parsed.as_object() else {
        bail!("display config {shown} must be a JSON object");
    };

    if object.is_empty() {
        bail!("display config {shown} contains no display models");
    }

    let mut resolutions = Vec::new();
    for (entry_name, entry) in object {
        let Some(resolution) = parse_resolution(entry) else {
            bail!("display model `{entry_name}` in {shown} needs a `resolution` of two numbers");
        };
        resolutions.push((entry_name.as_str(), resolution));
    }

    let found = resolutions.iter().find(|(entry_name, _)| *entry_name == name);
    let Some((_, (width, height))) = found else {
        let candidates: Vec<&str> = object.keys().map(String::as_str).collect();
        let place = format!("is not in the display config {shown}");
        return Err(unknown_model_error(name, &place, &candidates));
    };

    let model = DisplayModel::Custom {
        name: name.to_string(),
        json: contents.clone(),
    };
    let settings = DisplaySettings {
        model: name.to_string(),
        config: Some(contents),
    };
    Ok(SelectedDisplay {
        model,
        width: *width,
        height: *height,
        settings,
    })
}

fn parse_resolution(entry: &serde_json::Value) -> Option<Resolution> {
    let values = entry.get("resolution")?.as_array()?;
    if values.len() != 2 {
        return None;
    }

    let width = values[0].as_u64()?;
    let height = values[1].as_u64()?;
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    Some((width, height))
}

fn built_in_display(model: Option<&str>, width: u32, height: u32) -> anyhow::Result<SelectedDisplay> {
    let key = match (model, (width, height)) {
        (Some(key), _) => key,
        (None, FHD) => "trix_fhd",
        (None, UHD) => "trix_uhd_hdr",
        (None, _) => {
            let size = format!("{width}x{height}");
            bail!("no default display for {size}, pass `--display-model`")
        },
    };

    let found = BUILT_IN_DISPLAYS.iter().find(|(name, ..)| *name == key);
    let Some((name, display, (display_width, display_height))) = found else {
        let candidates: Vec<&str> = BUILT_IN_DISPLAYS.iter().map(|(name, ..)| *name).collect();
        return Err(unknown_model_error(key, "is not a built-in model", &candidates));
    };

    let settings = DisplaySettings {
        model: name.to_string(),
        config: None,
    };
    Ok(SelectedDisplay {
        model: display.clone(),
        width: *display_width,
        height: *display_height,
        settings,
    })
}

fn unknown_model_error(name: &str, place: &str, candidates: &[&str]) -> anyhow::Error {
    let closest = candidates
        .iter()
        .map(|candidate| (*candidate, strsim::normalized_levenshtein(name, candidate)))
        .max_by(|(_, left), (_, right)| left.total_cmp(right));
    let suggestion = match closest {
        Some((candidate, score)) if score >= SUGGESTION_THRESHOLD => {
            format!(", did you mean '{candidate}'?")
        },
        _ => ".".to_string(),
    };

    let available = candidates.join(", ");
    anyhow::anyhow!("display model `{name}` {place}{suggestion} Available models: {available}")
}
