use std::{
    collections::HashSet,
    hash::Hash,
    sync::{Arc, Mutex},
};

use crate::{font::Font, math::*, types::*};

// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize)]
pub struct LayoutOptions {
    /// Horizontal and vertical padding within a window frame.
    pub window_padding: Vec2,

    /// Horizontal and vertical spacing between widgets
    pub item_spacing: Vec2,

    /// Indent foldable regions etc by this much.
    pub indent: f32,

    /// Button size is text size plus this on each side
    pub button_padding: Vec2,

    /// Checkboxed, radio button and foldables have an icon at the start.
    /// The text starts after this many pixels.
    pub start_icon_width: f32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            item_spacing: vec2(8.0, 4.0),
            window_padding: vec2(6.0, 6.0),
            indent: 21.0,
            button_padding: vec2(5.0, 3.0),
            start_icon_width: 20.0,
        }
    }
}

// ----------------------------------------------------------------------------

// TODO: rename
pub struct GuiResponse {
    /// The mouse is hovering above this
    pub hovered: bool,

    /// The mouse went got pressed on this thing this frame
    pub clicked: bool,

    /// The mouse is interacting with this thing (e.g. dragging it)
    pub active: bool,

    /// Used for showing a popup (if any)
    data: Arc<Data>,
}

impl GuiResponse {
    /// Show some stuff if the item was hovered
    pub fn tooltip<F>(&mut self, add_contents: F) -> &mut Self
    where
        F: FnOnce(&mut Region),
    {
        if self.hovered {
            let window_pos = self.data.input().mouse_pos + vec2(16.0, 16.0);
            show_popup(&self.data, window_pos, add_contents);
        }
        self
    }

    /// Show this text if the item was hovered
    pub fn tooltip_text<S: Into<String>>(&mut self, text: S) -> &mut Self {
        self.tooltip(|popup| {
            popup.label(text);
        })
    }
}

// ----------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Memory {
    /// The widget being interacted with (e.g. dragged, in case of a slider).
    active_id: Option<Id>,

    /// Which foldable regions are open.
    open_foldables: HashSet<Id>,
}

// ----------------------------------------------------------------------------

struct TextFragment {
    /// The start of each character, starting at zero.
    x_offsets: Vec<f32>,
    /// 0 for the first line, n * line_spacing for the rest
    y_offset: f32,
    text: String,
}

type TextFragments = Vec<TextFragment>;

// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

impl Default for Direction {
    fn default() -> Direction {
        Direction::Vertical
    }
}

// ----------------------------------------------------------------------------

type Id = u64;

// ----------------------------------------------------------------------------

/// TODO: improve this
#[derive(Clone, Default)]
pub struct GraphicLayers {
    pub(crate) graphics: Vec<GuiCmd>,
    pub(crate) hovering_graphics: Vec<GuiCmd>,
}

impl GraphicLayers {
    pub fn drain(&mut self) -> impl ExactSizeIterator<Item = GuiCmd> {
        // TODO: there must be a nicer way to do this?
        let mut all_commands: Vec<_> = self.graphics.drain(..).collect();
        all_commands.extend(self.hovering_graphics.drain(..));
        all_commands.into_iter()
    }
}

// ----------------------------------------------------------------------------

// TODO: give a better name.
/// Contains the input, options and output of all GUI commands.
pub struct Data {
    pub(crate) options: LayoutOptions,
    pub(crate) font: Arc<Font>,
    pub(crate) input: GuiInput,
    pub(crate) memory: Mutex<Memory>,
    pub(crate) graphics: Mutex<GraphicLayers>,
}

impl Clone for Data {
    fn clone(&self) -> Self {
        Data {
            options: self.options.clone(),
            font: self.font.clone(),
            input: self.input.clone(),
            memory: Mutex::new(self.memory.lock().unwrap().clone()),
            graphics: Mutex::new(self.graphics.lock().unwrap().clone()),
        }
    }
}

impl Data {
    pub fn new(font: Arc<Font>) -> Data {
        Data {
            options: Default::default(),
            font,
            input: Default::default(),
            memory: Default::default(),
            graphics: Default::default(),
        }
    }

    pub fn input(&self) -> &GuiInput {
        &self.input
    }

    pub fn options(&self) -> &LayoutOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: LayoutOptions) {
        self.options = options;
    }

    // TODO: move
    pub fn new_frame(&mut self, gui_input: GuiInput) {
        self.input = gui_input;
        if !gui_input.mouse_down {
            self.memory.lock().unwrap().active_id = None;
        }
    }
}

