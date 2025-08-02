use once_cell::sync::Lazy;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use rusttype::{Font, Scale, point};
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use toml;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    protocol::{
        wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_keyboard::WlKeyboard,
        wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm::WlShm,
        wl_surface::WlSurface,
    },
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

#[derive(Deserialize)]
struct AppConfig {
    width: u32,
    max_height: u32,
    scroll_speed: f32,
    border_radius: f32,
    auto_scroll: bool,
}

#[derive(Deserialize)]
struct FontConfig {
    path: String,
    size: f32,
}

#[derive(Deserialize)]
struct ColorsConfig {
    blue: u8,
    green: u8,
    red: u8,
    alpha_multiplier: f32,
}

#[derive(Deserialize)]
struct KeybindingsConfig {
    show_hide: String,
    type_text: String,
    take_screenshot: String,
    record_audio: String,
    solve: String,
    clear: String,
    switch_to_workspace: String,
}

#[derive(Deserialize)]
struct Config {
    app: AppConfig,
    font: FontConfig,
    colors: ColorsConfig,
    keybindings: KeybindingsConfig,
}

static CONFIG: Lazy<Config> = Lazy::new(|| {
    use std::env;
    use std::path::PathBuf;

    let mut config_paths = Vec::new();

    // Add paths from environment variables in order of priority
    if let Ok(xdg_config_home) = env::var("XDG_CONFIG_HOME") {
        if !xdg_config_home.is_empty() {
            let base = PathBuf::from(xdg_config_home);
            config_paths.push(base.join("aerogel/aerogel.toml"));
            config_paths.push(base.join("aerogel.toml"));
        }
    }
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            let base = PathBuf::from(home);
            config_paths.push(base.join(".config/aerogel/aerogel.toml"));
            config_paths.push(base.join(".aerogel.toml"));
        }
    }

    // Add system-wide and relative fallback paths
    config_paths.extend([
        PathBuf::from("/etc/aerogel/aerogel.toml"),
        PathBuf::from("../../aerogel.toml"),
        PathBuf::from("aerogel.toml"),
    ]);

    let config_str = config_paths
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .expect("Failed to read aerogel.toml from any of the expected locations");

    toml::from_str(&config_str).expect("Failed to parse aerogel.toml")
});

#[derive(Debug, Clone)]
struct DragState {
    is_dragging: bool,
    start_x: f64,
    start_y: f64,
    _start_margin_x: i32,
    _start_margin_y: i32,
}

impl Default for DragState {
    fn default() -> Self {
        Self {
            is_dragging: false,
            start_x: 0.0,
            start_y: 0.0,
            _start_margin_x: 20,
            _start_margin_y: 20,
        }
    }
}

fn get_default_text() -> String {
    format!(
        r#"# Keybindings
```
Show / Hide          {}
Type Text            {}
Take Screenshot      {}
Record Audio         {}
Solve                {}
Clear                {}
Switch Workspace     {}+n
```"#,
        &CONFIG.keybindings.show_hide,
        &CONFIG.keybindings.type_text,
        &CONFIG.keybindings.take_screenshot,
        &CONFIG.keybindings.record_audio,
        &CONFIG.keybindings.solve,
        &CONFIG.keybindings.clear,
        &CONFIG.keybindings.switch_to_workspace,
    )
}

struct AppState {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    output: Option<WlOutput>,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,

    // Surface and buffer management
    surface: Option<WlSurface>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    current_buffer: Option<WlBuffer>,

    // Overlay dimensions and position
    width: u32,
    height: u32,
    margin_x: i32,
    margin_y: i32,

    // Overlay state
    visible: bool,

    // Border radius and styling
    border_radius: f32,

    // Drag state
    drag_state: DragState,

    // Current pointer position (global coordinates)
    pointer_x: f64,
    pointer_y: f64,

    // Output dimensions for boundary checking
    output_width: i32,
    output_height: i32,

    // Font for text rendering
    font: Font<'static>,

    // Text to display
    text: String,

    // Text update tracking
    last_text_update: Instant,
    text_update_interval: Duration,
    current_workspace: u32,

    // Track if text has changed (for redrawing)
    text_changed: bool,

    // Scrolling
    scroll_offset_y: f32,
    max_scroll_offset_y: f32,

