//! Day and night themes on a schedule: a day theme from sunrise, a night theme
//! from sunset (computed here, from a city or typed latitude and longitude),
//! or from two fixed clock times. No network and no Location permission: the
//! place comes from a built-in city list, and the sun from the NOAA solar
//! equations (the ones behind the NOAA sunrise/sunset calculator), which are
//! accurate to about a minute outside the polar circles.
//!
//! Settings live in `settings.toml` under `[schedule]`. What has been applied
//! is remembered in `~/.config/palette/schedule.state` (see `apply_now`).
//!
//! Test hooks (read only by this module): `PALETTE_NOW` (unix seconds) and
//! `PALETTE_UTC_OFFSET` (minutes east of UTC) replace the real clock and time
//! zone, so the command-line tests run at a fixed moment anywhere.

mod cities;

use std::fs;

use anyhow::{Context, Result, bail};

pub use cities::{CITIES, City};

use crate::paths::Paths;
use crate::{Switch, settings, state, system, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Day,
    Night,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Day => "day",
            Phase::Night => "night",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Switch at sunrise and sunset.
    Sun,
    /// Switch at two clock times.
    Fixed,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Sun => "sun",
            Mode::Fixed => "fixed",
        }
    }
}

/// The `[schedule]` table of `settings.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub on: bool,
    pub day: String,
    pub night: String,
    pub mode: Mode,
    /// A name from the built-in list (used unless `lat` and `lon` are both set).
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Fixed mode: minutes after midnight, local time.
    pub day_at: u16,
    pub night_at: u16,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            on: false,
            day: String::new(),
            night: String::new(),
            mode: Mode::Sun,
            city: None,
            lat: None,
            lon: None,
            day_at: 7 * 60,
            night_at: 19 * 60,
        }
    }
}

impl Schedule {
    pub fn theme_for(&self, phase: Phase) -> &str {
        match phase {
            Phase::Day => &self.day,
            Phase::Night => &self.night,
        }
    }

    /// Where the sun is computed for: typed coordinates win over a city.
    pub fn location(&self) -> Result<(f64, f64, String)> {
        if let (Some(lat), Some(lon)) = (self.lat, self.lon) {
            return Ok((lat, lon, format!("{lat:.2}, {lon:.2}")));
        }
        let name = self.city.as_deref().context(
            "no place set: pass --city <name> (palette schedule cities) or --lat and --lon",
        )?;
        let city = find_city(name).with_context(|| unknown_city(name))?;
        Ok((
            city.lat,
            city.lon,
            format!("{}, {}", city.name, city.country),
        ))
    }
}

// ---------------------------------------------------------------- settings.toml

/// Read the `[schedule]` table. Anything missing or malformed falls back to
/// the default, so a hand-edited file never stops Palette from starting.
pub(crate) fn from_toml(table: Option<&toml::Value>) -> Schedule {
    let mut s = Schedule::default();
    let Some(t) = table.and_then(|v| v.as_table()) else {
        return s;
    };
    let text = |k: &str| t.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let num = |k: &str| {
        t.get(k)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
    };
    s.on = t.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
    s.day = text("day").unwrap_or_default();
    s.night = text("night").unwrap_or_default();
    s.mode = match text("mode").as_deref() {
        Some("fixed") => Mode::Fixed,
        _ => Mode::Sun,
    };
    s.city = text("city").filter(|c| !c.is_empty());
    s.lat = num("lat").filter(|v| (-90.0..=90.0).contains(v));
    s.lon = num("lon").filter(|v| (-180.0..=180.0).contains(v));
    if let Some(m) = text("day_at").and_then(|v| parse_hm(&v).ok()) {
        s.day_at = m;
    }
    if let Some(m) = text("night_at").and_then(|v| parse_hm(&v).ok()) {
        s.night_at = m;
    }
    s
}

pub(crate) fn to_toml(s: &Schedule) -> String {
    let quote = |v: &str| toml::Value::String(v.to_string()).to_string();
    let mut out = format!(
        "\n# Day and night themes (`palette schedule`): sunrise/sunset for a place, or fixed times\n\
         [schedule]\non = {}\nday = {}\nnight = {}\nmode = \"{}\"\n",
        s.on,
        quote(&s.day),
        quote(&s.night),
        s.mode.as_str()
    );
    if let Some(city) = &s.city {
        out.push_str(&format!("city = {}\n", quote(city)));
    }
    if let (Some(lat), Some(lon)) = (s.lat, s.lon) {
        out.push_str(&format!("lat = {lat}\nlon = {lon}\n"));
    }
    out.push_str(&format!(
        "day_at = \"{}\"\nnight_at = \"{}\"\n",
        fmt_hm(s.day_at),
        fmt_hm(s.night_at)
    ));
    out
}

