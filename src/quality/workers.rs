use crate::quality::SourceFile;
use anyhow::Result;

// A batch has at most the configured number of files. Results are collected in
// input order, so parallel parsing does not alter selection or comparison order.
pub(crate) fn batch<T, F>(sources: &[(SourceFile, String)], analyze: F) -> Result<Vec<T>>
where
    T: Send,
    F: Fn(&SourceFile, &str) -> T + Sync,
{
    std::thread::scope(|scope| {
        let mut handles = vec![];
        for (file, source) in sources {
            let analyze = &analyze;
            handles.push(
                std::thread::Builder::new()
                    .name("archguard-source".into())
                    .spawn_scoped(scope, move || analyze(file, source))?,
            );
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("source worker panicked"))
            })
            .collect()
    })
}
