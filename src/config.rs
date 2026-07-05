use nexus::paths::get_addon_dir;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

use crate::json_loader::{load_tracks_from_json, EventTrack};

// The serializable config types live in the nexus-free core crate so their
// tests can run natively; re-export them so `crate::config::X` paths keep
// working throughout the addon.
pub use event_timers_core::config::{
    LabelColumnPosition, NotificationConfig, ReminderConfig, Settings, TextAlignment,
    TimeRulerInterval, ToastPosition, TrackOverride, TrackVisualConfig, TrackedEventId,
    UserConfig,
};

const USER_CONFIG_FILENAME: &str = "user_config.json";

// === Runtime Configuration ===

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub tracks: Vec<EventTrack>,
    pub categories: Vec<String>,
    pub category_visibility: HashMap<String, bool>,
    pub show_main_window: bool,
    pub is_window_locked: bool,
    pub hide_background: bool,
    pub show_time_ruler: bool,
    pub show_scrollbar: bool,
    pub timeline_width: f32,
    pub view_range_seconds: f32,
    pub current_time_position: f32,
    pub show_category_headers: bool,
    pub spacing_same_category: f32,
    pub spacing_between_categories: f32,
    pub category_order: Vec<String>,
    pub global_track_background: [f32; 4],
    pub global_track_padding: f32,
    pub override_all_track_heights: bool,
    pub global_track_height: f32,
    pub draw_event_borders: bool,
    pub event_border_color: [f32; 4],
    pub event_border_thickness: f32,
    pub category_header_alignment: TextAlignment,
    pub category_header_padding: f32,
    pub label_column_position: LabelColumnPosition,
    pub label_column_width: f32,
    pub label_column_show_category: bool,
    pub label_column_show_track: bool,
    pub label_column_text_size: f32,
    pub label_column_bg_color: [f32; 4],
    pub label_column_text_color: [f32; 4],
    pub label_column_category_color: [f32; 4],
    pub close_on_escape: bool,
    pub copy_with_event_name: bool,
    pub show_quick_access_icon: bool,
    pub setup_onboarding_seen: bool,

    // === Time Ruler Settings ===
    pub time_ruler_interval: TimeRulerInterval,
    pub time_ruler_show_current_time: bool,

    // === Notification Settings ===
    pub tracked_events: HashSet<TrackedEventId>,
    pub favorite_events: HashSet<TrackedEventId>,
    pub oneshot_events: HashSet<TrackedEventId>,
    pub notification_config: NotificationConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let (tracks, categories) = load_tracks_from_json();
        Self {
            tracks,
            categories,
            category_visibility: HashMap::new(),
            show_main_window: false,
            is_window_locked: false,
            hide_background: false,
            show_time_ruler: false,
            show_scrollbar: true,
            timeline_width: 800.0,
            view_range_seconds: 3600.0,
            current_time_position: 0.5,
            show_category_headers: false,
            spacing_same_category: 0.0,
            spacing_between_categories: 0.0,
            category_order: Vec::new(),
            global_track_background: [0.2, 0.2, 0.2, 0.2],
            global_track_padding: 0.0,
            override_all_track_heights: false,
            global_track_height: 40.0,
            draw_event_borders: true,
            event_border_color: [0.0, 0.0, 0.0, 1.0],
            event_border_thickness: 1.0,
            category_header_alignment: TextAlignment::Center,
            category_header_padding: 0.0,
            label_column_position: LabelColumnPosition::None,
            label_column_width: 150.0,
            label_column_show_category: false,
            label_column_show_track: true,
            label_column_text_size: 1.0,
            label_column_bg_color: [0.0, 0.0, 0.0, 0.0],
            label_column_text_color: [1.0, 1.0, 1.0, 1.0],
            label_column_category_color: [0.8, 0.8, 0.2, 1.0],
            close_on_escape: true,
            copy_with_event_name: false,
            show_quick_access_icon: true,
            setup_onboarding_seen: false,
            time_ruler_interval: TimeRulerInterval::default(),
            time_ruler_show_current_time: false,
            tracked_events: HashSet::new(),
            favorite_events: HashSet::new(),
            oneshot_events: HashSet::new(),
            notification_config: NotificationConfig::default(),
        }
    }
}

// === Global State ===

pub static RUNTIME_CONFIG: Lazy<Mutex<RuntimeConfig>> =
    Lazy::new(|| Mutex::new(RuntimeConfig::default()));
pub static USER_CONFIG: Lazy<Mutex<UserConfig>> = Lazy::new(|| Mutex::new(UserConfig::default()));
pub static SELECTED_TRACK: Lazy<Mutex<Option<usize>>> = Lazy::new(|| Mutex::new(None));
pub static SELECTED_EVENT: Lazy<Mutex<Option<usize>>> = Lazy::new(|| Mutex::new(None));

