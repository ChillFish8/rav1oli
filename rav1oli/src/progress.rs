use std::sync::OnceLock;
use std::time::Duration;

use indicatif::style::TemplateError;
use indicatif::{HumanDuration, MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use tracing_indicatif::IndicatifWriter;
use tracing_indicatif::writer::Stderr;

const BAR_TEMPLATE: &str = "{msg} [{elapsed_precise}] [{bar:40}] {pos}/{len} frames ({fps}, eta {eta})";

static PROGRESS: OnceLock<MultiProgress> = OnceLock::new();

/// The [MultiProgress] every bar is registered on.
///
/// It reports itself hidden when stderr is not a terminal, so a redirected run draws nothing.
fn multi() -> &'static MultiProgress {
    PROGRESS.get_or_init(MultiProgress::new)
}

/// The writer for the tracing subscriber.
///
/// Each write suspends the bars, so log lines land above them instead of overwriting them.
pub fn tracing_writer() -> IndicatifWriter<Stderr> {
    let progress = multi().clone();
    IndicatifWriter::new(progress)
}

fn format_fps(per_second: f64) -> String {
    format!("{per_second:.2} fps")
}

fn frames_per_second(frames: u64, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }

    frames as f64 / elapsed_seconds
}

fn bar_style() -> Result<ProgressStyle, TemplateError> {
    let style = ProgressStyle::with_template(BAR_TEMPLATE)?;
    let style = style.with_key(
        "fps",
        |state: &ProgressState, writer: &mut dyn std::fmt::Write| {
            let per_second = state.per_sec();
            let text = format_fps(per_second);
            // The key callback cannot return the `fmt::Result`, so a failed write is dropped.
            let _ = writer.write_str(&text);
        },
    );
    Ok(style)
}

fn frame_bar_on(multi: &MultiProgress, message: &'static str, total: u64) -> ProgressBar {
    let style = bar_style().expect("the bar template is valid");
    let bar = ProgressBar::new(total).with_style(style).with_message(message);
    bar.enable_steady_tick(Duration::from_millis(250));
    multi.add(bar)
}

/// Creates a frame progress bar on the shared [MultiProgress].
pub fn frame_bar(message: &'static str, total: u64) -> ProgressBar {
    frame_bar_on(multi(), message, total)
}

/// Clears the bar and logs how many frames it covered and how fast.
pub fn finish(bar: &ProgressBar, verb: &str) {
    let frames = bar.position();
    let elapsed = bar.elapsed();
    let fps = frames_per_second(frames, elapsed.as_secs_f64());
    let elapsed = HumanDuration(elapsed);

    bar.finish_and_clear();
    multi().remove(bar);

    tracing::info!("{verb} {frames} frames in {elapsed} ({fps:.2} fps)");
}

#[cfg(test)]
mod tests {
    use indicatif::ProgressDrawTarget;

    use super::*;

    #[test]
    fn bar_template_is_pinned() {
        let expected = "{msg} [{elapsed_precise}] [{bar:40}] {pos}/{len} frames ({fps}, eta {eta})";

        assert_eq!(BAR_TEMPLATE, expected);
    }

    #[test]
    fn bar_template_parses() {
        let style = bar_style();

        assert!(style.is_ok());
    }

    #[test]
    fn frame_bar_uses_total_as_length() {
        let bar = frame_bar("encoding", 10);

        assert_eq!(bar.length(), Some(10));
    }

    #[test]
    fn finish_marks_the_bar_done() {
        let bar = frame_bar("encoding", 10);
        bar.inc(10);

        finish(&bar, "encoded");

        assert!(bar.is_finished());
    }

    #[test]
    fn hidden_target_hides_frame_bars() {
        let hidden = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());

        let bar = frame_bar_on(&hidden, "scoring", 10);

        assert!(bar.is_hidden());
    }

    #[test]
    fn fps_key_formats_two_decimals() {
        assert_eq!(format_fps(7.5), "7.50 fps");
    }

    #[test]
    fn frames_per_second_is_zero_without_elapsed_time() {
        let fps = frames_per_second(10, 0.0);

        assert_eq!(fps, 0.0);
    }
}
