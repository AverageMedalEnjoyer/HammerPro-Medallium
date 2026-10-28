use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static PROCESS_START: OnceLock<Instant> = OnceLock::new();
static MAIN_START: OnceLock<Instant> = OnceLock::new();

pub fn init_process_start() -> Instant {
    let now = *MAIN_START.get_or_init(Instant::now);
    *PROCESS_START.get_or_init(|| now.checked_sub(time_before_main()).unwrap_or(now))
}

pub fn process_start() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}

#[cfg(windows)]
fn time_before_main() -> Duration {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let ticks = |t: &FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    let empty = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user, mut now) = (empty, empty, empty, empty, empty);
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) != 0 };
    if !ok {
        return Duration::ZERO;
    }
    unsafe { GetSystemTimePreciseAsFileTime(&mut now) };
    let elapsed = Duration::from_nanos(ticks(&now).saturating_sub(ticks(&created)).saturating_mul(100));
    if elapsed > Duration::from_secs(60) { Duration::ZERO } else { elapsed }
}

#[cfg(not(windows))]
fn time_before_main() -> Duration {
    Duration::ZERO
}

pub fn ms(d: Duration) -> u64 {
    u64::try_from((d.as_micros() + 500) / 1000).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug)]
pub struct Timeline {
    origin: Instant,
    marks: Vec<(&'static str, Instant)>,
}

impl Timeline {
    pub fn new(origin: Instant) -> Self {
        Self { origin, marks: Vec::new() }
    }

    pub fn mark(&mut self, name: &'static str) {
        self.mark_at(name, Instant::now());
    }

    pub fn mark_at(&mut self, name: &'static str, at: Instant) {
        if !self.has(name) {
            self.marks.push((name, at.max(self.origin)));
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.marks.iter().any(|(n, _)| *n == name)
    }

    pub fn at(&self, name: &str) -> Option<Duration> {
        self.marks.iter().find(|(n, _)| *n == name).map(|(_, t)| t.duration_since(self.origin))
    }

    pub fn total(&self) -> Duration {
        self.marks.iter().map(|(_, t)| t.duration_since(self.origin)).max().unwrap_or_default()
    }

    pub fn key_values(&self, prefix: &str) -> String {
        let mut out = String::new();
        for (name, t) in &self.marks {
            if !out.is_empty() {
                out.push(' ');
            }
            let _ = write!(out, "{prefix}{name}_ms={}", ms(t.duration_since(self.origin)));
        }
        out
    }

    pub fn phases(&self) -> String {
        let mut marks = self.marks.clone();
        marks.sort_by_key(|(_, t)| *t);
        let mut prev = self.origin;
        let mut out = String::new();
        for (name, t) in marks {
            if !out.is_empty() {
                out.push_str(", ");
            }
            let _ = write!(out, "{name} {} ms", ms(t.duration_since(prev)));
            prev = t;
        }
        out
    }
}

#[derive(Debug)]
pub struct StartupReport {
    timeline: Timeline,
    reported: bool,
}

impl Default for StartupReport {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupReport {
    pub fn new() -> Self {
        let mut timeline = Timeline::new(process_start());
        if let Some(main) = MAIN_START.get() {
            timeline.mark_at("main", *main);
        }
        timeline.mark("window");
        Self { timeline, reported: false }
    }

    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    pub fn mark(&mut self, name: &'static str) {
        self.timeline.mark(name);
    }