/// Show a pop-over window
pub fn show_popup<F>(data: &Arc<Data>, window_pos: Vec2, add_contents: F)
where
    F: FnOnce(&mut Region),
{
    // TODO: nicer way to do layering!
    let num_graphics_before = data.graphics.lock().unwrap().graphics.len();

    let window_padding = data.options.window_padding;

    let mut popup_region = Region {
        data: data.clone(),
        id: Default::default(),
        dir: Direction::Vertical,
        cursor: window_pos + window_padding,
        bounding_size: vec2(0.0, 0.0),
        available_space: vec2(400.0, std::f32::INFINITY), // TODO
    };

    add_contents(&mut popup_region);

    // TODO: handle the last item_spacing in a nicer way
    let inner_size = popup_region.bounding_size - data.options.item_spacing;
    let outer_size = inner_size + 2.0 * window_padding;

    let rect = Rect::from_min_size(window_pos, outer_size);

    let mut graphics = data.graphics.lock().unwrap();
    let popup_graphics = graphics.graphics.split_off(num_graphics_before);
    graphics.hovering_graphics.push(GuiCmd::Window { rect });
    graphics.hovering_graphics.extend(popup_graphics);
}

// ----------------------------------------------------------------------------

/// Represents a region of the screen
/// with a type of layout (horizontal or vertical).
/// TODO: make Region a trait so we can have type-safe HorizontalRegion etc?
pub struct Region {
    pub(crate) data: Arc<Data>,

    /// Unique ID of this region.
    pub(crate) id: Id,

    /// Doesn't change.
    pub(crate) dir: Direction,

    /// Changes only along self.dir
    pub(crate) cursor: Vec2,

    /// Bounding box children.
    /// We keep track of our max-size along the orthogonal to self.dir
    pub(crate) bounding_size: Vec2,

    /// This how much space we can take up without overflowing our parent.
    /// Shrinks as cursor increments.
    pub(crate) available_space: Vec2,
}

impl Region {
    /// It is up to the caller to make sure there is room for this.
    /// Can be used for free painting.
    /// NOTE: all coordinates are screen coordinates!
    pub fn add_graphic(&mut self, gui_cmd: GuiCmd) {
        self.data.graphics.lock().unwrap().graphics.push(gui_cmd)
    }

    pub fn options(&self) -> &LayoutOptions {
        self.data.options()
    }

    pub fn input(&self) -> &GuiInput {
        self.data.input()
    }

    pub fn cursor(&self) -> Vec2 {
        self.cursor
    }

    pub fn button<S: Into<String>>(&mut self, text: S) -> GuiResponse {
        let text: String = text.into();
        let id = self.make_child_id(&text);
        let (text, text_size) = self.layout_text(&text);
        let text_cursor = self.cursor + self.options().button_padding;
        let (rect, interact) =
            self.reserve_space(text_size + 2.0 * self.options().button_padding, Some(id));
        self.add_graphic(GuiCmd::Button { interact, rect });
        self.add_text(text_cursor, text);
        self.response(interact)
    }

    pub fn checkbox<S: Into<String>>(&mut self, text: S, checked: &mut bool) -> GuiResponse {
        let text: String = text.into();
        let id = self.make_child_id(&text);
        let (text, text_size) = self.layout_text(&text);
        let text_cursor = self.cursor
            + self.options().button_padding
            + vec2(self.options().start_icon_width, 0.0);
        let (rect, interact) = self.reserve_space(
            self.options().button_padding
                + vec2(self.options().start_icon_width, 0.0)
                + text_size
                + self.options().button_padding,
            Some(id),
        );
        if interact.clicked {
            *checked = !*checked;
        }
        self.add_graphic(GuiCmd::Checkbox {
            checked: *checked,
            interact,
            rect,
        });
        self.add_text(text_cursor, text);
        self.response(interact)
    }

    pub fn label<S: Into<String>>(&mut self, text: S) -> GuiResponse {
        let text: String = text.into();
        let (text, text_size) = self.layout_text(&text);
        self.add_text(self.cursor, text);
        let (_, interact) = self.reserve_space(text_size, None);
        self.response(interact)
    }