    // Syntect for syntax highlighting
    syntax_set: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

impl AppState {
    fn new() -> Self {
        // Load margins from log file
        let (loaded_margin_x, loaded_margin_y) = Self::load_margins_from_log().unwrap_or((20, 20));

        // Load font from config path, with fallbacks
        let font = {
            // Try to load from the path specified in the config file first.
            let config_font_result = std::fs::read(&CONFIG.font.path)
                .ok()
                .and_then(|data| Font::try_from_vec(data));

            config_font_result.unwrap_or_else(|| {
                // If the above fails, print a warning and try system fallbacks.
                eprintln!(
                    "Warning: Could not load font from config path: '{}'. Trying system fonts.",
                    &CONFIG.font.path
                );
                let system_font_data =
                    std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
                        .or_else(|_| std::fs::read("/System/Library/Fonts/Arial.ttf"))
                        .or_else(|_| std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf"))
                        .expect("No system fallback font found. Please check font paths.");

                Font::try_from_vec(system_font_data).expect("Failed to parse system fallback font.")
            })
        };

        // Load initial workspace
        let initial_workspace = Self::read_current_workspace().unwrap_or(1);
        let temp_file = format!(".tmp{}", initial_workspace);

        // Load initial text from log file for the correct workspace
        let initial_text = Self::load_text_from_log(&temp_file).unwrap_or_else(get_default_text);

        // Initialize syntect
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["base16-ocean.dark"].clone();

        let width = CONFIG.app.width;
        let calculated_height = calculate_text_height(&font, &initial_text, &ss, &theme, width);
        let height = calculated_height.min(CONFIG.app.max_height);

        let mut new_state = Self {
            compositor: None,
            shm: None,
            layer_shell: None,
            output: None,
            seat: None,
            pointer: None,
            surface: None,
            layer_surface: None,
            current_buffer: None,
            width,
            height,
            margin_x: loaded_margin_x,
            margin_y: loaded_margin_y,
            visible: true,
            border_radius: CONFIG.app.border_radius,
            drag_state: DragState::default(),
            pointer_x: 0.0,
            pointer_y: 0.0,
            output_width: 1920,
            output_height: 1080,
            font,
            text: initial_text,
            last_text_update: Instant::now(),
            text_update_interval: Duration::from_millis(10),
            current_workspace: initial_workspace,
            text_changed: false,
            scroll_offset_y: 0.0,
            max_scroll_offset_y: 0.0,
            syntax_set: ss,
            theme,
        };

        // Calculate initial max_scroll_offset_y
        let total_text_height = calculate_text_height(
            &new_state.font,
            &new_state.text,
            &new_state.syntax_set,
            &new_state.theme,
            new_state.width,
        );
        new_state.max_scroll_offset_y =
            (total_text_height as f32 - new_state.height as f32).max(0.0);

        new_state
    }

    fn read_current_workspace() -> Option<u32> {
        std::fs::read_to_string(".aerogel_workspace")
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    fn check_for_workspace_switch(&mut self, qh: &QueueHandle<Self>) {
        if let Some(new_workspace) = Self::read_current_workspace() {
            if new_workspace != self.current_workspace && new_workspace > 0 {
                println!("[overlay] Switching to workspace {}", new_workspace);
                self.current_workspace = new_workspace;
                self.scroll_offset_y = 0.0;
                self.last_text_update = Instant::now() - (self.text_update_interval * 2);
                self.force_redraw(qh);
            }
        }
    }

    fn load_margins_from_log() -> Option<(i32, i32)> {
        let file = File::open("margin.log").ok()?;
        let reader = BufReader::new(file);

        let mut last_margin_x = None;
        let mut last_margin_y = None;
        let mut last_line = String::new();

        // Read all lines and extract the most recent margin values
        for line in reader.lines() {
            if let Ok(line) = line {
                // Parse lines in format: "Dragging: x=value, y=value"
                if line.contains("Dragging:") {
                    // Find x= and y= values
                    if let Some(x_start) = line.find("x=") {
                        if let Some(x_end) = line[x_start + 2..].find(',') {
                            let x_str = &line[x_start + 2..x_start + 2 + x_end];
                            if let Ok(x_val) = x_str.trim().parse::<i32>() {
                                last_margin_x = Some(x_val);
                            }
                        }
                    }

                    if let Some(y_start) = line.find("y=") {
                        let y_str = &line[y_start + 2..];
                        // Remove any trailing whitespace or characters
                        let y_clean = y_str
                            .trim()
                            .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '-');
                        if let Ok(y_val) = y_clean.parse::<i32>() {
                            last_margin_y = Some(y_val);
                        }
                    }

                    // Keep track of the last valid line
                    last_line = line;
                }
            }
        }

        // Clear the log file and write back only the last line
        if let Ok(mut file) = File::create("margin.log") {
            if !last_line.is_empty() {
                let _ = writeln!(file, "{}", last_line);
            }
        }

        // Return the last found values, or None if not found
        match (last_margin_x, last_margin_y) {
            (Some(x), Some(y)) => {
                println!("Loaded margins from log: x={}, y={}", x, y);
                Some((x, y))
            }
            _ => {
                println!("No valid margins found in log file, using defaults");
                None
            }
        }
    }

    fn load_text_from_log(filename: &str) -> Option<String> {
        match std::fs::read_to_string(filename) {
            Ok(content) => {
                if content.trim().is_empty() {
                    None
                } else {
                    Some(content)
                }
            }
            Err(_) => None,
        }
    }

    fn update_text_from_log(&mut self) -> bool {
        let now = Instant::now();

        if now.duration_since(self.last_text_update) >= self.text_update_interval {
            self.last_text_update = now;

            let temp_file = format!(".tmp{}", self.current_workspace);
            let new_text = Self::load_text_from_log(&temp_file).unwrap_or_else(get_default_text);

            if new_text != self.text {
                self.text = new_text;
                self.text_changed = true;

                // Recalculate height
                let calculated_height = calculate_text_height(
                    &self.font,
                    &self.text,
                    &self.syntax_set,
                    &self.theme,
                    self.width,
                );
                self.height = calculated_height.min(CONFIG.app.max_height);

                // Calculate max_scroll_offset_y
                let total_text_height = calculate_text_height(
                    &self.font,
                    &self.text,
                    &self.syntax_set,
                    &self.theme,
                    self.width,
                );
                self.max_scroll_offset_y = (total_text_height as f32 - self.height as f32).max(0.0);

                if CONFIG.app.auto_scroll {
                    self.scroll_offset_y = self.max_scroll_offset_y;
                } else {
                    self.scroll_offset_y = self.scroll_offset_y.min(self.max_scroll_offset_y);
                }

                if let Some(layer_surface) = &self.layer_surface {
                    layer_surface.set_size(self.width, self.height);
                    if let Some(surface) = &self.surface {
                        surface.commit();
                    }
                }

                return true;
            }
        }

        false
    }

    fn create_overlay(&mut self, qh: &QueueHandle<AppState>) {
        if let (Some(compositor), Some(layer_shell)) = (&self.compositor, &self.layer_shell) {
            let surface = compositor.create_surface(qh, ());

            let layer_surface = layer_shell.get_layer_surface(
                &surface,
                self.output.as_ref(),
                Layer::Overlay,
                "draggable-overlay".to_string(),
                qh,
                (),
            );

            layer_surface.set_size(self.width, self.height);
            layer_surface.set_anchor(Anchor::Top | Anchor::Left);
            layer_surface.set_margin(self.margin_y, 0, 0, self.margin_x);
            layer_surface.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);

            surface.commit();

            self.surface = Some(surface);
            self.layer_surface = Some(layer_surface);
        }
    }