// === Configuration Management ===

pub fn apply_user_overrides() {
    // Load fresh tracks from JSON (outside locks)
    let (default_tracks, categories) = load_tracks_from_json();

    // Scope 1: Clean up user config
    let (cleaned_custom_tracks, track_overrides, settings) = {
        let mut user_cfg = USER_CONFIG.lock();

        // Deduplicate custom tracks by name (keep first occurrence)
        let mut seen_custom_track_names: HashSet<String> = HashSet::new();
        user_cfg.custom_tracks.retain(|track| {
            if seen_custom_track_names.contains(&track.name) {
                false // Remove duplicate
            } else {
                seen_custom_track_names.insert(track.name.clone());
                true // Keep first occurrence
            }
        });

        // Remove custom tracks that now exist in default JSON
        let default_track_names: HashSet<String> =
            default_tracks.iter().map(|t| t.name.clone()).collect();

        user_cfg
            .custom_tracks
            .retain(|track| !default_track_names.contains(&track.name));

        // Clone data we need (releases lock early)
        (
            user_cfg.custom_tracks.clone(),
            user_cfg.track_overrides.clone(),
            user_cfg.settings.clone(),
        )
    }; // user_cfg lock dropped here

    // Scope 2: Update runtime config
    {
        let mut runtime = RUNTIME_CONFIG.lock();

        // Set runtime tracks to defaults
        runtime.tracks = default_tracks;
        runtime.categories = categories;

        // Apply user overrides to default tracks
        for track in &mut runtime.tracks {
            if let Some(override_data) = track_overrides.get(&track.name) {
                if let Some(visible) = override_data.visible {
                    track.visible = visible;
                }
                if let Some(height) = override_data.height {
                    track.height = height;
                }

                for event in &mut track.events {
                    if override_data.disabled_events.contains(&event.name) {
                        event.enabled = false;
                    }
                }
            }
        }

        // Add cleaned custom tracks
        runtime.tracks.extend(cleaned_custom_tracks);

        // Apply all user settings
        runtime.category_visibility = settings.category_visibility;
        runtime.show_main_window = settings.show_main_window;
        runtime.is_window_locked = settings.is_window_locked;
        runtime.hide_background = settings.hide_background;
        runtime.show_time_ruler = settings.show_time_ruler;
        runtime.show_scrollbar = settings.show_scrollbar;
        runtime.timeline_width = settings.timeline_width;
        runtime.view_range_seconds = settings.view_range_seconds;
        runtime.current_time_position = settings.current_time_position;
        runtime.show_category_headers = settings.show_category_headers;
        runtime.spacing_same_category = settings.spacing_same_category;
        runtime.spacing_between_categories = settings.spacing_between_categories;
        runtime.category_order = settings.category_order;
        runtime.global_track_background = settings.global_track_background;
        runtime.global_track_padding = settings.global_track_padding;
        runtime.override_all_track_heights = settings.override_all_track_heights;
        runtime.global_track_height = settings.global_track_height;
        runtime.draw_event_borders = settings.draw_event_borders;
        runtime.event_border_color = settings.event_border_color;
        runtime.event_border_thickness = settings.event_border_thickness;
        runtime.category_header_alignment = settings.category_header_alignment;
        runtime.category_header_padding = settings.category_header_padding;
        runtime.label_column_position = settings.label_column_position;
        runtime.label_column_width = settings.label_column_width;
        runtime.label_column_show_category = settings.label_column_show_category;
        runtime.label_column_show_track = settings.label_column_show_track;
        runtime.label_column_text_size = settings.label_column_text_size;
        runtime.label_column_bg_color = settings.label_column_bg_color;
        runtime.label_column_text_color = settings.label_column_text_color;
        runtime.label_column_category_color = settings.label_column_category_color;
        runtime.close_on_escape = settings.close_on_escape;
        runtime.copy_with_event_name = settings.copy_with_event_name;
        runtime.show_quick_access_icon = settings.show_quick_access_icon;
        runtime.time_ruler_interval = settings.time_ruler_interval;
        runtime.time_ruler_show_current_time = settings.time_ruler_show_current_time;
        runtime.tracked_events = settings.tracked_events;
        runtime.favorite_events = settings.favorite_events;
        runtime.oneshot_events = settings.oneshot_events;
        runtime.notification_config = settings.notification_config;
        runtime.setup_onboarding_seen = settings.setup_onboarding_seen;
    } // runtime lock dropped here
}