    /// A radio button
    pub fn radio<S: Into<String>>(&mut self, text: S, checked: bool) -> GuiResponse {
        let text: String = text.into();
        let id = self.make_child_id(&text);
        let (text, text_size) = self.layout_text(&text);
        let text_cursor = self.cursor
            + self.options().button_padding
            + vec2(self.options().start_icon_width, 0.0);
        let (rect, interact) = self.reserve_space(
            self.options().button_padding
                + vec2(self.options().start_icon_width, 0.0)
                + text_size
                + self.options().button_padding,
            Some(id),
        );
        self.add_graphic(GuiCmd::RadioButton {
            checked,
            interact,
            rect,
        });
        self.add_text(text_cursor, text);
        self.response(interact)
    }

    pub fn slider_f32<S: Into<String>>(
        &mut self,
        text: S,
        value: &mut f32,
        min: f32,
        max: f32,
    ) -> GuiResponse {
        let text_string: String = text.into();
        if true {
            // Text to the right of the slider
            self.columns(2, |columns| {
                columns[1].label(format!("{}: {:.3}", text_string, value));
                columns[0].naked_slider_f32(&text_string, value, min, max)
            })
        } else {
            // Text above slider
            let (text, text_size) = self.layout_text(&format!("{}: {:.3}", text_string, value));
            self.add_text(self.cursor, text);
            self.reserve_space_inner(text_size);
            self.naked_slider_f32(&text_string, value, min, max)
        }
    }

    pub fn naked_slider_f32<H: Hash>(
        &mut self,
        id: &H,
        value: &mut f32,
        min: f32,
        max: f32,
    ) -> GuiResponse {
        debug_assert!(min <= max);
        let id = self.make_child_id(id);
        let (slider_rect, interact) = self.reserve_space(
            Vec2 {
                x: self.available_space.x,
                y: self.data.font.line_spacing(),
            },
            Some(id),
        );

        if interact.active {
            *value = remap_clamp(
                self.input().mouse_pos.x,
                slider_rect.min().x,
                slider_rect.max().x,
                min,
                max,
            );
        }

        self.add_graphic(GuiCmd::Slider {
            interact,
            max,
            min,
            rect: slider_rect,
            value: *value,
        });

        self.response(interact)
    }

    // ------------------------------------------------------------------------
    // Sub-regions:

    pub fn foldable<S, F>(&mut self, text: S, add_contents: F) -> GuiResponse
    where
        S: Into<String>,
        F: FnOnce(&mut Region),
    {
        assert!(
            self.dir == Direction::Vertical,
            "Horizontal foldable is unimplemented"
        );
        let text: String = text.into();
        let id = self.make_child_id(&text);
        let (text, text_size) = self.layout_text(&text);
        let text_cursor = self.cursor + self.options().button_padding;
        let (rect, interact) = self.reserve_space(
            vec2(
                self.available_space.x,
                text_size.y + 2.0 * self.options().button_padding.y,
            ),
            Some(id),
        );

        let open = {
            let mut memory = self.data.memory.lock().unwrap();
            if interact.clicked {
                if memory.open_foldables.contains(&id) {
                    memory.open_foldables.remove(&id);
                } else {
                    memory.open_foldables.insert(id);
                }
            }
            memory.open_foldables.contains(&id)
        };

        self.add_graphic(GuiCmd::FoldableHeader {
            interact,
            rect,
            open,
        });
        self.add_text(
            text_cursor + vec2(self.options().start_icon_width, 0.0),
            text,
        );

        if open {
            let old_id = self.id;
            self.id = id;
            self.indent(add_contents);
            self.id = old_id;
        }

        self.response(interact)
    }

    /// Create a child region which is indented to the right
    pub fn indent<F>(&mut self, add_contents: F)
    where
        F: FnOnce(&mut Region),
    {
        let indent = vec2(self.options().indent, 0.0);
        let mut child_region = Region {
            data: self.data.clone(),
            id: self.id,
            dir: self.dir,
            cursor: self.cursor + indent,
            bounding_size: vec2(0.0, 0.0),
            available_space: self.available_space - indent,
        };
        add_contents(&mut child_region);
        let size = child_region.bounding_size;
        self.reserve_space_inner(indent + size);
    }

    /// A horizontally centered region of the given width.
    pub fn centered_column(&mut self, width: f32) -> Region {
        Region {
            data: self.data.clone(),
            id: self.id,
            dir: self.dir,
            cursor: vec2((self.available_space.x - width) / 2.0, self.cursor.y),
            bounding_size: vec2(0.0, 0.0),
            available_space: vec2(width, self.available_space.y),
        }
    }