    fn draw_overlay(&mut self, qh: &QueueHandle<AppState>) {
        if let (Some(surface), Some(shm)) = (&self.surface, &self.shm) {
            if !self.visible {
                return;
            }

            // Destroy the old buffer if it exists
            if let Some(old_buffer) = self.current_buffer.take() {
                old_buffer.destroy();
            }

            if let Ok(buffer) = create_shm_buffer(
                shm,
                self.width,
                self.height,
                self.border_radius,
                &self.font,
                &self.text,
                &self.syntax_set,
                &self.theme,
                self.scroll_offset_y,
                self.current_workspace,
                qh,
            ) {
                self.current_buffer = Some(buffer.clone());
                surface.attach(Some(&buffer), 0, 0);
                surface.damage(0, 0, self.width as i32, self.height as i32);
                surface.commit();
            }
        }

        // Reset text changed flag after drawing
        self.text_changed = false;
    }

    fn toggle_visibility(&mut self, qh: &QueueHandle<AppState>) {
        self.visible = !self.visible;
        if self.visible {
            self.draw_overlay(qh);
        } else if let Some(surface) = &self.surface {
            surface.attach(None, 0, 0);
            surface.commit();
        }
    }

    fn start_drag(&mut self, surface_x: f64, surface_y: f64) {
        self.drag_state.is_dragging = true;
        // Store the initial pointer position to calculate deltas from.
        self.drag_state.start_x = surface_x;
        self.drag_state.start_y = surface_y;
    }

    fn update_drag(&mut self, surface_x: f64, surface_y: f64) {
        if !self.drag_state.is_dragging {
            return;
        }

        // Calculate pointer movement since the last motion event.
        let dx = surface_x - self.drag_state.start_x;
        let dy = surface_y - self.drag_state.start_y;

        // Apply the delta to our current margins.
        let new_margin_x = self.margin_x + dx as i32;
        let new_margin_y = self.margin_y + dy as i32;

        // Clamp to screen boundaries.
        let max_x = self.output_width.saturating_sub(self.width as i32).max(0);
        let max_y = self.output_height.saturating_sub(self.height as i32).max(0);
        self.margin_x = new_margin_x.max(0).min(max_x);
        self.margin_y = new_margin_y.max(0).min(max_y);

        // Update the reference point for the next motion event.
        self.drag_state.start_x = surface_x;
        self.drag_state.start_y = surface_y;

        // Log position
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("margin.log")
        {
            writeln!(file, "Dragging: x={}, y={}", self.margin_x, self.margin_y).ok();
        }

        // Update the layer surface with the new position.
        if let Some(layer_surface) = &self.layer_surface {
            layer_surface.set_margin(self.margin_y, 0, 0, self.margin_x);
            if let Some(surface) = &self.surface {
                surface.commit();
            }
        }
    }

    fn stop_drag(&mut self) {
        if self.drag_state.is_dragging {
            self.drag_state.is_dragging = false;
        }
    }

    fn force_redraw(&mut self, qh: &QueueHandle<AppState>) {
        self.text_changed = true;
        self.draw_overlay(qh);
    }
}

// Global registry handling
impl Dispatch<wayland_client::protocol::wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wayland_client::protocol::wl_registry::WlRegistry,
        event: wayland_client::protocol::wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_registry::Event;

        match event {
            Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version, qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version, qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version, qh, ()));
                }
                "wl_output" => {
                    if state.output.is_none() {
                        state.output = Some(registry.bind(name, version, qh, ()));
                    }
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind(name, version, qh, ()));
                }
                _ => {}
            },
            Event::GlobalRemove { .. } => {}
            _ => {}
        }
    }
}

