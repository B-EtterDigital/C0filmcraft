//! Context-local replacement palette; all Color32 fields of Tokens are supported.
use super::Tokens;
use egui::Color32;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Report {
    pub applied: Vec<String>,
    pub unknown: Vec<String>,
    pub invalid: Vec<String>,
}
fn key() -> egui::Id {
    egui::Id::unique("filmcraft-host-colors")
}
pub fn set(ctx: &egui::Context, value: &Value, mut report: Report) -> Result<Report, String> {
    let entries = match value {
        Value::Null => None,
        Value::Object(entries) => Some(entries),
        _ => return Err("themeTokens must be an object or null".into()),
    };
    let mut clean = BTreeMap::new();
    let mut scratch = Tokens::for_kind(super::ThemeKind::Dark);
    for (name, value) in entries.into_iter().flatten() {
        if !assign(&mut scratch, name, Color32::TRANSPARENT) {
            report.unknown.push(format!("themeTokens.{name}"));
        } else if let Some(color) = colour(value) {
            clean.insert(name.clone(), color);
            report.applied.push(format!("themeTokens.{name}"));
        } else {
            report.invalid.push(format!("themeTokens.{name}"));
        }
    }
    ctx.data_mut(|data| data.insert_temp(key(), clean));
    Ok(report)
}
pub fn resolve(ctx: &egui::Context, mut tokens: Tokens) -> Tokens {
    if let Some(entries) = ctx.data(|data| data.get_temp::<BTreeMap<String, Color32>>(key())) {
        for (name, color) in entries {
            assign(&mut tokens, &name, color);
        }
    }
    tokens
}
fn colour(value: &Value) -> Option<Color32> {
    let hex = value.as_str()?.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at| hex.get(at..at + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, if hex.len() == 8 { byte(6)? } else { 255 }))
}
fn assign(tokens: &mut Tokens, key: &str, color: Color32) -> bool {
    macro_rules! colors {($($field:ident),* $(,)?)=>{match key {$(stringify!($field)=>{tokens.$field=color;true},)*_=>false}};}
    colors!(
        app_bg,
        header_bg,
        panel_bg,
        tab_bg,
        tab_text,
        tab_text_active,
        focus,
        accent,
        accent_hover,
        text,
        text_dim,
        text_faint,
        icon,
        icon_active,
        hover,
        pressed,
        field_bg,
        field_border,
        separator,
        row_alt,
        row_selected,
        hot_text,
        pill_active_bg,
        clip_bar_bg,
        control_surface,
        control_well,
        control_border,
        control_handle_bg,
        control_handle,
        control_handle_dim,
        plot_bg,
        plot_grid,
        plot_axis,
        plot_handle,
        plot_handle_dim,
        meter_bg,
        meter_readout_bg,
        meter_clip_off,
        panner_bg,
        panner_border,
        panner_speaker,
        fader_knob,
        eq_track,
        eq_tick,
        eq_knob,
        eq_knob_active,
        eq_node,
        eq_node_off,
        crossover_handle,
        keyframe_plot_bg,
        keyframe_plot_grid,
        keyframe_plot_axis,
        keyframe_handle,
        switch_fill,
        switch_knob,
        switch_border,
        slider_disabled_track,
        slider_disabled_knob,
        button_hover,
        button_border,
        caption_track_bg,
        scroll_thumb,
        scroll_thumb_hover,
        vertical_scroll_thumb,
        disabled_clip_bg,
        tl_bg,
        tl_track_bg,
        tl_track_bg_alt,
        tl_header_bg,
        tl_ruler_bg,
        tl_ruler_tick,
        tl_ruler_text,
        playhead,
        track_target,
        in_out_shade,
        clip_selected_border,
        render_red,
        render_yellow,
        render_green,
        monitor_bg,
        timecode,
        danger
    )
}