    /// Start a region with horizontal layout
    pub fn horizontal<F>(&mut self, add_contents: F)
    where
        F: FnOnce(&mut Region),
    {
        let mut child_region = Region {
            data: self.data.clone(),
            id: self.id,
            dir: Direction::Horizontal,
            cursor: self.cursor,
            bounding_size: vec2(0.0, 0.0),
            available_space: self.available_space,
        };
        add_contents(&mut child_region);
        let size = child_region.bounding_size;
        self.reserve_space_inner(size);
    }

    /// Temporarily split split a vertical layout into two column regions.
    ///
    ///     gui.columns(2, |columns| {
    ///         columns[0].label("First column");
    ///         columns[1].label("Second column");
    ///     });
    pub fn columns<F, R>(&mut self, num_columns: usize, add_contents: F) -> R
    where
        F: FnOnce(&mut [Region]) -> R,
    {
        // TODO: ensure there is space
        let padding = self.options().item_spacing.x;
        let total_padding = padding * (num_columns as f32 - 1.0);
        let column_width = (self.available_space.x - total_padding) / (num_columns as f32);

        let mut columns: Vec<Region> = (0..num_columns)
            .map(|col_idx| Region {
                data: self.data.clone(),
                id: self.make_child_id(&("column", col_idx)),
                dir: Direction::Vertical,
                cursor: self.cursor + vec2((col_idx as f32) * (column_width + padding), 0.0),
                bounding_size: vec2(0.0, 0.0),
                available_space: vec2(column_width, self.available_space.y),
            })
            .collect();

        let result = add_contents(&mut columns[..]);

        let mut max_height = 0.0;
        for region in columns {
            let size = region.bounding_size;
            max_height = size.y.max(max_height);
        }

        self.reserve_space_inner(vec2(self.available_space.x, max_height));
        result
    }

    // ------------------------------------------------------------------------

    pub fn reserve_space(
        &mut self,
        size: Vec2,
        interaction_id: Option<Id>,
    ) -> (Rect, InteractInfo) {
        let rect = Rect {
            pos: self.cursor,
            size,
        };
        self.reserve_space_inner(size + self.options().item_spacing);
        let hovered = rect.contains(self.input().mouse_pos);
        let clicked = hovered && self.input().mouse_clicked;
        let active = if interaction_id.is_some() {
            let mut memory = self.data.memory.lock().unwrap();
            if clicked {
                memory.active_id = interaction_id;
            }
            memory.active_id == interaction_id
        } else {
            false
        };

        let interact = InteractInfo {
            hovered,
            clicked,
            active,
        };
        (rect, interact)
    }

    /// Reserve this much space and move the cursor.
    fn reserve_space_inner(&mut self, size: Vec2) {
        if self.dir == Direction::Horizontal {
            self.cursor.x += size.x;
            self.available_space.x -= size.x;
            self.bounding_size.x += size.x;
            self.bounding_size.y = self.bounding_size.y.max(size.y);
        } else {
            self.cursor.y += size.y;
            self.available_space.y -= size.x;
            self.bounding_size.y += size.y;
            self.bounding_size.x = self.bounding_size.x.max(size.x);
        }
    }

    fn make_child_id<H: Hash>(&self, child_id: &H) -> Id {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write_u64(self.id);
        child_id.hash(&mut hasher);
        hasher.finish()
    }

    // TODO: move this function
    fn layout_text(&self, text: &str) -> (TextFragments, Vec2) {
        let line_spacing = self.data.font.line_spacing();
        let mut cursor_y = 0.0;
        let mut max_width = 0.0;
        let mut text_fragments = Vec::new();
        for line in text.split('\n') {
            let x_offsets = self.data.font.layout_single_line(&line);
            let line_width = *x_offsets.last().unwrap();
            text_fragments.push(TextFragment {
                x_offsets,
                y_offset: cursor_y,
                text: line.into(),
            });

            cursor_y += line_spacing;
            max_width = line_width.max(max_width);
        }
        let bounding_size = vec2(max_width, cursor_y);
        (text_fragments, bounding_size)
    }

    fn add_text(&mut self, pos: Vec2, text: Vec<TextFragment>) {
        for fragment in text {
            self.add_graphic(GuiCmd::Text {
                pos: pos + vec2(0.0, fragment.y_offset),
                style: TextStyle::Label,
                text: fragment.text,
                x_offsets: fragment.x_offsets,
            });
        }
    }

    fn response(&mut self, interact: InteractInfo) -> GuiResponse {
        GuiResponse {
            hovered: interact.hovered,
            clicked: interact.clicked,
            active: interact.active,
            data: self.data.clone(),
        }
    }
}