/// `7`, `7:05`, `07:05` → minutes after midnight.
pub fn parse_hm(text: &str) -> Result<u16> {
    let t = text.trim();
    let (h, m) = t.split_once(':').unwrap_or((t, "0"));
    let (h, m): (u16, u16) = match (h.trim().parse(), m.trim().parse()) {
        (Ok(h), Ok(m)) => (h, m),
        _ => bail!("`{text}` isn't a time: use 24-hour HH:MM, like 07:00"),
    };
    if h > 23 || m > 59 {
        bail!("`{text}` isn't a time: hours 0-23, minutes 0-59");
    }
    Ok(h * 60 + m)
}

pub fn fmt_hm(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

// ---------------------------------------------------------------- cities

fn fold(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Case-insensitive exact match on the city name.
pub fn find_city(name: &str) -> Option<&'static City> {
    let want = fold(name);
    CITIES.iter().find(|c| fold(c.name) == want)
}

/// Cities whose name or country contains `query`, best matches first (exact,
/// then name starts with, then the rest). Empty query: the whole list.
pub fn search_cities(query: &str) -> Vec<&'static City> {
    let q = fold(query);
    let mut hits: Vec<(u8, &City)> = CITIES
        .iter()
        .filter_map(|c| {
            let (name, country) = (fold(c.name), fold(c.country));
            let rank = if q.is_empty() || name == q {
                0
            } else if name.starts_with(&q) {
                1
            } else if name.contains(&q) {
                2
            } else if country.contains(&q) {
                3
            } else {
                return None;
            };
            Some((rank, c))
        })
        .collect();
    hits.sort_by_key(|(rank, c)| (*rank, c.name));
    hits.into_iter().map(|(_, c)| c).collect()
}

fn unknown_city(name: &str) -> String {
    let near: Vec<&str> = search_cities(name).iter().take(5).map(|c| c.name).collect();
    if near.is_empty() {
        format!(
            "no city called `{name}` in the built-in list: see `palette schedule cities`, or pass --lat and --lon"
        )
    } else {
        format!(
            "no city called `{name}`: did you mean {}? (or pass --lat and --lon)",
            near.join(", ")
        )
    }
}

/// A time-zone name like `Asia/Singapore` → the matching built-in city.
pub fn city_for_zone(zone: &str) -> Option<&'static City> {
    let place = zone.rsplit('/').next()?.replace('_', " ");
    let alias = match place.as_str() {
        "Kolkata" | "Calcutta" => "Delhi",
        "Saigon" => "Ho Chi Minh City",
        "Kiev" => "Kyiv",
        "Yangon" | "Rangoon" => "Yangon",
        "Katmandu" => "Kathmandu",
        "Bangalore" => "Bangalore",
        "Denpasar" | "Makassar" => "Bali (Denpasar)",
        "Ho Chi Minh" => "Ho Chi Minh City",
        "Sao Paulo" => "Sao Paulo",
        other => other,
    };
    find_city(alias)
}

/// A city to suggest, from the Mac's time zone (`/etc/localtime`).
pub fn suggest_city() -> Option<&'static City> {
    city_for_zone(&system::time_zone_name()?)
}

// ---------------------------------------------------------------- calendar maths

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date for days since 1970-01-01: (year, month, day).
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

// ---------------------------------------------------------------- the sun (NOAA)

/// Solar zenith at sunrise/sunset: 90 degrees plus 50 arc-minutes for the
/// Sun's radius and atmospheric refraction.
const ZENITH: f64 = 90.833;

/// What the sun does on one calendar day at a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SunDay {
    /// Unix seconds of sunrise and sunset.
    Normal { rise: i64, set: i64 },
    /// Polar day: the sun does not set.
    AlwaysUp,
    /// Polar night: the sun does not rise.
    AlwaysDown,
}

