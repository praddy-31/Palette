//! Settings → Schedule: day and night themes, and the minute ticker.

use super::*;

#[derive(Serialize)]
pub(crate) struct ScheduleView {
    pub(crate) on: bool,
    pub(crate) day: String,
    pub(crate) night: String,
    /// "sun" or "fixed".
    pub(crate) mode: String,
    pub(crate) city: Option<String>,
    pub(crate) lat: Option<f64>,
    pub(crate) lon: Option<f64>,
    pub(crate) day_at: String,
    pub(crate) night_at: String,
    /// "Singapore, Singapore", or typed coordinates.
    pub(crate) place: Option<String>,
    /// City suggested by the Mac's time zone.
    pub(crate) suggestion: Option<String>,
    /// "Sunrise 06:48, sunset 18:53" / "Day from 07:00, night from 19:00".
    pub(crate) today: Option<String>,
    pub(crate) phase: Option<String>,
    /// "18:53 today, to Tokyo Night".
    pub(crate) next: Option<String>,
    pub(crate) problem: Option<String>,
}

pub(crate) fn schedule_view(paths: &Paths) -> Result<ScheduleView, AppError> {
    use palette::schedule as sch;
    let st = sch::which_now(paths).map_err(err)?;
    let s = &st.schedule;
    let sun = s.mode == sch::Mode::Sun;
    let today = st
        .now
        .as_ref()
        .and_then(|n| match (n.polar, n.day_starts, n.night_starts) {
            (Some(sch::Phase::Day), ..) => Some("Polar day: the sun doesn't set today".to_string()),
            (Some(sch::Phase::Night), ..) => {
                Some("Polar night: the sun doesn't rise today".to_string())
            }
            (None, Some(d), Some(nt)) => Some(if sun {
                format!("Sunrise {}, sunset {}", st.hm(d), st.hm(nt))
            } else {
                format!("Day from {}, night from {}", st.hm(d), st.hm(nt))
            }),
            _ => None,
        });
    let next = st
        .now
        .as_ref()
        .and_then(|n| n.next)
        .map(|(t, to)| format!("{} {}, to {}", st.hm(t), st.day_word(t), s.theme_for(to)));
    Ok(ScheduleView {
        on: s.on,
        day: s.day.clone(),
        night: s.night.clone(),
        mode: s.mode.as_str().into(),
        city: s.city.clone(),
        lat: s.lat,
        lon: s.lon,
        day_at: sch::fmt_hm(s.day_at),
        night_at: sch::fmt_hm(s.night_at),
        place: st.place.clone(),
        suggestion: sch::suggest_city().map(|c| c.name.to_string()),
        today,
        phase: st.now.as_ref().map(|n| n.phase.as_str().to_string()),
        next,
        problem: st.problem.clone(),
    })
}

/// The saved schedule with today's times and the next change.
#[tauri::command]
pub(crate) fn schedule_get() -> Result<ScheduleView, AppError> {
    schedule_view(&paths()?)
}

/// What the Settings pane sends back: the whole schedule.
#[derive(serde::Deserialize)]
pub(crate) struct ScheduleInput {
    pub(crate) on: bool,
    pub(crate) day: String,
    pub(crate) night: String,
    pub(crate) mode: String,
    pub(crate) city: Option<String>,
    pub(crate) lat: Option<f64>,
    pub(crate) lon: Option<f64>,
    pub(crate) day_at: String,
    pub(crate) night_at: String,
}

/// Save the schedule (`palette schedule on/off`). If it's on, switches at
/// once when the other theme is due, through the same path as `palette set`.
#[tauri::command]
pub(crate) async fn schedule_save(
    app: AppHandle,
    input: ScheduleInput,
) -> Result<ScheduleView, AppError> {
    use palette::schedule as sch;
    let (view, switched) = tauri::async_runtime::spawn_blocking(move || {
        let paths = paths()?;
        let new = sch::Schedule {
            on: input.on,
            day: input.day,
            night: input.night,
            mode: if input.mode == "fixed" {
                sch::Mode::Fixed
            } else {
                sch::Mode::Sun
            },
            city: input.city.filter(|c| !c.is_empty()),
            lat: input.lat,
            lon: input.lon,
            day_at: sch::parse_hm(&input.day_at).map_err(err)?,
            night_at: sch::parse_hm(&input.night_at).map_err(err)?,
        };
        let result = sch::save(&paths, new).map_err(err)?;
        Ok::<_, AppError>((schedule_view(&paths)?, schedule_applied(result)))
    })
    .await
    .map_err(err)??;
    if let Some(a) = switched {
        let _ = app.emit("theme-changed", a);
    }
    Ok(view)
}

pub(crate) fn schedule_applied(result: palette::schedule::Applied) -> Option<Applied> {
    match result {
        palette::schedule::Applied::Switched { switch, .. } => {
            Some(applied(switch.theme, &switch.effects))
        }
        _ => None,
    }
}

#[derive(Serialize)]
pub(crate) struct CityRow {
    pub(crate) name: &'static str,
    pub(crate) country: &'static str,
}

/// Built-in cities matching a search (name or country); empty: all of them.
#[tauri::command]
pub(crate) fn schedule_cities(query: String) -> Vec<CityRow> {
    palette::schedule::search_cities(&query)
        .into_iter()
        .map(|c| CityRow {
            name: c.name,
            country: c.country,
        })
        .collect()
}

/// Time until one second past the next whole minute of the clock.
pub(crate) fn until_next_minute() -> Duration {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Duration::from_secs(60 - secs % 60 + 1)
}

/// One check of the schedule (`palette schedule run`): the app does this at
/// launch and every minute.
pub(crate) fn schedule_tick(app: &AppHandle) {
    let Ok(paths) = paths() else { return };
    match palette::schedule::apply_now(&paths) {
        Ok(result) => {
            if let Some(a) = schedule_applied(result) {
                let _ = app.emit("theme-changed", a);
            }
        }
        Err(e) => eprintln!("palette: schedule: {e:#}"),
    }
}
