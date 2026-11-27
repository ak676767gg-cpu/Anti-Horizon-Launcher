// Downloads: one shared HTTP client (kept-alive connections), many files at once, every file checked against its
// SHA-1 when it arrives and written through a .part file, so a broken download never looks finished.
use crate::store::e;
use futures::{stream, StreamExt};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const PARALLEL: usize = 48;

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("AntiHorizonLauncher/", env!("CARGO_PKG_VERSION"), " (+https://anti-horizon.com)"))
            .pool_max_idle_per_host(PARALLEL)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(600))
            .build()
            .expect("HTTP client")
    })
}

pub fn host(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or(url)
}

pub async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = client().get(url).send().await.map_err(|err| format!("Could not reach {} ({err})", host(url)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("{} answered {status}", host(url)));
    }
    resp.json::<T>().await.map_err(e)
}

#[derive(Clone, Debug)]
pub struct Job {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(bytes);
    hex(&h.finalize())
}

/// Present = exists at the expected size. Hashes are checked on download, not on every launch, which keeps a
/// second launch near instant even with thousands of asset files.
pub fn present(job: &Job) -> bool {
    match std::fs::metadata(&job.path) {
        Ok(m) => m.is_file() && match job.size { Some(s) => m.len() == s, None => m.len() > 0 },
        Err(_) => false,
    }
}

/// Streams to disk through a .part file, hashing as it goes, so even Java's 100 MB modules file never sits in
/// memory whole.
async fn fetch_once(job: &Job) -> Result<(), String> {
    let mut resp = client().get(&job.url).send().await.map_err(e)?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    if let Some(dir) = job.path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(e)?;
    }
    let part = PathBuf::from(format!("{}.part", job.path.display()));
    let mut file = tokio::fs::File::create(&part).await.map_err(e)?;
    let mut hasher = Sha1::new();
    let mut written = 0u64;
    while let Some(chunk) = resp.chunk().await.map_err(e)? {
        hasher.update(&chunk);
        written += chunk.len() as u64;
        file.write_all(&chunk).await.map_err(e)?;
    }
    file.flush().await.map_err(e)?;
    drop(file);
    let good = job.sha1.as_ref().map_or(true, |want| &hex(&hasher.finalize()) == want) && job.size.map_or(true, |size| size == written);
    if !good {
        let _ = tokio::fs::remove_file(&part).await;
        return Err("checksum mismatch".into());
    }
    tokio::fs::rename(&part, &job.path).await.map_err(e)?;
    Ok(())
}

/// One file, up to three tries.
pub async fn fetch(job: &Job) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..3u64 {
        match fetch_once(job).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = err;
                tokio::time::sleep(Duration::from_millis(400 * (attempt + 1))).await;
            }
        }
    }
    Err(format!("Could not download {} ({last})", job.url))
}

/// Everything not already present, PARALLEL at a time; progress(done, total) as it goes.
pub async fn download_all<F>(jobs: Vec<Job>, progress: F) -> Result<(), String>
where
    F: Fn(usize, usize) + Send + Sync,
{
    let mut seen = HashSet::new();
    let todo: Vec<Job> = jobs.into_iter().filter(|j| seen.insert(j.path.clone()) && !present(j)).collect();
    let total = todo.len();
    progress(0, total);
    if total == 0 {
        return Ok(());
    }
    let done = AtomicUsize::new(0);
    let (done, progress) = (&done, &progress);
    // Each task owns its job: a borrowed one makes the future's lifetime too general for the compiler to call Send.
    let results: Vec<Result<(), String>> = stream::iter(todo)
        .map(|job| async move {
            let r = fetch(&job).await;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n == total || n % 16 == 0 {
                progress(n, total);
            }
            r
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;
    for r in results {
        r?;
    }
    Ok(())
}