/// Sun's declination (radians) and the equation of time (minutes) at a moment,
/// from the NOAA spreadsheet formulas. `jd` is the Julian day.
fn solar_position(jd: f64) -> (f64, f64) {
    let t = (jd - 2_451_545.0) / 36_525.0;
    let l0 = (280.46646 + t * (36_000.769_83 + t * 0.000_303_2)).rem_euclid(360.0);
    let m = 357.52911 + t * (35_999.050_29 - 0.000_153_7 * t);
    let e = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
    let (mr, l0r) = (m.to_radians(), l0.to_radians());
    let c = mr.sin() * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
        + (2.0 * mr).sin() * (0.019_993 - 0.000_101 * t)
        + (3.0 * mr).sin() * 0.000_289;
    let omega = 125.04 - 1934.136 * t;
    let lambda = (l0 + c - 0.005_69 - 0.004_78 * omega.to_radians().sin()).to_radians();
    let obliq = 23.0
        + (26.0 + (21.448 - t * (46.815 + t * (0.000_59 - t * 0.001_813))) / 60.0) / 60.0
        + 0.002_56 * omega.to_radians().cos();
    let obliq = obliq.to_radians();
    let decl = (obliq.sin() * lambda.sin()).asin();
    let y = (obliq / 2.0).tan().powi(2);
    let eq = 4.0
        * (y * (2.0 * l0r).sin() - 2.0 * e * mr.sin() + 4.0 * e * y * mr.sin() * (2.0 * l0r).cos()
            - 0.5 * y * y * (4.0 * l0r).sin()
            - 1.25 * e * e * (2.0 * mr).sin())
        .to_degrees();
    (decl, eq)
}

/// Minutes after 00:00 UTC of day `days` at which the sun crosses the horizon,
/// or the polar state if it doesn't. Two passes: the sun's position is taken
/// at the guessed time, then again at the first answer.
fn horizon_minutes(lat: f64, lon: f64, days: i64, rising: bool) -> Result<f64, SunDay> {
    let mut minutes = 720.0 - 4.0 * lon;
    for _ in 0..2 {
        let jd = days as f64 + 2_440_587.5 + minutes / 1440.0;
        let (decl, eq) = solar_position(jd);
        let latr = lat.to_radians();
        let cos_ha =
            ZENITH.to_radians().cos() / (latr.cos() * decl.cos()) - latr.tan() * decl.tan();
        if cos_ha > 1.0 {
            return Err(SunDay::AlwaysDown);
        }
        if cos_ha < -1.0 {
            return Err(SunDay::AlwaysUp);
        }
        let ha = cos_ha.acos().to_degrees();
        minutes = 720.0 - 4.0 * (lon + if rising { ha } else { -ha }) - eq;
    }
    Ok(minutes)
}

/// Sunrise and sunset for the calendar day numbered `days` (days since the
/// epoch) at (`lat`, `lon`). The day is the one whose solar noon at that
/// longitude falls on it, which matches the local date everywhere but in
/// time zones far from their longitude, where an event can move by a day only
/// near midnight.
pub fn sun_day(lat: f64, lon: f64, days: i64) -> SunDay {
    let at = |m: f64| days * 86_400 + (m * 60.0).round() as i64;
    match (
        horizon_minutes(lat, lon, days, true),
        horizon_minutes(lat, lon, days, false),
    ) {
        (Ok(rise), Ok(set)) => SunDay::Normal {
            rise: at(rise),
            set: at(set),
        },
        (Err(state), _) | (_, Err(state)) => state,
    }
}

// ---------------------------------------------------------------- what's on now

/// Minutes east of UTC at a moment (so daylight saving is followed).
pub type Offset<'a> = &'a dyn Fn(i64) -> i32;

/// The state of a schedule at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Now {
    pub phase: Phase,
    /// When this phase began: the boundary last passed. `None` in polar day or
    /// night with no boundary within a year.
    pub since: Option<i64>,
    /// The next change and what it changes to; `None` if the sun doesn't cross
    /// the horizon for over a year (or day and night times are equal).
    pub next: Option<(i64, Phase)>,
    /// Today's switch times (local date): the day theme's and the night theme's.
    pub day_starts: Option<i64>,
    pub night_starts: Option<i64>,
    /// Polar day or night today.
    pub polar: Option<Phase>,
}

/// How far (days) to look for a boundary before giving up.
const HORIZON_DAYS: i64 = 400;

fn local_days(ts: i64, offset: Offset) -> i64 {
    (ts + i64::from(offset(ts)) * 60).div_euclid(86_400)
}

/// A day's boundaries (sorted), and the polar state if it has none.
type DayBoundaries = (Vec<(i64, Phase)>, Option<Phase>);

/// The boundaries on one local day, with the sun state if it has none.
fn boundaries(s: &Schedule, days: i64, offset: Offset) -> Result<DayBoundaries> {
    match s.mode {
        Mode::Fixed => {
            let at = |minutes: u16| {
                let guess = days * 86_400 + i64::from(minutes) * 60;
                guess - i64::from(offset(guess - i64::from(offset(guess)) * 60)) * 60
            };
            let mut list = vec![(at(s.day_at), Phase::Day), (at(s.night_at), Phase::Night)];
            list.sort();
            Ok((list, None))
        }
        Mode::Sun => {
            let (lat, lon, _) = s.location()?;
            Ok(match sun_day(lat, lon, days) {
                SunDay::Normal { rise, set } => {
                    let mut list = vec![(rise, Phase::Day), (set, Phase::Night)];
                    list.sort();
                    (list, None)
                }
                SunDay::AlwaysUp => (Vec::new(), Some(Phase::Day)),
                SunDay::AlwaysDown => (Vec::new(), Some(Phase::Night)),
            })
        }
    }
}

