use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use eframe::egui::{self, Color32, Id, Pos2, RichText, Sense, Stroke, Ui, Vec2};

/// Popup calendar + time picker.
///
/// Call `DateTimePicker::new(id, &mut datetime).show(ui)`. The widget renders
/// as a small label/button showing the current value; clicking it opens a
/// popup with a month calendar and digital time spinners. Returns the egui
/// `Response` for the button so callers can detect `.changed()`.
pub struct DateTimePicker<'a> {
    id: Id,
    value: &'a mut chrono::DateTime<Utc>,
}

impl<'a> DateTimePicker<'a> {
    pub fn new(id: impl std::hash::Hash, value: &'a mut chrono::DateTime<Utc>) -> Self {
        Self {
            id: Id::new(id),
            value,
        }
    }

    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let local = self.value.naive_utc();

        let button_text = local.format("%Y-%m-%d  %H:%M:%S").to_string();
        let button_resp = ui.button(RichText::new(button_text).monospace());

        let mut changed = false;
        egui::Popup::from_toggle_button_response(&button_resp)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui: &mut Ui| {
                ui.set_min_width(220.0);
                egui::ScrollArea::vertical()
                    .max_height(500.0)
                    .show(ui, |ui| {
                        changed = show_calendar_and_time(ui, self.id, self.value);
                    });
            });

        if changed {
            // Return a response that signals change; reuse button response id area.
            let mut r = button_resp.clone();
            r.mark_changed();
            r
        } else {
            button_resp
        }
    }
}

/// Draws the calendar grid + time spinners into `ui`, mutates `dt` if the
/// user picks a new value. Returns true when the value changed.
fn show_calendar_and_time(ui: &mut Ui, id: Id, dt: &mut chrono::DateTime<Utc>) -> bool {
    let mut changed = false;
    let local = dt.naive_utc();

    // ── Month/year navigation ────────────────────────────────────────────
    let nav_year_id = id.with("nav_year");
    let nav_month_id = id.with("nav_month");

    // Persist the month/year the calendar is currently browsing separately
    // from the selected value so the user can navigate without changing dt.
    let (mut view_year, mut view_month) = ui.memory_mut(|m| {
        (
            *m.data
                .get_temp_mut_or_insert_with(nav_year_id, || local.year()),
            *m.data
                .get_temp_mut_or_insert_with(nav_month_id, || local.month()),
        )
    });

    ui.horizontal(|ui| {
        if ui.small_button("◀").clicked() {
            if view_month == 1 {
                view_month = 12;
                view_year -= 1;
            } else {
                view_month -= 1;
            }
        }
        ui.label(RichText::new(format!("{} {}", month_name(view_month), view_year)).strong());
        if ui.small_button("▶").clicked() {
            if view_month == 12 {
                view_month = 1;
                view_year += 1;
            } else {
                view_month += 1;
            }
        }
    });

    ui.memory_mut(|m| {
        m.data.insert_temp(nav_year_id, view_year);
        m.data.insert_temp(nav_month_id, view_month);
    });

    // ── Calendar grid ────────────────────────────────────────────────────
    let selected_date = local.date();
    let first_day = NaiveDate::from_ymd_opt(view_year, view_month, 1).unwrap();
    // Weekday of the 1st (Mon=0 .. Sun=6)
    let start_offset = first_day.weekday().num_days_from_monday() as usize;
    let days_in_month = days_in_month(view_year, view_month);

    egui::Grid::new(id.with("cal_grid"))
        .min_col_width(26.0)
        .max_col_width(26.0)
        .spacing(Vec2::new(2.0, 2.0))
        .show(ui, |ui| {
            // Header row
            for h in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                ui.label(RichText::new(h).small().weak());
            }
            ui.end_row();

            let mut col = 0usize;
            // Leading blanks
            for _ in 0..start_offset {
                ui.label("");
                col += 1;
            }
            for day in 1..=days_in_month {
                let this_date = NaiveDate::from_ymd_opt(view_year, view_month, day).unwrap();
                let is_selected = this_date == selected_date;
                let is_today = this_date == Utc::now().naive_utc().date();

                let label = RichText::new(format!("{:2}", day)).monospace();
                let label = if is_today { label.underline() } else { label };

                let btn = egui::Button::new(label)
                    .fill(if is_selected {
                        ui.visuals().selection.bg_fill
                    } else {
                        Color32::TRANSPARENT
                    })
                    .min_size(Vec2::new(26.0, 20.0));

                if ui.add(btn).clicked() {
                    let new_naive = NaiveDateTime::new(this_date, local.time());
                    *dt = Utc.from_utc_datetime(&new_naive);
                    changed = true;
                }

                col += 1;
                if col.is_multiple_of(7) {
                    ui.end_row();
                }
            }
        });

    ui.separator();

    // ── Time spinners ────────────────────────────────────────────────────
    let mut h = local.hour();
    let mut min = local.minute();
    let mut sec = local.second();

    ui.horizontal(|ui| {
        ui.label("Time:");
        changed |= time_spinner(ui, id.with("h"), &mut h, 0, 23, "h");
        ui.label(":");
        changed |= time_spinner(ui, id.with("m"), &mut min, 0, 59, "m");
        ui.label(":");
        changed |= time_spinner(ui, id.with("s"), &mut sec, 0, 59, "s");
    });

    if changed {
        // Reread local in case date was just changed above
        let local = dt.naive_utc();
        let new_time = NaiveTime::from_hms_opt(h, min, sec).unwrap_or(local.time());
        let new_naive = NaiveDateTime::new(local.date(), new_time);
        *dt = Utc.from_utc_datetime(&new_naive);
    }

    ui.separator();
    changed |= analogue_clock(ui, id, &mut h, &mut min, &mut sec);

    // Apply any clock-driven changes
    if changed {
        let local = dt.naive_utc();
        let new_time = NaiveTime::from_hms_opt(h, min, sec).unwrap_or(local.time());
        let new_naive = NaiveDateTime::new(local.date(), new_time);
        *dt = Utc.from_utc_datetime(&new_naive);
    }

    changed
}

