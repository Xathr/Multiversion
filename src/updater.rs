//! The update check and download as a sequence of HTTP requests that can be driven one step at a
//! time: by the proxy's reactor while players are online, or by blocking at startup.

use std::sync::Mutex;

use crate::http::{self, Pending, Progress, Reply};
use crate::update::{self, Status};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Where a downloaded update is saved, as the server admin sees it.
pub const UPDATE_FILE: &str = "plugins/data/multiversion/update/Multiversion.wasm";

static STATUS: Mutex<Status> = Mutex::new(Status::Off);
static FOLDER: Mutex<String> = Mutex::new(String::new());
static REQUESTED: Mutex<Option<Job>> = Mutex::new(None);

/// What the last check found.
pub fn status() -> Status {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
fn set_status(status: Status) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = status;
}
pub fn set_data_folder(folder: &str) {
    *FOLDER.lock().unwrap_or_else(|e| e.into_inner()) = folder.to_string();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Check,
    Download,
}

/// Asks the proxy to run `job` when it next has a moment. False if one is already waiting.
pub fn request(job: Job) -> bool {
    let mut slot = REQUESTED.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }
    *slot = Some(job);
    true
}
pub fn take_request() -> Option<Job> {
    REQUESTED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

enum Step {
    /// Asking the Market for the latest version.
    Check,
    /// Fetching the Market's signing key, to check the download of `latest`.
    Key { latest: String },
    /// Downloading the signed plugin file.
    File { latest: String, key: [u8; 32] },
}

/// A job in progress: one request at a time.
pub struct Flow {
    job: Job,
    step: Step,
    pending: Pending,
    redirects: usize,
}

pub enum FlowProgress {
    /// Still going: wait on `pollable()` and call `advance` again.
    Pending(Flow),
    /// Finished; the status is updated and these lines are for the console.
    Done(Vec<String>),
}

fn file_path() -> (String, String) {
    let dir = format!("{}/update", FOLDER.lock().unwrap_or_else(|e| e.into_inner()));
    let path = format!("{dir}/{}", update::ASSET);
    (dir, path)
}

fn finish(status: Status) -> FlowProgress {
    set_status(status.clone());
    FlowProgress::Done(lines(&status))
}

/// A failed download leaves the "newer version is out" status as it was.
fn download_failed(latest: String, why: &str) -> FlowProgress {
    set_status(Status::Available { latest, downloaded: None });
    let mut out = vec![crate::color::bad(&format!("update download failed: {why}"))];
    out.extend(lines(&status()));
    FlowProgress::Done(out)
}

impl Flow {
    /// Starts `job`. A download uses the version found by the last check.
    pub fn start(job: Job) -> FlowProgress {
        let (step, url, max) = match job {
            Job::Check => (Step::Check, update::check_url(VERSION), 64 * 1024),
            Job::Download => match status() {
                Status::Available { latest, .. } => (Step::Key { latest }, update::PUBLIC_KEY_URL.to_string(), 4096),
                other => return FlowProgress::Done([vec!["there is no newer version to download".to_string()], lines(&other)].concat()),
            },
        };
        Self::request(job, step, &url, max, 0)
    }

    fn request(job: Job, step: Step, url: &str, max: usize, redirects: usize) -> FlowProgress {
        match http::start(url, max) {
            Ok(pending) => FlowProgress::Pending(Flow { job, step, pending, redirects }),
            Err(e) => Self::failed(step, e),
        }
    }

    fn failed(step: Step, why: String) -> FlowProgress {
        match step {
            Step::Check => finish(Status::Failed(why)),
            Step::Key { latest } | Step::File { latest, .. } => download_failed(latest, &why),
        }
    }

    pub fn pollable(&self) -> wasip2::io::poll::Pollable {
        self.pending.pollable()
    }

    /// Takes the job as far as it can go without waiting.
    pub fn advance(self) -> FlowProgress {
        let Flow { job, step, pending, redirects } = self;
        let body = match http::advance(pending) {
            Progress::Pending(pending) => return FlowProgress::Pending(Flow { job, step, pending, redirects }),
            Progress::Done(Ok(Reply::Body(body))) => body,
            Progress::Done(Ok(Reply::Redirect(to))) => {
                if redirects >= http::MAX_REDIRECTS {
                    return Self::failed(step, "too many redirects".into());
                }
                let max = match step {
                    Step::Check => 64 * 1024,
                    Step::Key { .. } => 4096,
                    Step::File { .. } => MAX_PLUGIN_BYTES,
                };
                return Self::request(job, step, &to, max, redirects + 1);
            }
            Progress::Done(Err(e)) => return Self::failed(step, e),
        };
        match step {
            Step::Check => match update::parse_check(&body) {
                Ok(latest) if update::is_newer(&latest, VERSION) => {
                    // Keep "already downloaded" if this version was fetched before.
                    let downloaded = match status() {
                        Status::Available { latest: had, downloaded } if had == latest => downloaded,
                        _ => None,
                    };
                    finish(Status::Available { latest, downloaded })
                }
                Ok(latest) => finish(Status::Current { latest }),
                Err(e) => finish(Status::Failed(e)),
            },
            Step::Key { latest } => {
                let key = match update::parse_public_key(&body) {
                    Ok(key) => key,
                    Err(e) => return download_failed(latest, &e),
                };
                let (_, path) = file_path();
                if std::fs::read(&path).is_ok_and(|have| update::verify_download(&have, &key, &latest).is_ok()) {
                    return finish(Status::Available { latest, downloaded: Some(UPDATE_FILE.into()) }); // already there
                }
                Self::request(job, Step::File { latest, key }, update::DOWNLOAD_URL, MAX_PLUGIN_BYTES, 0)
            }
            Step::File { latest, key } => {
                if let Err(e) = update::verify_download(&body, &key, &latest) {
                    return download_failed(latest, &format!("{e}; nothing was saved"));
                }
                let (dir, path) = file_path();
                if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, body)) {
                    return download_failed(latest, &format!("{UPDATE_FILE}: {e}"));
                }
                finish(Status::Available { latest, downloaded: Some(UPDATE_FILE.into()) })
            }
        }
    }
}