/// Pure: which theme slot is on at `now`, and when it changes. Errors only if
/// the place is unknown.
pub fn evaluate(s: &Schedule, now: i64, offset: Offset) -> Result<Now> {
    if s.mode == Mode::Fixed && s.day_at == s.night_at {
        bail!("the day and night times are the same: pick two different times");
    }
    let today = local_days(now, offset);
    let (todays, polar) = boundaries(s, today, offset)?;
    let find = |step: i64| -> Result<Option<(i64, Phase)>> {
        for k in 0..=HORIZON_DAYS {
            let (list, _) = boundaries(s, today + step * k, offset)?;
            let hit = if step < 0 {
                list.into_iter().rfind(|(t, _)| *t <= now)
            } else {
                list.into_iter().find(|(t, _)| *t > now)
            };
            if hit.is_some() {
                return Ok(hit);
            }
        }
        Ok(None)
    };
    let prev = find(-1)?;
    let next = find(1)?;
    let phase = match (prev, polar) {
        // Polar day or night today: the last sunrise or sunset is weeks old.
        (_, Some(phase)) => phase,
        (Some((_, phase)), None) => phase,
        // No boundary in over a year and not polar today: take the opposite of
        // whatever is next, which is the phase we are in.
        (None, None) => next.map_or(Phase::Night, |(_, p)| match p {
            Phase::Day => Phase::Night,
            Phase::Night => Phase::Day,
        }),
    };
    let first = |p: Phase| todays.iter().find(|(_, q)| *q == p).map(|(t, _)| *t);
    Ok(Now {
        phase,
        since: prev.map(|(t, _)| t),
        next,
        day_starts: first(Phase::Day),
        night_starts: first(Phase::Night),
        polar,
    })
}

// ---------------------------------------------------------------- the real clock

/// The moment to use: `PALETTE_NOW` if set (tests), else the system clock.
pub fn now_unix() -> i64 {
    std::env::var("PALETTE_NOW")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64)
        })
}

/// Minutes east of UTC at `ts` in the Mac's time zone (`PALETTE_UTC_OFFSET`
/// overrides, for tests). UTC if the system can't say.
pub fn system_offset(ts: i64) -> i32 {
    if let Some(v) = std::env::var("PALETTE_UTC_OFFSET")
        .ok()
        .and_then(|v| v.trim().parse().ok())
    {
        return v;
    }
    system::utc_offset_minutes(ts).unwrap_or(0)
}

/// `HH:MM` of a moment in local time.
pub fn local_hm(ts: i64, offset: Offset) -> String {
    let local = ts + i64::from(offset(ts)) * 60;
    let minute = local.rem_euclid(86_400) / 60;
    fmt_hm(minute as u16)
}

/// "today", "tomorrow" or the date, for a moment relative to `now`.
pub fn day_word(ts: i64, now: i64, offset: Offset) -> String {
    match local_days(ts, offset) - local_days(now, offset) {
        0 => "today".into(),
        1 => "tomorrow".into(),
        _ => {
            let (y, m, d) = civil_from_days(local_days(ts, offset));
            format!("on {y}-{m:02}-{d:02}")
        }
    }
}

/// Everything `palette schedule` and the Settings pane show.
#[derive(Debug, Clone)]
pub struct Status {
    pub schedule: Schedule,
    /// "Singapore, Singapore" or "1.35, 103.82"; `None` in fixed mode or when unset.
    pub place: Option<String>,
    /// Absent when the schedule can't be evaluated (no place yet).
    pub now: Option<Now>,
    pub problem: Option<String>,
    pub at: i64,
    pub utc_offset: i32,
}

impl Status {
    pub fn hm(&self, ts: i64) -> String {
        local_hm(ts, &system_offset)
    }

    pub fn day_word(&self, ts: i64) -> String {
        day_word(ts, self.at, &system_offset)
    }
}