// Layer surface event handling
impl Dispatch<ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event;

        match event {
            Event::Configure {
                serial,
                width,
                height,
            } => {
                if width > 0 && height > 0 {
                    state.width = width;
                    state.height = height;
                }

                layer_surface.ack_configure(serial);
                state.draw_overlay(qh);
            }
            Event::Closed => {
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

// Pointer handling for dragging
impl Dispatch<WlPointer, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wayland_client::protocol::wl_pointer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_pointer::Axis;
        use wayland_client::protocol::wl_pointer::Event;

        match event {
            Event::Enter {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
            }
            Event::Leave { .. } => {

                // Continue dragging even when pointer leaves surface
            }
            Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;

                if state.drag_state.is_dragging {
                    state.update_drag(surface_x, surface_y);
                }
            }
            Event::Button {
                button,
                state: button_state,
                ..
            } => {
                use wayland_client::protocol::wl_pointer::ButtonState;

                match button_state {
                    WEnum::Value(ButtonState::Pressed) => {
                        if button == 0x110 {
                            // Left mouse button

                            state.start_drag(state.pointer_x, state.pointer_y);
                        }
                    }
                    WEnum::Value(ButtonState::Released) => {
                        if button == 0x110 {
                            // Left mouse button

                            state.stop_drag();
                        }
                    }
                    _ => {}
                }
            }
            Event::Axis { axis, value, .. } => {
                match axis {
                    WEnum::Value(Axis::VerticalScroll) => {
                        let scroll_amount = value as f32 / CONFIG.app.scroll_speed; // Adjust scroll speed
                        state.scroll_offset_y = (state.scroll_offset_y + scroll_amount)
                            .max(0.0)
                            .min(state.max_scroll_offset_y);
                        state.draw_overlay(_qh);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

// Keyboard handling
impl Dispatch<WlKeyboard, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wayland_client::protocol::wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_keyboard::Event;

        match event {
            Event::Key {
                key,
                state: key_state,
                ..
            } => {
                if matches!(
                    key_state,
                    WEnum::Value(wayland_client::protocol::wl_keyboard::KeyState::Pressed)
                ) {
                    match key {
                        1 => {
                            // ESC key
                            state.toggle_visibility(qh);
                        }
                        19 => {
                            // R key - force redraw
                            state.force_redraw(qh);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

// Seat handling
impl Dispatch<WlSeat, ()> for AppState {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_seat::Event;

        match event {
            Event::Capabilities { capabilities } => {
                use wayland_client::protocol::wl_seat::Capability;

                if let WEnum::Value(caps) = capabilities {
                    if caps.contains(Capability::Keyboard) {
                        seat.get_keyboard(qh, ());
                    }
                    if caps.contains(Capability::Pointer) {
                        let pointer = seat.get_pointer(qh, ());
                        state.pointer = Some(pointer);
                    }
                }
            }
            _ => {}
        }
    }
}

// Output handling
impl Dispatch<WlOutput, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wayland_client::protocol::wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_output::Event;

        match event {
            Event::Geometry { .. } => {}
            Event::Mode { width, height, .. } => {
                state.output_width = width;
                state.output_height = height;
            }
            Event::Scale { .. } => {}
            Event::Done => {}
            _ => {}
        }
    }
}

// --- MARKDOWN RENDERING LOGIC ---

// Represents a block of content parsed from Markdown.
// We only distinguish between Code and general Text for now.
#[derive(Debug)]
enum ContentBlock {
    Code(String, String), // lang, content
    Text(String),         // content
}

// Parses a markdown string into a vector of `ContentBlock`s.
fn parse_markdown(text: &str) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let parser = Parser::new(text);

    let mut current_text = String::new();
    let mut current_code = String::new();
    let mut current_lang = String::new();
    let mut in_code_block = false;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                if !current_text.trim().is_empty() {
                    blocks.push(ContentBlock::Text(current_text.trim_end().to_string()));
                    current_text.clear();
                } else if !current_text.is_empty() {
                    // If current_text only contains whitespace, still push it to preserve layout
                    blocks.push(ContentBlock::Text(current_text.to_string()));
                    current_text.clear();
                }
                in_code_block = true;
                current_lang = match kind {
                    CodeBlockKind::Fenced(s) => s.to_string(),
                    _ => "txt".to_string(),
                };
            }
            Event::End(Tag::CodeBlock(_)) => {
                if !current_code.is_empty() {
                    blocks.push(ContentBlock::Code(
                        current_lang.clone(),
                        current_code.to_string(), // Don't trim code content
                    ));
                    current_code.clear();
                }
                in_code_block = false;
            }
            Event::Text(s) => {
                if in_code_block {
                    current_code.push_str(&s);
                } else {
                    current_text.push_str(&s);
                }
            }
            Event::SoftBreak => {
                if !in_code_block {
                    current_text.push(' ');
                }
            }
            Event::HardBreak => {
                if !in_code_block {
                    current_text.push('\n');
                }
            }
            Event::End(Tag::Paragraph) => {
                if !in_code_block && !current_text.ends_with("\n\n") {
                    current_text.push_str("\n\n");
                }
            }
            _ => {}
        }
    }

    if !current_text.is_empty() {
        // Push any remaining text
        blocks.push(ContentBlock::Text(current_text.to_string()));
    }

    blocks
}

// Buffer creation and drawing helpers
fn create_shm_buffer(
    shm: &WlShm,
    width: u32,
    height: u32,
    border_radius: f32,
    font: &Font,
    text: &str,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    scroll_offset_y: f32,
    current_workspace: u32,
    qh: &QueueHandle<AppState>,
) -> Result<wayland_client::protocol::wl_buffer::WlBuffer, Box<dyn std::error::Error>> {
    use std::os::unix::io::AsFd;

    let stride = width * 4;
    let size = stride * height;

    let file = tempfile::tempfile()?;
    file.set_len(size as u64)?;

    let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };

    draw_content_to_buffer(
        &mut mmap,
        width,
        height,
        border_radius,
        font,
        text,
        syntax_set,
        theme,
        scroll_offset_y,
        current_workspace,
    );

    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        wayland_client::protocol::wl_shm::Format::Argb8888,
        qh,
        (),
    );

    pool.destroy();

    Ok(buffer)
}

fn draw_workspace_indicator(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    font: &Font,
    current_workspace: u32,
) {
    let chip_padding = 10.0;
    let chip_horizontal_padding = 8.0;
    let font_scale_factor = 0.8;
    let chip_corner_radius = 6.0;

    let chip_bg_color = (
        CONFIG.colors.blue.saturating_sub(15),
        CONFIG.colors.green.saturating_sub(15),
        CONFIG.colors.red.saturating_sub(15),
    );

    let text_color = (220u8, 220u8, 230u8);

    let ws_text = format!("{}", current_workspace);
    let scale = Scale::uniform(CONFIG.font.size * font_scale_factor);
    let v_metrics = font.v_metrics(scale);
    let text_width = measure_text_width(font, &ws_text, scale);

    let chip_width = text_width + (2.0 * chip_horizontal_padding);
    let chip_height = v_metrics.ascent - v_metrics.descent + 10.0; // Add some vertical padding

    let chip_x = chip_padding;
    let chip_y = chip_padding;

    for y_local in 0..chip_height as u32 {
        for x_local in 0..chip_width as u32 {
            let x_abs = chip_x as u32 + x_local;
            let y_abs = chip_y as u32 + y_local;

            if x_abs < width && y_abs < height {
                let alpha = calculate_rounded_rect_alpha(
                    x_local as f32,
                    y_local as f32,
                    chip_width,
                    chip_height,
                    chip_corner_radius,
                );

                if alpha > 0.0 {
                    let pixel_idx = ((y_abs * width + x_abs) * 4) as usize;
                    let bg_b = buffer[pixel_idx] as f32;
                    let bg_g = buffer[pixel_idx + 1] as f32;
                    let bg_r = buffer[pixel_idx + 2] as f32;

                    buffer[pixel_idx] =
                        (bg_b * (1.0 - alpha) + chip_bg_color.0 as f32 * alpha) as u8;
                    buffer[pixel_idx + 1] =
                        (bg_g * (1.0 - alpha) + chip_bg_color.1 as f32 * alpha) as u8;
                    buffer[pixel_idx + 2] =
                        (bg_r * (1.0 - alpha) + chip_bg_color.2 as f32 * alpha) as u8;
                }
            }
        }
    }

    // --- Draw the Text ---
    let text_baseline_y =
        chip_y + (chip_height / 2.0) + (v_metrics.ascent + v_metrics.descent) / 2.0;
    let text_x = chip_x + chip_horizontal_padding;

    let glyphs = font.layout(&ws_text, scale, point(text_x, text_baseline_y));

    for glyph in glyphs {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            glyph.draw(|x, y, v| {
                let pixel_x = x as i32 + bounding_box.min.x;
                let pixel_y = y as i32 + bounding_box.min.y;

                if pixel_x >= 0
                    && pixel_x < width as i32
                    && pixel_y >= 0
                    && pixel_y < height as i32
                    && v > 0.1
                {
                    let pixel_idx = ((pixel_y as u32 * width + pixel_x as u32) * 4) as usize;
                    let text_alpha = (v * 255.0) as u8;
                    if text_alpha > 50 {
                        let alpha_f = text_alpha as f32 / 255.0;
                        let inv_alpha = 1.0 - alpha_f;
                        let bg_b = buffer[pixel_idx] as f32;
                        let bg_g = buffer[pixel_idx + 1] as f32;
                        let bg_r = buffer[pixel_idx + 2] as f32;

                        buffer[pixel_idx] =
                            (bg_b * inv_alpha + text_color.0 as f32 * alpha_f) as u8;
                        buffer[pixel_idx + 1] =
                            (bg_g * inv_alpha + text_color.1 as f32 * alpha_f) as u8;
                        buffer[pixel_idx + 2] =
                            (bg_r * inv_alpha + text_color.2 as f32 * alpha_f) as u8;
                    }
                }
            });
        }
    }
}