pub fn extract_user_overrides() {
    let runtime = RUNTIME_CONFIG.lock();
    let mut user_cfg = USER_CONFIG.lock();

    user_cfg.track_overrides.clear();
    user_cfg.custom_tracks.clear();

    let (default_tracks, _) = load_tracks_from_json();
    let default_map: HashMap<String, &EventTrack> =
        default_tracks.iter().map(|t| (t.name.clone(), t)).collect();

    for track in &runtime.tracks {
        if let Some(default_track) = default_map.get(&track.name) {
            let mut override_data = TrackOverride::default();
            let mut has_changes = false;

            if track.visible != default_track.visible {
                override_data.visible = Some(track.visible);
                has_changes = true;
            }

            if (track.height - default_track.height).abs() > 0.1 {
                override_data.height = Some(track.height);
                has_changes = true;
            }

            for event in &track.events {
                if !event.enabled {
                    override_data.disabled_events.push(event.name.clone());
                    has_changes = true;
                }
            }

            if has_changes {
                user_cfg
                    .track_overrides
                    .insert(track.name.clone(), override_data);
            }
        } else {
            user_cfg.custom_tracks.push(track.clone());
        }
    }

    // Exhaustive struct construction: the compiler guarantees no setting is
    // forgotten here. Collapses to a single clone once RuntimeConfig embeds
    // Settings directly.
    user_cfg.settings = Settings {
        category_visibility: runtime.category_visibility.clone(),
        show_main_window: runtime.show_main_window,
        is_window_locked: runtime.is_window_locked,
        hide_background: runtime.hide_background,
        show_time_ruler: runtime.show_time_ruler,
        show_scrollbar: runtime.show_scrollbar,
        timeline_width: runtime.timeline_width,
        view_range_seconds: runtime.view_range_seconds,
        current_time_position: runtime.current_time_position,
        show_category_headers: runtime.show_category_headers,
        spacing_same_category: runtime.spacing_same_category,
        spacing_between_categories: runtime.spacing_between_categories,
        category_order: runtime.category_order.clone(),
        global_track_background: runtime.global_track_background,
        global_track_padding: runtime.global_track_padding,
        override_all_track_heights: runtime.override_all_track_heights,
        global_track_height: runtime.global_track_height,
        draw_event_borders: runtime.draw_event_borders,
        event_border_color: runtime.event_border_color,
        event_border_thickness: runtime.event_border_thickness,
        category_header_alignment: runtime.category_header_alignment,
        category_header_padding: runtime.category_header_padding,
        label_column_position: runtime.label_column_position,
        label_column_width: runtime.label_column_width,
        label_column_show_category: runtime.label_column_show_category,
        label_column_show_track: runtime.label_column_show_track,
        label_column_text_size: runtime.label_column_text_size,
        label_column_bg_color: runtime.label_column_bg_color,
        label_column_text_color: runtime.label_column_text_color,
        label_column_category_color: runtime.label_column_category_color,
        close_on_escape: runtime.close_on_escape,
        copy_with_event_name: runtime.copy_with_event_name,
        show_quick_access_icon: runtime.show_quick_access_icon,
        setup_onboarding_seen: runtime.setup_onboarding_seen,
        time_ruler_interval: runtime.time_ruler_interval,
        time_ruler_show_current_time: runtime.time_ruler_show_current_time,
        tracked_events: runtime.tracked_events.clone(),
        favorite_events: runtime.favorite_events.clone(),
        oneshot_events: runtime.oneshot_events.clone(),
        notification_config: runtime.notification_config.clone(),
    };
}

// === File I/O ===

pub fn get_user_config_path() -> Option<PathBuf> {
    get_addon_dir("event_timers").map(|p| p.join(USER_CONFIG_FILENAME))
}

pub fn load_user_config() {
    if let Some(path) = get_user_config_path() {
        if path.exists() {
            if let Ok(json_str) = fs::read_to_string(&path) {
                if let Ok(loaded) = serde_json::from_str::<UserConfig>(&json_str) {
                    *USER_CONFIG.lock() = loaded;
                    apply_user_overrides();
                    return;
                }
            }
        }
    }

    apply_user_overrides();
}

pub fn save_user_config() {
    extract_user_overrides();

    let user_cfg = USER_CONFIG.lock();
    if let Some(path) = get_user_config_path() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ok();
        }
        if let Ok(json_str) = serde_json::to_string_pretty(&*user_cfg) {
            fs::write(&path, json_str).ok();
        }
    }
}

pub fn get_track_visual_config(
    track_name: &str,
    global_bg: [f32; 4],
    global_padding: f32,
) -> TrackVisualConfig {
    let user_override = {
        let user_cfg = USER_CONFIG.lock();
        user_cfg
            .track_overrides
            .get(track_name)
            .and_then(|o| o.visual.clone())
    };

    if let Some(visual) = user_override {
        return visual;
    }

    TrackVisualConfig {
        background_color: global_bg,
        padding: global_padding,
    }
}