/// Analogue clock face with draggable hour, minute, and second hands.
/// Mutates h/m/s when the user drags a hand; returns true if changed.
fn analogue_clock(ui: &mut Ui, id: Id, h: &mut u32, m: &mut u32, s: &mut u32) -> bool {
    const RADIUS: f32 = 80.0;
    const SIZE: f32 = RADIUS * 2.0 + 8.0;
    // Hand lengths as fraction of radius
    const HOUR_LEN: f32 = 0.55;
    const MIN_LEN: f32 = 0.80;
    const SEC_LEN: f32 = 0.90;
    // Hit-test radius around each hand tip
    const HIT_R: f32 = 10.0;

    let (resp, painter) = ui.allocate_painter(Vec2::splat(SIZE), Sense::hover());
    let center = resp.rect.center();

    let vis = ui.visuals();
    let face_color = vis.extreme_bg_color;
    let rim_color = vis.widgets.noninteractive.fg_stroke.color;
    let hand_color = vis.widgets.active.fg_stroke.color;
    let sec_color = Color32::from_rgb(220, 60, 60);

    // Face
    painter.circle(center, RADIUS, face_color, Stroke::new(1.5, rim_color));

    // Hour tick marks
    for tick in 0..12 {
        let angle = tick as f32 * std::f32::consts::TAU / 12.0;
        let (sa, ca) = angle.sin_cos();
        let outer = center + Vec2::new(sa, -ca) * RADIUS;
        let inner = center + Vec2::new(sa, -ca) * (RADIUS * 0.88);
        painter.line_segment([inner, outer], Stroke::new(1.5, rim_color));
    }
    // Minute tick marks (skip hour positions)
    for tick in 0..60 {
        if tick % 5 == 0 {
            continue;
        }
        let angle = tick as f32 * std::f32::consts::TAU / 60.0;
        let (sa, ca) = angle.sin_cos();
        let outer = center + Vec2::new(sa, -ca) * RADIUS;
        let inner = center + Vec2::new(sa, -ca) * (RADIUS * 0.94);
        painter.line_segment([inner, outer], Stroke::new(0.8, rim_color));
    }

    // Hand angles — 12 o'clock is angle 0, clockwise
    let hour_frac = (*h % 12) as f32 / 12.0 + *m as f32 / 720.0;
    let min_frac = *m as f32 / 60.0 + *s as f32 / 3600.0;
    let sec_frac = *s as f32 / 60.0;

    let hand_tip = |frac: f32, len: f32| -> Pos2 {
        let angle = frac * std::f32::consts::TAU;
        let (sa, ca) = angle.sin_cos();
        center + Vec2::new(sa, -ca) * (RADIUS * len)
    };

    let hour_tip = hand_tip(hour_frac, HOUR_LEN);
    let min_tip = hand_tip(min_frac, MIN_LEN);
    let sec_tip = hand_tip(sec_frac, SEC_LEN);

    // Draw hands
    painter.line_segment([center, hour_tip], Stroke::new(4.0, hand_color));
    painter.line_segment([center, min_tip], Stroke::new(2.5, hand_color));
    painter.line_segment([center, sec_tip], Stroke::new(1.5, sec_color));
    // Centre pip
    painter.circle_filled(center, 4.0, hand_color);

    let mut changed = false;

    // Drag handling — one invisible sense rect per hand tip.
    // Priority: second > minute > hour (topmost drawn last so it wins on overlap).
    let drag_hand = |ui: &mut Ui, drag_id: Id, tip: Pos2| -> Option<f32> {
        let hit_rect = egui::Rect::from_center_size(tip, Vec2::splat(HIT_R * 2.0));
        let r = ui.interact(hit_rect, drag_id, Sense::drag());
        if r.dragged() {
            let pointer = r.interact_pointer_pos()?;
            let delta = pointer - center;
            // atan2 with y-flipped so 12 o'clock = 0 increasing clockwise
            let angle = delta.x.atan2(-delta.y);
            let frac = (angle / std::f32::consts::TAU).rem_euclid(1.0);
            Some(frac)
        } else {
            None
        }
    };

    if let Some(frac) = drag_hand(ui, id.with("drag_h"), hour_tip) {
        *h = ((frac * 12.0) as u32 + if *h >= 12 { 12 } else { 0 }).min(23);
        changed = true;
    }
    if let Some(frac) = drag_hand(ui, id.with("drag_m"), min_tip) {
        *m = (frac * 60.0) as u32 % 60;
        changed = true;
    }
    if let Some(frac) = drag_hand(ui, id.with("drag_s"), sec_tip) {
        *s = (frac * 60.0) as u32 % 60;
        changed = true;
    }

    // Show AM/PM toggle when hour hand is dragged near 12
    ui.horizontal(|ui| {
        ui.label(format!("{:02}:{:02}:{:02}", h, m, s));
        let is_pm = *h >= 12;
        if ui.selectable_label(!is_pm, "AM").clicked() && is_pm {
            *h -= 12;
            changed = true;
        }
        if ui.selectable_label(is_pm, "PM").clicked() && !is_pm {
            *h += 12;
            changed = true;
        }
    });

    changed
}