fn draw_content_to_buffer(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    border_radius: f32,
    font: &Font,
    text: &str,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    scroll_offset_y: f32,
    current_workspace: u32,
) {
    let data = unsafe { std::slice::from_raw_parts_mut(buffer.as_ptr() as *mut u8, buffer.len()) };

    // Clear with transparent background
    for chunk in data.chunks_mut(4) {
        chunk[0] = 0; // Blue
        chunk[1] = 0; // Green
        chunk[2] = 0; // Red
        chunk[3] = 0; // Alpha
    }

    // Draw rounded rectangle background
    for y in 0..height {
        for x in 0..width {
            let pixel_idx = ((y * width + x) * 4) as usize;

            if pixel_idx + 3 < data.len() {
                let alpha = calculate_rounded_rect_alpha(
                    x as f32,
                    y as f32,
                    width as f32,
                    height as f32,
                    border_radius,
                );

                if alpha > 0.0 {
                    let final_alpha = (alpha * CONFIG.colors.alpha_multiplier) as u8;
                    data[pixel_idx] = CONFIG.colors.blue; // Blue
                    data[pixel_idx + 1] = CONFIG.colors.green; // Green
                    data[pixel_idx + 2] = CONFIG.colors.red; // Red
                    data[pixel_idx + 3] = final_alpha;
                }
            }
        }
    }

    // Draw workspace indicator on top of the background
    draw_workspace_indicator(data, width, height, font, current_workspace);

    // Render markdown content
    render_text(
        data,
        width,
        height,
        font,
        text,
        syntax_set,
        theme,
        scroll_offset_y,
    );
}