    pub fn after_frame(&mut self, content_known: bool, fgd_known: bool) -> Option<Duration> {
        self.timeline.mark("first_frame");
        if content_known {
            self.timeline.mark("content");
        }
        if fgd_known {
            self.timeline.mark("fgd");
        }
        if self.reported || !(self.timeline.has("content") && self.timeline.has("fgd")) {
            return None;
        }
        self.reported = true;
        let first = self.timeline.at("first_frame").unwrap_or_default();
        log::info!(
            "startup: first frame after {} ms, everything ready after {} ms ({})",
            ms(first),
            ms(self.timeline.total()),
            self.timeline.phases()
        );
        Some(first)
    }
}

#[derive(Clone, Debug)]
pub struct OpenReport {
    pub name: String,
    pub bytes: u64,
    pub timeline: Timeline,
    pub longest_frame: Duration,
    pub sync: Duration,
    pub sync_count: u64,
}

impl OpenReport {
    pub fn new(name: String, requested: Instant, timings: &hp_vmf::LoadTimings, worker: Duration) -> Self {
        let mut timeline = Timeline::new(requested);
        let mut at = requested;
        for (phase, d) in [
            ("read", timings.read),
            ("decode", timings.decode),
            ("scan", timings.scan),
            ("build", timings.build),
            ("scene", worker.saturating_sub(timings.total())),
        ] {
            at += d;
            timeline.mark_at(phase, at);
        }
        Self {
            name,
            bytes: timings.bytes,
            timeline,
            longest_frame: Duration::ZERO,
            sync: Duration::ZERO,
            sync_count: 0,
        }
    }

    pub fn log(&self) {
        log::info!(
            "open {} ({:.1} MB): drawn after {} ms ({}; scene upload {} ms), longest frame {} ms",
            self.name,
            self.bytes as f64 / 1.0e6,
            ms(self.timeline.total()),
            self.timeline.phases(),
            ms(self.sync),
            ms(self.longest_frame)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_reports_lay_the_worker_phases_end_to_end() {
        let requested = Instant::now();
        let timings = hp_vmf::LoadTimings {
            bytes: 2_000_000,
            read: Duration::from_millis(10),
            decode: Duration::from_millis(5),
            scan: Duration::from_millis(20),
            build: Duration::from_millis(100),
        };
        let mut r = OpenReport::new("big.vmf".into(), requested, &timings, Duration::from_millis(185));
        assert_eq!(r.timeline.at("read"), Some(Duration::from_millis(10)));
        assert_eq!(r.timeline.at("build"), Some(Duration::from_millis(135)));
        assert_eq!(r.timeline.at("scene"), Some(Duration::from_millis(185)));
        r.timeline.mark_at("install", requested + Duration::from_millis(200));
        assert_eq!(
            r.timeline.phases(),
            "read 10 ms, decode 5 ms, scan 20 ms, build 100 ms, scene 50 ms, install 15 ms"
        );
    }

    #[test]
    fn the_startup_summary_waits_for_the_content() {
        let mut s = StartupReport::new();
        s.mark("app");
        assert_eq!(s.after_frame(false, true), None);
        assert!(s.timeline().has("first_frame") && s.timeline().has("fgd"));
        let first = s.after_frame(true, true).expect("reported once everything is known");
        assert_eq!(Some(first), s.timeline().at("first_frame"));
        assert_eq!(s.after_frame(true, true), None, "reported once");
    }

    #[test]
    fn timelines_report_phases_and_totals() {
        let origin = Instant::now();
        let mut t = Timeline::new(origin);
        t.mark_at("window", origin + Duration::from_millis(200));
        t.mark_at("app", origin + Duration::from_millis(250));
        t.mark_at("app", origin + Duration::from_millis(900));
        t.mark_at("first_frame", origin + Duration::from_millis(300));
        assert_eq!(t.at("app"), Some(Duration::from_millis(250)));
        assert_eq!(t.total(), Duration::from_millis(300));
        assert_eq!(t.phases(), "window 200 ms, app 50 ms, first_frame 50 ms");
        assert_eq!(t.key_values("startup_"), "startup_window_ms=200 startup_app_ms=250 startup_first_frame_ms=300");
        assert!(t.has("window") && !t.has("content"));
        t.mark_at("early", origin.checked_sub(Duration::from_millis(5)).unwrap_or(origin));
        assert_eq!(t.at("early"), Some(Duration::ZERO));
    }

    #[test]
    fn the_process_start_is_before_now() {
        let start = init_process_start();
        assert!(start <= Instant::now());
        assert_eq!(process_start(), start);
        assert_eq!(ms(Duration::from_micros(1499)), 1);
        assert_eq!(ms(Duration::from_micros(1500)), 2);
    }
}