/// The Market refuses plugin files over 5 MB; leave room for that to grow.
const MAX_PLUGIN_BYTES: usize = 32 * 1024 * 1024;

/// Runs `job` to the end, blocking. Only for startup, before the proxy carries players.
pub fn run_blocking(job: Job) -> Vec<String> {
    let mut progress = Flow::start(job);
    loop {
        match progress {
            FlowProgress::Done(lines) => return lines,
            FlowProgress::Pending(flow) => {
                flow.pollable().block();
                progress = flow.advance();
            }
        }
    }
}

/// The update status as console lines.
pub fn lines(status: &Status) -> Vec<String> {
    use crate::color;
    match status {
        Status::Off => vec![color::dim("update check is off ([updates] check = false in config.toml)")],
        Status::Current { latest } => vec![format!("{} {}", color::good(&format!("multiversion v{VERSION} is up to date")), color::dim(&format!("(latest on the Pumpkin Market: v{latest})")))],
        Status::Failed(why) => vec![color::dim(&format!("update check failed: {why}"))],
        Status::Available { latest, downloaded } => {
            let mut out = vec![
                format!("{} {}", color::warn(&format!("multiversion v{latest} is out")), color::dim(&format!("(this server runs v{VERSION})"))),
                format!("{} {}", color::dim("Pumpkin Market:"), color::version(update::PAGE_URL)),
            ];
            out.push(match downloaded {
                Some(path) => format!("{} {path}. Stop the server and copy it over plugins/Multiversion.wasm.", color::good("downloaded, signed by the Market:")),
                None => color::dim("run /multiversion update download to fetch the signed file"),
            });
            out
        }
    }
}