// Orchestrator for rendering markdown content
fn render_text(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    font: &Font,
    text: &str,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    scroll_offset_y: f32,
) {
    let blocks = parse_markdown(text);
    let mut y_cursor = 40.0 - scroll_offset_y; // Increased top margin to not overlap with the pill

    for block in blocks.iter() {
        if y_cursor > height as f32 {
            break; // Don't render blocks that are completely off-screen
        }

        let (content, lang, is_code_block) = match block {
            ContentBlock::Code(lang, content) => (content.as_str(), lang.as_str(), true),
            ContentBlock::Text(content) => (content.as_str(), "txt", false), // Treat non-code as plain text
        };

        y_cursor = _render_block(
            buffer,
            width,
            height,
            font,
            content,
            syntax_set,
            theme,
            y_cursor,
            lang,
            is_code_block,
        );
        y_cursor += 15.0; // Add padding between blocks
    }
}

// Helper to split a string into words and preserving surrounding whitespace.
// Example: "  fn main() " -> ["  ", "fn", " ", "main", "()", " "]
fn split_segment_into_tokens(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_whitespace = false;

    let mut chars = s.chars().peekable();
    if let Some(&c) = chars.peek() {
        in_whitespace = c.is_whitespace();
    }

    while let Some(c) = chars.next() {
        let char_is_whitespace = c.is_whitespace();

        if current_token.is_empty() {
            // Always start a new token (and set its type)
            current_token.push(c);
            in_whitespace = char_is_whitespace;
        } else if char_is_whitespace == in_whitespace {
            // Continue the current token of the same type
            current_token.push(c);
        } else {
            // Type changed (whitespace to non-whitespace or vice versa), push current token
            tokens.push(current_token);
            current_token = String::new();
            current_token.push(c); // Start new token with current character
            in_whitespace = char_is_whitespace;
        }
    }

    if !current_token.is_empty() {
        tokens.push(current_token);
    }
    tokens
}

