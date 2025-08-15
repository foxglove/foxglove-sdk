use std::path::Path;

/// Returns the summary section from a finished MCAP file. The summary section must exist.
pub fn read_summary(mcap: impl AsRef<Path>) -> mcap::Summary {
    let written = std::fs::read(mcap).expect("failed to read mcap");
    let summary = mcap::Summary::read(written.as_ref()).expect("failed to read summary");
    summary.expect("missing summary")
}