/// `which_now`: the saved schedule at the real moment.
pub fn which_now(paths: &Paths) -> Result<Status> {
    let schedule = settings::load(paths)?.schedule;
    let at = now_unix();
    let place = (schedule.mode == Mode::Sun)
        .then(|| schedule.location().ok().map(|(_, _, p)| p))
        .flatten();
    let (now, problem) = match evaluate(&schedule, at, &system_offset) {
        Ok(n) => (Some(n), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    };
    Ok(Status {
        schedule,
        place,
        now,
        problem,
        at,
        utc_offset: system_offset(at),
    })
}

// ---------------------------------------------------------------- applying

fn state_file(paths: &Paths) -> std::path::PathBuf {
    paths.root().join("schedule.state")
}

fn last_boundary(paths: &Paths) -> Option<i64> {
    fs::read_to_string(state_file(paths))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn remember_boundary(paths: &Paths, boundary: i64) -> Result<()> {
    fs::create_dir_all(paths.root())?;
    fs::write(state_file(paths), format!("{boundary}\n")).context("saving schedule state")
}

fn forget_boundary(paths: &Paths) {
    let _ = fs::remove_file(state_file(paths));
}

/// What `apply_now` did.
#[derive(Debug)]
pub enum Applied {
    /// The schedule is off.
    Off,
    /// The right theme is already active (or this boundary was handled).
    Unchanged {
        phase: Phase,
        theme: String,
    },
    /// The user switched by hand since the last boundary; left alone until the next one.
    KeptManual {
        phase: Phase,
        wanted: String,
        active: String,
    },
    Switched {
        phase: Phase,
        switch: Switch,
    },
}

/// Switch to the scheduled theme if a boundary has passed since the last one
/// handled. The rule that keeps Palette from fighting the user:
///
/// * Each boundary (a sunrise, a sunset, a fixed time) is acted on **once**.
///   The moment it is handled, its timestamp goes in `schedule.state`.
/// * Every later check sees the same boundary as already handled and does
///   nothing, so a theme chosen by hand stays until the *next* boundary.
/// * Turning the schedule on, or changing it, forgets the state, so it takes
///   effect straight away.
///
/// Missed boundaries (Mac asleep, app closed) are caught up on the next check:
/// the theme for the *current* phase is applied, not each one in between.
pub fn apply_now(paths: &Paths) -> Result<Applied> {
    let status = which_now(paths)?;
    let s = &status.schedule;
    if !s.on {
        return Ok(Applied::Off);
    }
    let Some(now) = status.now else {
        bail!(
            status
                .problem
                .unwrap_or_else(|| "schedule can't be worked out".into())
        );
    };
    let wanted = s.theme_for(now.phase).to_string();
    let boundary = now.since.unwrap_or(0);
    let active = state::current_theme(paths)?;
    let unchanged = |phase| Applied::Unchanged {
        phase,
        theme: wanted.clone(),
    };
    if active.as_deref() == Some(wanted.as_str()) {
        remember_boundary(paths, boundary)?;
        return Ok(unchanged(now.phase));
    }
    if last_boundary(paths) == Some(boundary) {
        return Ok(Applied::KeptManual {
            phase: now.phase,
            wanted,
            active: active.unwrap_or_default(),
        });
    }
    let switch = crate::switch(paths, &wanted, false)?;
    remember_boundary(paths, boundary)?;
    Ok(Applied::Switched {
        phase: now.phase,
        switch,
    })
}

// ---------------------------------------------------------------- changing it

/// Check a schedule can run: both themes exist, and the place or times are good.
pub fn validate(paths: &Paths, s: &Schedule) -> Result<()> {
    let names: Vec<String> = theme::list(paths)?.into_iter().map(|(n, _)| n).collect();
    for (what, name) in [("day", &s.day), ("night", &s.night)] {
        if name.is_empty() {
            bail!("pick a {what} theme");
        }
        if !names.contains(name) {
            bail!("the {what} theme `{name}` isn't installed (palette list)");
        }
    }
    if s.mode == Mode::Fixed && s.day_at == s.night_at {
        bail!("the day and night times are the same: pick two different times");
    }
    if s.mode == Mode::Sun {
        s.location()?;
    }
    Ok(())
}

/// Save a schedule. If it is on, check it, forget what was handled, and apply
/// it now. Saving it off just saves (so themes can be picked beforehand).
pub fn save(paths: &Paths, new: Schedule) -> Result<Applied> {
    if new.on {
        validate(paths, &new)?;
    }
    let mut all = settings::load(paths)?;
    all.schedule = new;
    settings::save(paths, &all)?;
    forget_boundary(paths);
    apply_now(paths)
}

/// What `palette schedule on` was given; `None` keeps the saved value.
#[derive(Debug, Default, Clone)]
pub struct Update {
    pub day: Option<String>,
    pub night: Option<String>,
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// "07:00,19:00": fixed times (day first).
    pub at: Option<String>,
}

/// `palette schedule on`: merge the options into the saved schedule, turn it
/// on and apply it. Without a place the Mac's time zone suggests one.
pub fn enable(paths: &Paths, u: Update) -> Result<Applied> {
    let mut s = settings::load(paths)?.schedule;
    s.on = true;
    if let Some(day) = u.day {
        s.day = day;
    }
    if let Some(night) = u.night {
        s.night = night;
    }
    match (u.lat, u.lon) {
        (Some(lat), Some(lon)) => {
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                bail!("latitude is -90 to 90 and longitude -180 to 180");
            }
            s.lat = Some(lat);
            s.lon = Some(lon);
            s.city = None;
            s.mode = Mode::Sun;
        }
        (None, None) => {}
        _ => bail!("give both --lat and --lon"),
    }
    if let Some(name) = u.city {
        let city = find_city(&name).with_context(|| unknown_city(&name))?;
        s.city = Some(city.name.to_string());
        s.lat = None;
        s.lon = None;
        s.mode = Mode::Sun;
    }
    if let Some(at) = u.at {
        let (a, b) = at
            .split_once(',')
            .context("--at takes two times: --at 07:00,19:00 (day first)")?;
        s.day_at = parse_hm(a)?;
        s.night_at = parse_hm(b)?;
        s.mode = Mode::Fixed;
    }
    if s.mode == Mode::Sun && s.city.is_none() && s.lat.is_none() {
        let city = suggest_city().context(
            "which place? pass --city <name> (palette schedule cities), --lat and --lon, or --at 07:00,19:00",
        )?;
        s.city = Some(city.name.to_string());
    }
    save(paths, s)
}

/// `palette schedule off`: stop switching (the choices are kept).
pub fn disable(paths: &Paths) -> Result<()> {
    let mut all = settings::load(paths)?;
    all.schedule.on = false;
    settings::save(paths, &all)?;
    forget_boundary(paths);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(_: i64) -> i32 {
        0
    }

    /// Local HH:MM of a sunrise/sunset in minutes after midnight.
    fn minutes(ts: i64, off: i32) -> i64 {
        (ts + i64::from(off) * 60).rem_euclid(86_400) / 60
    }

    fn near(got: i64, want_h: i64, want_m: i64) {
        let want = want_h * 60 + want_m;
        assert!(
            (got - want).abs() <= 3,
            "got {:02}:{:02}, wanted {want_h:02}:{want_m:02} +/- 3 min",
            got / 60,
            got % 60
        );
    }

    fn sun(lat: f64, lon: f64, y: i64, m: i64, d: i64, off: i32) -> (i64, i64) {
        match sun_day(lat, lon, days_from_civil(y, m, d)) {
            SunDay::Normal { rise, set } => (minutes(rise, off), minutes(set, off)),
            other => panic!("expected a normal day, got {other:?}"),
        }
    }

    #[test]
    fn calendar_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 2, 29), 19_782);
        for z in [-800, 0, 19_000, 20_000, 25_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }

    // NOAA reference values (timeanddate / NOAA calculator), local times.
    #[test]
    fn london_solstices() {
        let (rise, set) = sun(51.51, -0.13, 2024, 6, 21, 60);
        near(rise, 4, 43);
        near(set, 21, 21);
        let (rise, set) = sun(51.51, -0.13, 2024, 12, 21, 0);
        near(rise, 8, 4);
        near(set, 15, 53);
    }

    #[test]
    fn new_york_summer() {
        let (rise, set) = sun(40.71, -74.01, 2024, 6, 21, -240);
        near(rise, 5, 25);
        near(set, 20, 31);
    }

    #[test]
    fn singapore_is_steady_all_year() {
        let (lat, lon) = (1.35, 103.82);
        for (m, d) in [(1, 1), (3, 20), (6, 21), (9, 22), (12, 21)] {
            let (rise, set) = sun(lat, lon, 2024, m, d, 480);
            assert!(
                (6 * 60 + 45..=7 * 60 + 20).contains(&rise),
                "{m}-{d} {rise}"
            );
            assert!(
                (18 * 60 + 50..=19 * 60 + 25).contains(&set),
                "{m}-{d} {set}"
            );
            // Twelve hours of daylight, give or take 15 minutes.
            assert!(((set - rise) - 720).abs() <= 15);
        }
    }

    #[test]
    fn singapore_earliest_sunrise() {
        // Published: earliest sunrise of 2026 is about 06:46 on 22 October.
        let (rise, _) = sun(1.35, 103.82, 2026, 10, 22, 480);
        near(rise, 6, 46);
    }

    #[test]
    fn southern_hemisphere_flips_seasons() {
        let (rise, set) = sun(-33.87, 151.21, 2024, 12, 21, 660);
        near(rise, 5, 41);
        near(set, 20, 5);
        let (rise, set) = sun(-33.87, 151.21, 2024, 6, 21, 600);
        near(rise, 7, 0);
        near(set, 16, 54);
    }

    #[test]
    fn polar_day_and_night() {
        let tromso = (69.65, 18.96);
        assert_eq!(
            sun_day(tromso.0, tromso.1, days_from_civil(2024, 6, 21)),
            SunDay::AlwaysUp
        );
        assert_eq!(
            sun_day(tromso.0, tromso.1, days_from_civil(2024, 12, 21)),
            SunDay::AlwaysDown
        );
        let s = Schedule {
            on: true,
            day: "a".into(),
            night: "b".into(),
            city: Some("Tromso".into()),
            ..Schedule::default()
        };
        let midsummer = days_from_civil(2024, 6, 21) * 86_400 + 12 * 3600;
        let n = evaluate(&s, midsummer, &utc).unwrap();
        assert_eq!(n.polar, Some(Phase::Day));
        assert_eq!(n.phase, Phase::Day);
        assert!(n.next.is_some(), "the sun does set again, weeks later");
        let midwinter = days_from_civil(2024, 12, 21) * 86_400 + 12 * 3600;
        let n = evaluate(&s, midwinter, &utc).unwrap();
        assert_eq!((n.polar, n.phase), (Some(Phase::Night), Phase::Night));
        assert_eq!(n.next.unwrap().1, Phase::Day);
        assert!(n.day_starts.is_none());
    }

    fn fixed() -> Schedule {
        Schedule {
            on: true,
            day: "light".into(),
            night: "dark".into(),
            mode: Mode::Fixed,
            ..Schedule::default()
        }
    }

    fn at(y: i64, m: i64, d: i64, h: i64, min: i64, off: i32) -> i64 {
        days_from_civil(y, m, d) * 86_400 + (h * 60 + min) * 60 - i64::from(off) * 60
    }

    #[test]
    fn fixed_times_split_the_day() {
        let off = |_| 480;
        let s = fixed();
        let n = evaluate(&s, at(2026, 10, 15, 12, 0, 480), &off).unwrap();
        assert_eq!(n.phase, Phase::Day);
        assert_eq!(n.since, Some(at(2026, 10, 15, 7, 0, 480)));
        assert_eq!(n.next, Some((at(2026, 10, 15, 19, 0, 480), Phase::Night)));
        let n = evaluate(&s, at(2026, 10, 15, 20, 0, 480), &off).unwrap();
        assert_eq!(n.phase, Phase::Night);
        assert_eq!(n.next, Some((at(2026, 10, 16, 7, 0, 480), Phase::Day)));
        let n = evaluate(&s, at(2026, 10, 15, 3, 0, 480), &off).unwrap();
        assert_eq!(n.phase, Phase::Night);
        assert_eq!(n.since, Some(at(2026, 10, 14, 19, 0, 480)));
        assert_eq!(local_hm(n.next.unwrap().0, &off), "07:00");
    }

    #[test]
    fn night_before_day_wraps_midnight() {
        // A night owl: day from 13:00, night from 02:00.
        let off = |_| 0;
        let s = Schedule {
            day_at: 13 * 60,
            night_at: 2 * 60,
            ..fixed()
        };
        assert_eq!(
            evaluate(&s, at(2026, 1, 5, 1, 0, 0), &off).unwrap().phase,
            Phase::Day
        );
        assert_eq!(
            evaluate(&s, at(2026, 1, 5, 3, 0, 0), &off).unwrap().phase,
            Phase::Night
        );
        assert_eq!(
            evaluate(&s, at(2026, 1, 5, 14, 0, 0), &off).unwrap().phase,
            Phase::Day
        );
        let same = Schedule {
            night_at: 13 * 60,
            ..s
        };
        assert!(evaluate(&same, 0, &off).is_err());
    }

    #[test]
    fn sun_mode_phase_and_next() {
        let off = |_| 480;
        let s = Schedule {
            on: true,
            day: "light".into(),
            night: "dark".into(),
            city: Some("singapore".into()),
            ..Schedule::default()
        };
        let noon = evaluate(&s, at(2026, 10, 15, 12, 0, 480), &off).unwrap();
        assert_eq!(noon.phase, Phase::Day);
        let (when, to) = noon.next.unwrap();
        assert_eq!(to, Phase::Night);
        let hm = local_hm(when, &off);
        assert!(hm.starts_with("18:5") || hm.starts_with("19:0"), "{hm}");
        let late = evaluate(&s, at(2026, 10, 15, 23, 0, 480), &off).unwrap();
        assert_eq!(late.phase, Phase::Night);
        assert_eq!(late.next.unwrap().1, Phase::Day);
        assert_eq!(
            day_word(late.next.unwrap().0, at(2026, 10, 15, 23, 0, 480), &off),
            "tomorrow"
        );
    }

    #[test]
    fn unknown_place_is_an_error() {
        let s = Schedule {
            city: Some("Atlantis".into()),
            ..Schedule::default()
        };
        assert!(evaluate(&s, 0, &utc).is_err());
        assert!(evaluate(&Schedule::default(), 0, &utc).is_err());
        let typed = Schedule {
            lat: Some(51.5),
            lon: Some(-0.1),
            ..Schedule::default()
        };
        assert!(evaluate(&typed, 0, &utc).is_ok());
    }

    #[test]
    fn cities() {
        assert!(CITIES.len() >= 150, "{}", CITIES.len());
        for name in [
            "Singapore",
            "Kuala Lumpur",
            "London",
            "New York",
            "San Francisco",
            "Tokyo",
            "Sydney",
            "Mumbai",
            "Delhi",
            "Bangalore",
            "Berlin",
            "Paris",
        ] {
            assert!(find_city(name).is_some(), "{name}");
        }
        assert_eq!(find_city("  sINGAPORE ").unwrap().name, "Singapore");
        assert!(find_city("Gotham").is_none());
        let hits = search_cities("york");
        assert_eq!(hits[0].name, "New York");
        assert_eq!(search_cities("malaysia")[0].country, "Malaysia");
        assert_eq!(search_cities("lon")[0].name, "London");
        // No duplicates, sane coordinates.
        let mut names: Vec<_> = CITIES.iter().map(|c| fold(c.name)).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), CITIES.len());
        assert!(
            CITIES
                .iter()
                .all(|c| c.lat.abs() <= 90.0 && c.lon.abs() <= 180.0)
        );
    }

    #[test]
    fn time_zones_suggest_cities() {
        assert_eq!(city_for_zone("Asia/Singapore").unwrap().name, "Singapore");
        assert_eq!(
            city_for_zone("Asia/Kuala_Lumpur").unwrap().name,
            "Kuala Lumpur"
        );
        assert_eq!(city_for_zone("America/New_York").unwrap().name, "New York");
        assert_eq!(
            city_for_zone("America/Los_Angeles").unwrap().name,
            "Los Angeles"
        );
        assert_eq!(city_for_zone("Asia/Kolkata").unwrap().name, "Delhi");
        assert_eq!(city_for_zone("Europe/London").unwrap().name, "London");
        assert!(city_for_zone("Etc/UTC").is_none());
    }

    #[test]
    fn times_parse() {
        assert_eq!(parse_hm("07:00").unwrap(), 420);
        assert_eq!(parse_hm("7:05").unwrap(), 425);
        assert_eq!(parse_hm("19").unwrap(), 1140);
        assert!(parse_hm("24:00").is_err());
        assert!(parse_hm("7:60").is_err());
        assert!(parse_hm("noon").is_err());
        assert_eq!(fmt_hm(425), "07:05");
    }

    #[test]
    fn settings_round_trip() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        assert_eq!(
            settings::load(&paths).unwrap().schedule,
            Schedule::default()
        );
        let s = Schedule {
            on: true,
            day: "rose-pine-dawn".into(),
            night: "tokyo-night".into(),
            mode: Mode::Fixed,
            city: Some("Singapore".into()),
            lat: Some(1.5),
            lon: Some(-103.25),
            day_at: 6 * 60 + 30,
            night_at: 19 * 60 + 45,
        };
        let mut all = settings::load(&paths).unwrap();
        all.schedule = s.clone();
        settings::save(&paths, &all).unwrap();
        assert_eq!(settings::load(&paths).unwrap().schedule, s);
        // Other settings survive and the file stays valid TOML.
        let text = fs::read_to_string(paths.root().join("settings.toml")).unwrap();
        assert!(text.contains("[schedule]") && text.contains("day_at = \"06:30\""));
        text.parse::<toml::Table>().unwrap();
    }

    #[test]
    fn bad_toml_values_fall_back() {
        let t: toml::Table = "mode = 'x'\nlat = 200.0\nlon = 10\nday_at = 'nope'\non = 'yes'"
            .parse()
            .unwrap();
        let s = from_toml(Some(&toml::Value::Table(t)));
        assert_eq!(s.mode, Mode::Sun);
        assert_eq!((s.lat, s.lon), (None, Some(10.0)));
        assert_eq!(s.day_at, 420);
        assert!(!s.on);
    }
}