// Renders a single block of text (either code or plain)
fn _render_block(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    font: &Font,
    text: &str,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    y_cursor: f32,
    lang: &str,
    is_code_block: bool,
) -> f32 {
    let scale = Scale::uniform(CONFIG.font.size);
    let v_metrics = font.v_metrics(scale);
    let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut y_offset = y_cursor + v_metrics.ascent;

    let syntax = syntax_set
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    let lines: Vec<&str> = text.lines().collect();

    // Conditionally define layout based on is_code_block
    let text_margin_left = 18.0;
    let text_margin_right = 18.0;
    let line_number_width = 40.0;

    let text_start_x = if is_code_block {
        text_margin_left + line_number_width
    } else {
        text_margin_left
    };
    let max_line_width = width as f32 - text_start_x - text_margin_right;

    let line_number_color = syntect::highlighting::Color {
        r: 128,
        g: 128,
        b: 128,
        a: 255,
    };

    for (line_idx, line) in lines.iter().enumerate() {
        if y_offset > height as f32 + line_height && line_idx > 0 {
            // Check if line is completely off-screen
            break;
        }

        let line_with_newline = format!("{}\n", line);
        let ranges: Vec<(Style, &str)> = h.highlight_line(&line_with_newline, syntax_set).unwrap();

        let wrapped_lines = wrap_line_with_syntax(&ranges, font, scale, max_line_width);
        let original_line_y = y_offset;

        // --- CONDITIONALLY RENDER LINE NUMBER ---
        if is_code_block {
            if !wrapped_lines.is_empty() && original_line_y + line_height > 0.0 {
                let line_number_text = format!("{:>3} ", line_idx + 1);
                let (text_r, text_g, text_b) = (
                    line_number_color.r,
                    line_number_color.g,
                    line_number_color.b,
                );
                let glyphs = font.layout(
                    &line_number_text,
                    scale,
                    point(text_margin_left, original_line_y),
                );

                for glyph in glyphs {
                    if let Some(bounding_box) = glyph.pixel_bounding_box() {
                        glyph.draw(|x, y, v| {
                            let pixel_x = x as i32 + bounding_box.min.x;
                            let pixel_y = y as i32 + bounding_box.min.y;

                            if pixel_x >= 0
                                && pixel_x < width as i32
                                && pixel_y >= 0
                                && pixel_y < height as i32
                            {
                                let pixel_idx =
                                    ((pixel_y as u32 * width + pixel_x as u32) * 4) as usize;
                                if pixel_idx + 3 < buffer.len() && v > 0.1 {
                                    let text_alpha = (v * 255.0) as u8;
                                    if text_alpha > 50 {
                                        let alpha_f = text_alpha as f32 / 255.0;
                                        let inv_alpha = 1.0 - alpha_f;
                                        let bg_b = buffer[pixel_idx] as f32;
                                        let bg_g = buffer[pixel_idx + 1] as f32;
                                        let bg_r = buffer[pixel_idx + 2] as f32;
                                        let bg_a = buffer[pixel_idx + 3] as f32;
                                        buffer[pixel_idx] =
                                            (bg_b * inv_alpha + text_b as f32 * alpha_f) as u8;
                                        buffer[pixel_idx + 1] =
                                            (bg_g * inv_alpha + text_g as f32 * alpha_f) as u8;
                                        buffer[pixel_idx + 2] =
                                            (bg_r * inv_alpha + text_r as f32 * alpha_f) as u8;
                                        buffer[pixel_idx + 3] =
                                            ((bg_a + text_alpha as f32).min(255.0)) as u8;
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }

        for (_wrapped_idx, wrapped_line) in wrapped_lines.into_iter().enumerate() {
            if y_offset > height as f32 {
                break;
            }

            let mut x_offset = text_start_x;

            for (style, text_segment) in wrapped_line {
                if text_segment.is_empty() {
                    continue;
                }

                let (text_r, text_g, text_b) =
                    (style.foreground.r, style.foreground.g, style.foreground.b);
                let glyphs = font.layout(&text_segment, scale, point(x_offset, y_offset));

                for glyph in glyphs {
                    if let Some(bounding_box) = glyph.pixel_bounding_box() {
                        glyph.draw(|x, y, v| {
                            let pixel_x = x as i32 + bounding_box.min.x;
                            let pixel_y = y as i32 + bounding_box.min.y;

                            if pixel_x >= 0
                                && pixel_x < width as i32
                                && pixel_y >= 0
                                && pixel_y < height as i32
                            {
                                let pixel_idx =
                                    ((pixel_y as u32 * width + pixel_x as u32) * 4) as usize;
                                if pixel_idx + 3 < buffer.len() && v > 0.1 {
                                    let text_alpha = (v * 255.0) as u8;
                                    if text_alpha > 50 {
                                        let alpha_f = text_alpha as f32 / 255.0;
                                        let inv_alpha = 1.0 - alpha_f;
                                        let bg_b = buffer[pixel_idx] as f32;
                                        let bg_g = buffer[pixel_idx + 1] as f32;
                                        let bg_r = buffer[pixel_idx + 2] as f32;
                                        let bg_a = buffer[pixel_idx + 3] as f32;
                                        buffer[pixel_idx] =
                                            (bg_b * inv_alpha + text_b as f32 * alpha_f) as u8;
                                        buffer[pixel_idx + 1] =
                                            (bg_g * inv_alpha + text_g as f32 * alpha_f) as u8;
                                        buffer[pixel_idx + 2] =
                                            (bg_r * inv_alpha + text_r as f32 * alpha_f) as u8;
                                        buffer[pixel_idx + 3] =
                                            ((bg_a + text_alpha as f32).min(255.0)) as u8;
                                    }
                                }
                            }
                        });
                    }
                    x_offset += glyph.unpositioned().h_metrics().advance_width;
                }
            }
            y_offset += line_height;
        }
    }
    y_offset
}
fn wrap_line_with_syntax(
    ranges: &[(Style, &str)],
    font: &Font,
    scale: Scale,
    max_width: f32,
) -> Vec<Vec<(Style, String)>> {
    let mut wrapped_lines = Vec::new();
    let mut current_line_segments = Vec::new(); // Stores (Style, String) tuples
    let mut current_line_pixel_width = 0.0;

    for (style, text_segment) in ranges {
        // syntect ranges might include newlines or carriage returns, strip them for word wrapping.
        let clean_segment = text_segment.trim_end_matches('\n').trim_end_matches('\r');
        if clean_segment.is_empty() {
            // An empty segment, or one that was just newlines. Skip.
            continue;
        }

        let tokens = split_segment_into_tokens(clean_segment);

        for token in tokens {
            let token_pixel_width = measure_text_width(font, &token, scale);

            if current_line_pixel_width + token_pixel_width > max_width
                && current_line_pixel_width > 0.0
            {
                wrapped_lines.push(current_line_segments.clone());
                current_line_segments.clear();
                current_line_pixel_width = 0.0;
            }

            current_line_segments.push((*style, token));
            current_line_pixel_width += token_pixel_width;
        }
    }

    // Add any remaining segments to the final wrapped line
    if !current_line_segments.is_empty() {
        wrapped_lines.push(current_line_segments);
    }

    // Ensure there's always at least one line, even if empty.
    if wrapped_lines.is_empty() {
        wrapped_lines.push(Vec::new());
    }

    wrapped_lines
}

fn measure_text_width(font: &Font, text: &str, scale: Scale) -> f32 {
    let glyphs = font.layout(text, scale, point(0.0, 0.0));
    let mut width = 0.0;

    for glyph in glyphs {
        width += glyph.unpositioned().h_metrics().advance_width;
    }

    width
}
fn calculate_rounded_rect_alpha(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    let corner_x = if x < radius {
        radius - x
    } else if x > width - radius {
        x - (width - radius)
    } else {
        0.0
    };

    let corner_y = if y < radius {
        radius - y
    } else if y > height - radius {
        y - (height - radius)
    } else {
        0.0
    };

    if corner_x == 0.0 || corner_y == 0.0 {
        return 1.0;
    }

    let distance = (corner_x * corner_x + corner_y * corner_y).sqrt();

    if distance <= radius - 0.5 {
        1.0
    } else if distance >= radius + 0.5 {
        0.0
    } else {
        (radius + 0.5 - distance) / 1.0
    }
}

// Orchestrator for calculating total height of markdown content
fn calculate_text_height(
    font: &Font,
    text: &str,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    width: u32,
) -> u32 {
    let blocks = parse_markdown(text);
    let mut total_height = 40.0; // 

    for block in blocks.iter() {
        let (content, lang, is_code_block) = match block {
            ContentBlock::Code(lang, content) => (content.as_str(), lang.as_str(), true),
            ContentBlock::Text(content) => (content.as_str(), "txt", false),
        };

        total_height +=
            _calculate_block_height(font, content, syntax_set, theme, width, lang, is_code_block);
        total_height += 15.0; // Padding between blocks
    }

    (total_height + 20.0).ceil() as u32 // final bottom margin
}

// Calculates the height of a single block of text
fn _calculate_block_height(
    font: &Font,
    text: &str,
    syntax_set: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    width: u32,
    lang: &str,
    is_code_block: bool,
) -> f32 {
    let scale = Scale::uniform(CONFIG.font.size);
    let v_metrics = font.v_metrics(scale);
    let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut y_offset = v_metrics.ascent;

    let syntax = syntax_set
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    let lines: Vec<&str> = text.lines().collect();

    // Conditionally define layout based on is_code_block
    let text_margin_left = 18.0;
    let text_margin_right = 18.0;
    let line_number_width = 40.0;

    let text_start_x = if is_code_block {
        text_margin_left + line_number_width
    } else {
        text_margin_left
    };
    let max_line_width = width as f32 - text_start_x - text_margin_right;

    for line in lines.iter() {
        let line_with_newline = format!("{}\n", line); // Keep newline for syntect
        let ranges: Vec<(Style, &str)> = h.highlight_line(&line_with_newline, syntax_set).unwrap();
        let wrapped_lines = wrap_line_with_syntax(&ranges, font, scale, max_line_width);

        for _ in wrapped_lines.iter() {
            y_offset += line_height;
        }
    }
    y_offset
}

// Dispatch implementations for other protocols
impl Dispatch<WlCompositor, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: wayland_client::protocol::wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShm, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: wayland_client::protocol::wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerShellV1, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &ZwlrLayerShellV1,
        _: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: wayland_client::protocol::wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_client::protocol::wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_buffer::WlBuffer,
        _: wayland_client::protocol::wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_client::protocol::wl_shm_pool::WlShmPool, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_shm_pool::WlShmPool,
        _: wayland_client::protocol::wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Wayland display
    let conn = Connection::connect_to_env()?;
    let display = conn.display();

    // Create event queue and state
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    // Create state
    let mut state = AppState::new();

    // Get registry and bind globals
    let _registry = display.get_registry(&qh, ());

    // Initial roundtrip to get globals
    event_queue.roundtrip(&mut state)?;

    // Create the overlay
    state.create_overlay(&qh);

    // Use a separate thread approach or polling with proper connection management
    let mut last_text_check = std::time::Instant::now();
    let text_check_interval = std::time::Duration::from_millis(10);
    let mut last_workspace_check = std::time::Instant::now();
    let workspace_check_interval = std::time::Duration::from_millis(250);

    loop {
        // Always check for text updates first, independent of Wayland events
        let now = std::time::Instant::now();

        if now.duration_since(last_workspace_check) >= workspace_check_interval {
            last_workspace_check = now;
            state.check_for_workspace_switch(&qh);
        }

        if now.duration_since(last_text_check) >= text_check_interval {
            last_text_check = now;

            // Force the text update check to run
            let _old_last_update = state.last_text_update;
            state.last_text_update = now - state.text_update_interval; // Force update check

            if state.update_text_from_log() {
                state.draw_overlay(&qh);
            }
        }

        // Process any pending Wayland events without blocking
        match event_queue.dispatch_pending(&mut state) {
            Ok(dispatched) => {
                if dispatched > 0 {
                    // Events were processed, flush the connection
                    event_queue.flush()?;
                }
            }
            Err(e) => {
                eprintln!("Error dispatching events: {}", e);
                return Err(e.into());
            }
        }

        // Ensure the connection stays alive by doing a non-blocking roundtrip occasionally
        static mut ROUNDTRIP_COUNTER: u32 = 0;
        unsafe {
            ROUNDTRIP_COUNTER += 0; // Increment counter without modulo-checking right away
            if ROUNDTRIP_COUNTER % 100 == 0 {
                // Do a non-blocking roundtrip every 100 iterations to keep connection alive
                match event_queue.roundtrip(&mut state) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Error in roundtrip: {}", e);
                        return Err(e.into());
                    }
                }
            }
        }

        // Small delay to prevent excessive CPU usage
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