/// A small ▲ value ▼ spinner for a numeric time component.
fn time_spinner(ui: &mut Ui, id: Id, value: &mut u32, min: u32, max: u32, _suffix: &str) -> bool {
    // Editable text buffer stored in egui memory
    let buf_id = id.with("buf");
    let mut buf: String = ui.memory_mut(|m| {
        m.data
            .get_temp_mut_or_insert_with(buf_id, || format!("{:02}", *value))
            .clone()
    });

    // Sync buffer from value when not focused
    let field_id = id.with("field");
    if !ui.memory(|m| m.has_focus(field_id)) {
        buf = format!("{:02}", *value);
        ui.memory_mut(|m| m.data.insert_temp(buf_id, buf.clone()));
    }

    let mut changed = false;
    ui.vertical(|ui| {
        ui.set_max_width(32.0);
        if ui.small_button("▲").clicked() {
            *value = if *value >= max { min } else { *value + 1 };
            changed = true;
        }
        let resp = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .id(field_id)
                .desired_width(28.0)
                .horizontal_align(egui::Align::Center),
        );
        if resp.changed() {
            if let Ok(v) = buf.parse::<u32>() {
                if v <= max {
                    *value = v;
                    changed = true;
                }
            }
            ui.memory_mut(|m| m.data.insert_temp(buf_id, buf.clone()));
        }
        if ui.button("▼").clicked() {
            *value = if *value <= min { max } else { *value - 1 };
            changed = true;
        }
    });

    changed
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
        .num_days() as u32
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "?",
    }
}
