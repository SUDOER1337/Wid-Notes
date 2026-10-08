// File: src/sidebar.rs
// Types:
// SidebarView()      - Left activity-bar view a user can switch to
// RightView()        - Right activity-bar view (preview, agent chat)
// RailIcon()         - Glyph vocabulary shared by both rails
// RailSide()         - Which window edge a rail hugs (decides the accent edge)
// RailEntry          - Trait implemented by both view enums so one rail renderer serves them
// SearchHit()        - One matching line produced by the Search view
// Sidebar            - Left activity-bar selection + side panel state
// RightSidebar       - Right activity-bar selection + side panel state
//
// Functions:
//   SidebarView::label()          - Human readable title for the side panel header
//   SidebarView::all()            - Ordered list of views shown in the activity bar
//   Sidebar::toggle(view)         - Open a view, or collapse the panel if already active
//   Sidebar::is_active(view)      - Whether a view is the currently selected one
//   Sidebar::activity_bar_ui()    - Draw the left icon strip and handle selection clicks
//   RightView::label()            - Human readable title for the right panel header
//   RightView::all()              - Ordered list of right-rail views
//   RightSidebar::toggle(view)    - Open a view, or collapse the panel if already active
//   RightSidebar::is_open(view)   - Whether a view drives the right panel
//   RightSidebar::set_open()      - Show/hide a view from a menu checkbox
//   RightSidebar::activity_bar_ui() - Draw the right icon strip and handle selection clicks
//   rail_ui()                     - Shared rail renderer: buttons, pinning, click result
//   paint_icon()                  - Vector placeholder glyph for one icon
//   panel_header()                - Uppercase side panel title with optional hint
//   section_label()               - Subdued uppercase PAGES / JOURNALS heading
//   nav_row()                     - Full-width navigation row with hover/selected states
//   new_page_control()            - New-page text field plus a separate "+" action
//   render_placeholder()          - "not implemented yet" body for stub views
//
// Helpers:
//   slot_rect()                   - Activity-bar row rect at a given y offset
//   accent_stripe()               - Active-row stripe on the edge facing the panel
//   fit_hint()                    - Hide a header hint that would not fit
//   label_in_rect()               - Vertically centered, ellipsized label inside a rect
//   header_fg()                   - Theme-derived color for the panel header title
//   subtle_fg()                   - Theme-derived color for secondary text
//   icon_fg()                     - Theme-derived resting color for rail icons
//   selected_bg()                 - Theme-derived fill for the selected explorer row

use crate::theme::Theme;
use egui::{Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2, epaint::StrokeKind};

/// Width of the icon strip, matching VSCode's activity bar.
pub const ACTIVITY_BAR_WIDTH: f32 = 48.0;
/// Height of each activity-bar row; always one button tall so the rail reads as a grid.
const ICON_BUTTON: f32 = 42.0;
/// Design grid every glyph is authored on (a 24x24 box).
const GRID: f32 = 24.0;
/// Fraction of the button edge a glyph occupies; the rest is padding.
const ICON_FILL: f32 = 0.5;
/// Width of the left accent stripe marking the active view and the selected row.
pub const ACCENT_WIDTH: f32 = 2.0;
/// Inset between a row's edge and its text.
const ROW_PAD_X: f32 = 8.0;
/// Height of a page/journal navigation row.
pub const ROW_HEIGHT: f32 = 22.0;
/// Height of the new-page control; also the edge of its square "+" button.
pub const CONTROL_HEIGHT: f32 = 26.0;
/// Corner radius for rows, fields and buttons.
const CORNER_RADIUS: f32 = 3.0;
/// Text size of a page/journal entry row.
const ENTRY_TEXT_SIZE: f32 = 13.0;
/// Text size of a PAGES / JOURNALS group heading.
const SECTION_TEXT_SIZE: f32 = 11.0;
/// Space above a section label, separating it from the previous group.
const SECTION_GAP: f32 = 10.0;
/// Space between the header rule and the content below it.
const HEADER_GAP: f32 = 6.0;

// ── SidebarView ──

/// A panel that can be selected from the activity bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarView {
    /// Pages and journals tree.
    Explorer,
    /// Full-text search across workspace notes.
    Search,
    /// Page link graph (not implemented yet).
    Graph,
    /// Note metadata / frontmatter inspector (not implemented yet).
    Outline,
    /// Workspace settings.
    Settings,
    /// Whiteboard
    Whiteboard,
}

impl SidebarView {
    /// Title shown in the side panel header.
    pub fn label(self) -> &'static str {
        match self {
            Self::Explorer => "Explorer",
            Self::Search => "Search",
            Self::Graph => "Graph",
            Self::Outline => "Outline",
            Self::Settings => "Settings",
            Self::Whiteboard => "Whiteboard",
        }
    }

    /// Activity-bar order, top to bottom.
    pub fn all() -> [SidebarView; 6] {
        [
            Self::Explorer,
            Self::Search,
            Self::Graph,
            Self::Outline,
            Self::Whiteboard,
            Self::Settings,
        ]
    }
}

impl RailEntry for SidebarView {
    fn icon(self) -> RailIcon {
        match self {
            Self::Explorer => RailIcon::Explorer,
            Self::Search => RailIcon::Search,
            Self::Graph => RailIcon::Graph,
            Self::Outline => RailIcon::Outline,
            Self::Settings => RailIcon::Settings,
            Self::Whiteboard => RailIcon::Whiteboard,
        }
    }

    fn label(self) -> &'static str {
        SidebarView::label(self)
    }

    /// Settings pins to the rail's far end like VSCode; the rest stack from the top.
    fn pinned(self) -> bool {
        self == Self::Settings
    }
}

// ── RightView ──

/// A panel that can be selected from the right-hand activity bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightView {
    /// Rendered markdown of the current buffer.
    Preview,
    /// Agent chat over a local model.
    Agent,
}

impl RightView {
    /// Title shown in the side panel header.
    pub fn label(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Agent => "Agent Chat",
        }
    }

    /// Right-rail order, top to bottom.
    pub fn all() -> [RightView; 2] {
        [Self::Preview, Self::Agent]
    }
}

impl RailEntry for RightView {
    fn icon(self) -> RailIcon {
        match self {
            Self::Preview => RailIcon::Preview,
            Self::Agent => RailIcon::Agent,
        }
    }

    fn label(self) -> &'static str {
        RightView::label(self)
    }
}

// ── Rail vocabulary ──

/// One glyph slot, shared by both rails so `rail_ui` never knows about view enums.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RailIcon {
    /// Two offset sheets — a page/file stack.
    Explorer,
    /// Magnifier — search.
    Search,
    /// Three joined nodes — a graph.
    Graph,
    /// Stacked rules with a leading tick — an outline.
    Outline,
    /// Framed board with a pen stroke — a whiteboard.
    Whiteboard,
    /// Hub ring with radial teeth — settings.
    Settings,
    /// Framed page split down the middle — a preview.
    Preview,
    /// Speech bubble with a tail — a chat.
    Agent,
}

/// Which window edge a rail sits against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailSide {
    /// Rail hugs the left edge; its panel opens to the right.
    Left,
    /// Rail hugs the right edge; its panel opens to the left.
    Right,
}

/// Anything an activity-bar row can show. Lets one renderer paint both rails.
pub trait RailEntry: Copy + PartialEq {
    /// Glyph painted in the button.
    fn icon(self) -> RailIcon;
    /// Tooltip / hover text, and the panel header title.
    fn label(self) -> &'static str;
    /// Whether the row pins to the rail's far end instead of stacking from the top.
    fn pinned(self) -> bool {
        false
    }
}

/// One matching line from the Search view.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub page: String,
    pub line_no: usize,
    pub line_text: String,
}

// ── Sidebar ──

/// State backing the activity bar and its side panel.
#[derive(Debug)]
pub struct Sidebar {
    /// Selected view; the side panel always renders this one.
    pub view: SidebarView,
    /// Whether the side panel (next to the icons) is shown at all.
    pub open: bool,
    /// Free-text query for the Search view.
    pub query: String,
    /// Results for the Search view, filled in by `app.rs`.
    pub hits: Vec<SearchHit>,
    /// Whether the last search produced zero results (drives the empty-state text).
    pub searched: bool,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            view: SidebarView::Explorer,
            open: true,
            query: String::new(),
            hits: Vec::new(),
            searched: false,
        }
    }
}

impl Sidebar {
    /** Select a view, collapsing the panel when it is already selected.
     *  @param view - The view the user clicked in the activity bar
     */
    pub fn toggle(&mut self, view: SidebarView) {
        // Collapse on second click, matching VSCode's toggle behaviour.
        if self.view == view && self.open {
            self.open = false;
        } else {
            self.view = view;
            self.open = true;
        }
    }

    /// Whether `view` is the one currently driving the side panel.
    pub fn is_active(&self, view: SidebarView) -> bool {
        self.open && self.view == view
    }

    /** Render the left activity bar: a vertical strip of square icon buttons.
     *  @param ui - Panel ui; expected to be `ACTIVITY_BAR_WIDTH` wide
     *  @param theme - Active theme for background, hover and active colors
     */
    pub fn activity_bar_ui(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let views = SidebarView::all();
        let active = views.iter().copied().find(|v| self.is_active(*v));
        if let Some(view) = rail_ui(ui, RailSide::Left, &views, active, theme) {
            self.toggle(view);
        }
    }
}

// ── RightSidebar ──

/// State backing the right-hand activity bar and its side panel.
#[derive(Clone, Copy, Debug)]
pub struct RightSidebar {
    /// Selected view; the side panel always renders this one.
    pub view: RightView,
    /// Whether the side panel (next to the icons) is shown at all.
    pub open: bool,
}

impl Default for RightSidebar {
    fn default() -> Self {
        // Closed by default: the rail is always visible, the panel is opt-in.
        Self {
            view: RightView::Agent,
            open: false,
        }
    }
}

impl RightSidebar {
    /** Select a view, collapsing the panel when it is already selected.
     *  @param view - The view the user clicked in the right activity bar
     */
    pub fn toggle(&mut self, view: RightView) {
        // Collapse on second click, matching VSCode's toggle behaviour.
        if self.view == view && self.open {
            self.open = false;
        } else {
            self.view = view;
            self.open = true;
        }
    }

    /// Whether `view` is the one currently driving the right panel.
    pub fn is_open(&self, view: RightView) -> bool {
        self.open && self.view == view
    }

    /** Show or hide `view` from a menu checkbox, without switching views.
     *  Hiding a view that is not the active one is a no-op: there is only ever
     *  one right panel, so the checkbox must not steal it.
     *  @param view - The view whose checkbox changed
     *  @param open - The checkbox's new state
     */
    pub fn set_open(&mut self, view: RightView, open: bool) {
        match (open, self.view == view) {
            (true, _) => {
                self.view = view;
                self.open = true;
            }
            (false, true) => self.open = false,
            (false, false) => {}
        }
    }

    /** Render the right activity bar: a vertical strip of square icon buttons.
     *  @param ui - Panel ui; expected to be `ACTIVITY_BAR_WIDTH` wide
     *  @param theme - Active theme for background, hover and active colors
     */
    pub fn activity_bar_ui(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let views = RightView::all();
        let active = views.iter().copied().find(|v| self.is_open(*v));
        if let Some(view) = rail_ui(ui, RailSide::Right, &views, active, theme) {
            self.toggle(view);
        }
    }
}

// ── Shared rail renderer ──

/** Draw one activity bar and report the row the user clicked.
 *  Both rails run through here, so the icon grid, hover states and active accent
 *  can never drift apart between the left and right strips.
 *  @param ui - Panel ui; expected to be `ACTIVITY_BAR_WIDTH` wide and full height
 *  @param side - Which edge the rail hugs; decides the accent stripe's edge
 *  @param views - Rows in top-to-bottom order; `RailEntry::pinned` rows sink to the bottom
 *  @param active - View driving the side panel, or `None` when the panel is collapsed
 *  @param theme - Active theme for background, hover and active colors
 *  @returns The clicked view, if the click landed on a row
 */
pub fn rail_ui<T: RailEntry>(
    ui: &mut egui::Ui,
    side: RailSide,
    views: &[T],
    active: Option<T>,
    theme: &Theme,
) -> Option<T> {
    ui.painter()
        .rect_filled(ui.max_rect(), 0.0, theme.titlebar.background);

    let bar_w = ui.available_width();
    let bar_h = ui.available_height();
    let stacked_h = views.iter().filter(|v| !v.pinned()).count() as f32 * ICON_BUTTON;
    let pinned_n = views.iter().filter(|v| v.pinned()).count() as f32;
    // Never let pinned rows overlap the stacked rows on a short window.
    let pinned_y = (bar_h - ICON_BUTTON * pinned_n).max(stacked_h);

    let mut clicked = None;
    ui.allocate_ui(Vec2::new(bar_w, bar_h), |ui| {
        let mut y = 0.0;
        for view in views.iter().filter(|v| !v.pinned()) {
            if rail_button(
                ui,
                side,
                slot_rect(y, bar_w),
                *view,
                Some(*view) == active,
                theme,
            ) {
                clicked = Some(*view);
            }
            y += ICON_BUTTON;
        }
        let mut y = pinned_y;
        for view in views.iter().filter(|v| v.pinned()) {
            if rail_button(
                ui,
                side,
                slot_rect(y, bar_w),
                *view,
                Some(*view) == active,
                theme,
            ) {
                clicked = Some(*view);
            }
            y += ICON_BUTTON;
        }
    });
    clicked
}

/** Draw one activity-bar button; returns true when clicked.
 *  @param ui - Panel ui the button rect is relative to
 *  @param side - Which edge the rail hugs; decides the accent stripe's edge
 *  @param slot - Button rect
 *  @param view - View this row stands for
 *  @param active - Whether the side panel is currently showing this view
 *  @param theme - Active theme for fill, accent and icon colors
 *  @returns True when the button was clicked
 */
fn rail_button<T: RailEntry>(
    ui: &mut egui::Ui,
    side: RailSide,
    slot: Rect,
    view: T,
    active: bool,
    theme: &Theme,
) -> bool {
    // Click sense, not hover: hover alone would make `response.clicked()` a
    // permanent false and the icon would never select its view.
    let (response, painter) = ui.allocate_painter(slot.size(), Sense::click());
    let rect = response.rect;
    let hovered = response.hovered();

    // Restrained active state: the theme's active fill plus a 2px accent on the
    // edge the panel sits on. Hover never paints over an active row, so the rail
    // never flickers between two backgrounds as the pointer crosses the icon.
    if active {
        painter.rect_filled(rect, 0.0, theme.sidebar.active_bg);
        painter.rect_filled(accent_stripe(rect, side), 0.0, theme.sidebar.active_fg);
    } else if hovered {
        painter.rect_filled(rect, 0.0, theme.sidebar.hover_bg);
    }

    // Resting icons sit back a little; hover and active bring them forward.
    let color = if active {
        theme.sidebar.active_fg
    } else if hovered {
        theme.sidebar.foreground
    } else {
        icon_fg(theme)
    };
    paint_icon(&painter, rect, view.icon(), color);

    let clicked = response.clicked();
    response.on_hover_text(view.label());
    clicked
}

/// Activity-bar row at `y`; full rail width so the glyph centers predictably.
fn slot_rect(y: f32, width: f32) -> Rect {
    Rect::from_min_size(Pos2::new(0.0, y), Vec2::new(width, ICON_BUTTON))
}

/// Active-row stripe on the rail edge that faces the side panel.
fn accent_stripe(rect: Rect, side: RailSide) -> Rect {
    let width = rect.width().min(ACCENT_WIDTH);
    match side {
        RailSide::Left => Rect::from_min_size(rect.left_top(), Vec2::new(width, rect.height())),
        RailSide::Right => Rect::from_min_size(
            Pos2::new(rect.right() - width, rect.top()),
            Vec2::new(width, rect.height()),
        ),
    }
}

// ── Icons ──

/** Paint a placeholder glyph for `icon`, centered and 1:1 inside `rect`.
 *  @param painter - Painter bound to the activity-bar layer
 *  @param rect - Button rect to center the glyph in
 *  @param icon - Which glyph to draw
 *  @param color - Stroke/fill color
 */
fn paint_icon(painter: &egui::Painter, rect: Rect, icon: RailIcon, color: Color32) {
    // One uniform scale from the design grid. Deriving it from min(width, height)
    // is what guarantees no icon is ever stretched — the glyph box is square and
    // so is the button, so they land 1:1 at any activity-bar width.
    let px = rect.width().min(rect.height()) * ICON_FILL;
    let scale = px / GRID;
    let origin = rect.center() - Vec2::splat(px / 2.0);
    // Design-grid coords -> screen. Only ever multiplied by `scale`.
    let p = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;

    let stroke = Stroke::new(1.6, color);
    let line =
        |a: (f32, f32), b: (f32, f32)| Shape::line_segment([p(a.0, a.1), p(b.0, b.1)], stroke);
    let dot = |x: f32, y: f32| Shape::circle_filled(p(x, y), 2.0 * scale, color);

    let shapes: Vec<Shape> = match icon {
        // Two offset sheets — a page/file stack.
        RailIcon::Explorer => vec![
            Shape::rect_stroke(
                Rect::from_min_size(p(5.0, 5.0), Vec2::new(11.0, 14.0) * scale),
                2.0,
                stroke,
                StrokeKind::Middle,
            ),
            Shape::rect_stroke(
                Rect::from_min_size(p(8.0, 8.0), Vec2::new(11.0, 14.0) * scale),
                2.0,
                stroke,
                StrokeKind::Middle,
            ),
        ],
        // Magnifier: circle lens plus a diagonal handle.
        RailIcon::Search => vec![
            Shape::circle_stroke(p(10.5, 10.5), 5.5 * scale, stroke),
            line((14.5, 14.5), (18.5, 18.5)),
        ],
        // Three nodes joined by edges — a graph.
        RailIcon::Graph => vec![
            line((6.0, 17.0), (12.0, 7.0)),
            line((12.0, 7.0), (18.0, 17.0)),
            line((6.0, 17.0), (18.0, 17.0)),
            dot(12.0, 7.0),
            dot(6.0, 17.0),
            dot(18.0, 17.0),
        ],
        // Stacked rules with a leading tick — a document outline.
        RailIcon::Outline => vec![
            line((4.0, 6.0), (20.0, 6.0)),
            line((4.0, 12.0), (20.0, 12.0)),
            line((4.0, 18.0), (14.0, 18.0)),
            dot(4.0, 6.0),
            dot(4.0, 12.0),
            dot(4.0, 18.0),
        ],
        // Frame with a pen stroke across it — a whiteboard.
        RailIcon::Whiteboard => vec![
            Shape::rect_stroke(
                Rect::from_min_size(p(3.5, 4.5), Vec2::new(17.0, 15.0) * scale),
                2.0,
                stroke,
                StrokeKind::Middle,
            ),
            line((7.0, 16.0), (17.0, 8.0)),
        ],
        // Gear approximation: hub ring with eight radial teeth.
        RailIcon::Settings => {
            let mut v = vec![Shape::circle_stroke(p(12.0, 12.0), 4.5 * scale, stroke)];
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let dir = Vec2::new(a.cos(), a.sin());
                let center = p(12.0, 12.0);
                v.push(Shape::line_segment(
                    [center + dir * 6.5 * scale, center + dir * 9.5 * scale],
                    stroke,
                ));
            }
            v
        }
        // Page split down the middle — a live preview beside the source.
        RailIcon::Preview => vec![
            Shape::rect_stroke(
                Rect::from_min_size(p(3.5, 4.5), Vec2::new(17.0, 15.0) * scale),
                2.0,
                stroke,
                StrokeKind::Middle,
            ),
            line((12.0, 4.5), (12.0, 19.5)),
        ],
        // Speech bubble with a tail — an agent conversation.
        RailIcon::Agent => vec![
            Shape::rect_stroke(
                Rect::from_min_size(p(3.5, 4.5), Vec2::new(17.0, 12.0) * scale),
                2.0,
                stroke,
                StrokeKind::Middle,
            ),
            line((8.5, 16.5), (8.5, 20.5)),
            line((8.5, 20.5), (12.5, 16.5)),
        ],
    };

    for shape in shapes {
        painter.add(shape);
    }
}

// ── Side panel building blocks ──

/** Draw the side panel header: uppercase title with an optional right-hand hint.
 *  A hint that would not fit next to the title is dropped instead of clipped,
 *  so a narrow panel degrades by losing decoration rather than text.
 *  @param ui - Panel ui, already inset by the panel frame
 *  @param title - View title, drawn uppercase
 *  @param hint - Optional short hint pinned to the right edge
 *  @param theme - Active theme for the title, hint and rule colors
 */
pub fn panel_header(ui: &mut egui::Ui, title: &str, hint: Option<&str>, theme: &Theme) {
    ui.horizontal(|ui| {
        // One flat row: no item spacing, the title and hint own their own insets.
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let title_rect = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(title.to_uppercase())
                        .size(11.0)
                        .strong()
                        .color(header_fg(theme)),
                )
                .truncate(),
            )
            .rect;

        if let Some(h) = fit_hint(ui, title_rect.right(), hint, theme) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(h).size(11.0).color(subtle_fg(theme)))
                        .truncate(),
                );
            });
        }
    });

    ui.add_space(HEADER_GAP);
    let (rule, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(0.0), 1.0),
        Sense::hover(),
    );
    ui.painter().rect_filled(rule, 0.0, theme.sidebar.separator);
    ui.add_space(2.0);
}

/** Draw a subdued uppercase group heading such as PAGES or JOURNALS.
 *  @param ui - Panel ui, positioned above the group
 *  @param text - Group name, drawn uppercase
 *  @param theme - Active theme used to derive the muted text color
 */
pub fn section_label(ui: &mut egui::Ui, text: &str, theme: &Theme) {
    ui.add_space(SECTION_GAP);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(0.0), 16.0),
        Sense::hover(),
    );
    label_in_rect(
        ui,
        rect,
        &text.to_uppercase(),
        subtle_fg(theme),
        SECTION_TEXT_SIZE,
    );
    ui.add_space(2.0);
}

/** Draw one full-width navigation row; returns true when it was clicked.
 *  A selected row gets a soft fill plus the left accent stripe, so the open
 *  document reads as a sidebar item rather than a text selection.
 *  @param ui - Panel ui, positioned at the top of the row
 *  @param text - Row label, ellipsized when the panel is narrow
 *  @param selected - Whether this row points at the document currently open
 *  @param theme - Active theme for fill, accent and text colors
 *  @returns True when the row was clicked
 */
pub fn nav_row(ui: &mut egui::Ui, text: &str, selected: bool, theme: &Theme) -> bool {
    let width = ui.available_width().max(0.0);
    let (response, painter) = ui.allocate_painter(Vec2::new(width, ROW_HEIGHT), Sense::click());
    let rect = response.rect;

    if selected {
        painter.rect_filled(rect, CORNER_RADIUS, selected_bg(theme));
        let stripe = Rect::from_min_size(rect.left_top(), Vec2::new(ACCENT_WIDTH, rect.height()));
        painter.rect_filled(stripe, 0.0, theme.sidebar.active_fg);
    } else if response.hovered() {
        painter.rect_filled(rect, CORNER_RADIUS, theme.sidebar.hover_bg);
    }

    let color = if selected {
        theme.sidebar.active_fg
    } else {
        theme.sidebar.foreground
    };
    label_in_rect(ui, rect, text, color, ENTRY_TEXT_SIZE);

    response.clicked()
}

/** Draw the "New page" control: a quiet text field plus a separate "+" action.
 *  @param ui - Panel ui, positioned at the top of the control
 *  @param text - Buffer holding the page name being typed
 *  @param theme - Active theme for the field, the button and the focus ring
 *  @returns True when the page should be created (Enter pressed, or "+" clicked)
 */
pub fn new_page_control(ui: &mut egui::Ui, text: &mut String, theme: &Theme) -> bool {
    let gap = 6.0;
    let field_w = (ui.available_width() - CONTROL_HEIGHT - gap).max(40.0);
    let mut create = false;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);

        // Style the field through egui's own visuals so cursor, selection and
        // focus handling keep working; only the colors are themed here.
        let visuals = &mut ui.visuals_mut();
        // A quiet 1px ring instead of the bright default selection stroke.
        visuals.selection.stroke = Stroke::new(1.0, subtle_fg(theme));
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.corner_radius = CORNER_RADIUS.into();
            widget.bg_stroke = Stroke::new(1.0, theme.sidebar.border);
        }

        let edit = ui.add(
            egui::TextEdit::singleline(text)
                .hint_text("New page...")
                .background_color(theme.palette.input_bg)
                .desired_width(field_w)
                .min_size(Vec2::new(field_w, CONTROL_HEIGHT)),
        );
        create |= edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        // "+" stays a separate square button: a quiet surface at rest, a touch
        // brighter on hover, so the action reads as a button without shouting.
        let hovered_bg = theme
            .sidebar
            .hover_bg
            .lerp_to_gamma(theme.sidebar.foreground, 0.18);
        let visuals = &mut ui.visuals_mut();
        for (widget, fill) in [
            (&mut visuals.widgets.inactive, theme.sidebar.hover_bg),
            (&mut visuals.widgets.hovered, hovered_bg),
            (&mut visuals.widgets.active, hovered_bg),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::NONE;
            widget.corner_radius = CORNER_RADIUS.into();
        }
        if ui
            .add_sized([CONTROL_HEIGHT, CONTROL_HEIGHT], egui::Button::new("+"))
            .clicked()
        {
            create = true;
        }
    });

    create
}

/// Placeholder message for views whose data layer does not exist yet.
pub fn render_placeholder(ui: &mut egui::Ui, view: SidebarView) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(
            egui::RichText::new(format!("{} is not implemented yet", view.label()))
                .color(Color32::from_gray(150))
                .size(12.0),
        );
    });
}

// ── Text and theme helpers ──

/** Return `hint` when it still fits to the right of `after`, otherwise `None`.
 *  @param ui - Panel ui the hint would be drawn in
 *  @param after - Right edge the hint must clear
 *  @param hint - Candidate hint text
 *  @param theme - Active theme, used to render the hint while measuring
 *  @returns The hint, or `None` when it has to be dropped
 */
fn fit_hint<'a>(
    ui: &egui::Ui,
    after: f32,
    hint: Option<&'a str>,
    theme: &Theme,
) -> Option<&'a str> {
    let hint = hint?;
    let width = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            hint.to_owned(),
            FontId::proportional(11.0),
            subtle_fg(theme),
        )
        .size()
        .x
    });
    (after + ROW_PAD_X + width <= ui.max_rect().right()).then_some(hint)
}

/** Draw `text` inside `rect`, vertically centered and ellipsized when too long.
 *  @param ui - Panel ui that owns `rect`
 *  @param rect - Area to fill; the text is inset by `ROW_PAD_X` on both sides
 *  @param text - Text to draw
 *  @param color - Text color
 *  @param size - Font size in points
 */
fn label_in_rect(ui: &mut egui::Ui, rect: Rect, text: &str, color: Color32, size: f32) {
    let text_rect = rect.shrink2(Vec2::new(ROW_PAD_X, 0.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(text).size(size).color(color)).truncate());
        });
    });
}

/// Panel header title: bright enough to lead, quiet enough not to shout.
fn header_fg(theme: &Theme) -> Color32 {
    theme
        .sidebar
        .foreground
        .lerp_to_gamma(theme.sidebar.background, 0.25)
}

/// Secondary text (section labels, hints) derived from the sidebar foreground.
fn subtle_fg(theme: &Theme) -> Color32 {
    theme
        .sidebar
        .foreground
        .lerp_to_gamma(theme.sidebar.background, 0.45)
}

/// Resting activity-bar icon color; dimmed so active and hover states stand out.
fn icon_fg(theme: &Theme) -> Color32 {
    theme
        .sidebar
        .foreground
        .lerp_to_gamma(theme.titlebar.background, 0.2)
}

/// Selected-row fill: the theme's active color softened toward the panel background.
fn selected_bg(theme: &Theme) -> Color32 {
    theme
        .sidebar
        .background
        .lerp_to_gamma(theme.sidebar.active_bg, 0.72)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    /// Glyph scale comes from a square design grid, so icons never distort.
    #[test]
    fn icon_scale_is_uniform() {
        for side in [32.0_f32, 48.0, 58.0, 96.0] {
            // Mirrors paint_icon(): px is the square glyph box, one factor both axes.
            let px = side.min(side) * ICON_FILL;
            let scale = px / GRID;
            // Same factor drives x and y — that is what 1:1 means here.
            assert_eq!(
                GRID * scale,
                px,
                "grid must map onto the glyph box for side {side}"
            );
            assert!(px <= side, "glyph {px} must fit inside button {side}");
            assert!(scale > 0.0);
        }
    }

    /// A non-square button still yields a square glyph, centered on the button.
    #[test]
    fn icon_glyph_stays_square_in_wide_rect() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 48.0));
        let px = rect.width().min(rect.height()) * ICON_FILL;
        let origin = rect.center() - Vec2::splat(px / 2.0);
        let corner = origin;
        let far = origin + Vec2::splat(px);
        assert_eq!(far.x - corner.x, far.y - corner.y, "glyph must be square");
        assert!((corner.x + px / 2.0 - rect.center().x).abs() < 1e-6);
    }

    /// Activity-bar rows must not overlap: each slot starts one ICON_BUTTON lower.
    #[test]
    fn slots_do_not_overlap() {
        let mut y = 0.0;
        for _ in SidebarView::all() {
            let slot = slot_rect(y, ACTIVITY_BAR_WIDTH);
            assert_eq!(slot.width(), ACTIVITY_BAR_WIDTH);
            assert_eq!(slot.height(), ICON_BUTTON, "row must be one button tall");
            y += ICON_BUTTON;
        }
    }

    /// The rail stays 48px wide and every row keeps the same height, so glyphs
    /// land on an even grid instead of drifting when the panel is resized.
    #[test]
    fn rail_rows_keep_a_fixed_height() {
        for y in [0.0, 48.0, 96.0] {
            let slot = slot_rect(y, ACTIVITY_BAR_WIDTH);
            assert_eq!(slot.height(), ICON_BUTTON);
            assert_eq!(slot.left(), 0.0);
        }
    }

    /// Clicking the active view collapses the panel; another view replaces it.
    #[test]
    fn toggle_collapses_then_switches() {
        let mut sb = Sidebar::default();
        assert!(sb.open);
        assert!(sb.is_active(SidebarView::Explorer));

        sb.toggle(SidebarView::Explorer);
        assert!(!sb.open, "second click on active view collapses");

        sb.toggle(SidebarView::Graph);
        assert!(sb.open);
        assert!(sb.is_active(SidebarView::Graph));
        assert!(!sb.is_active(SidebarView::Explorer));
    }

    /// Every view needs a distinct label for tooltips and panel headers.
    #[test]
    fn view_labels_are_unique() {
        let mut labels: Vec<&str> = SidebarView::all().iter().map(|v| v.label()).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "duplicate view label");
    }

    /// Same contract on the right rail: distinct labels, one glyph each.
    #[test]
    fn right_view_labels_and_icons_are_unique() {
        let mut labels: Vec<&str> = RightView::all().iter().map(|v| v.label()).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "duplicate right view label");

        let mut icons: Vec<RailIcon> = RightView::all().iter().map(|v| v.icon()).collect();
        let total = icons.len();
        icons.sort_unstable();
        icons.dedup();
        assert_eq!(icons.len(), total, "duplicate right view glyph");
    }

    /// The right rail collapses on second click and switches like the left one,
    /// but starts closed: only the icons are shown until the user asks for a panel.
    #[test]
    fn right_toggle_collapses_then_switches() {
        let mut sb = RightSidebar::default();
        assert!(!sb.open, "right panel must not steal space on first launch");

        sb.toggle(RightView::Agent);
        assert!(sb.open);
        assert!(sb.is_open(RightView::Agent));

        sb.toggle(RightView::Agent);
        assert!(!sb.open, "second click on active view collapses");

        sb.toggle(RightView::Preview);
        assert!(sb.open);
        assert!(sb.is_open(RightView::Preview));
        assert!(!sb.is_open(RightView::Agent));
    }

    /// A checkbox on an inactive view must not steal the panel from the active one.
    #[test]
    fn set_open_leaves_inactive_view_alone() {
        let mut sb = RightSidebar::default();
        sb.toggle(RightView::Agent);

        sb.set_open(RightView::Preview, false);
        assert!(
            sb.is_open(RightView::Agent),
            "hiding another view is a no-op"
        );

        sb.set_open(RightView::Preview, true);
        assert!(
            sb.is_open(RightView::Preview),
            "opening a view switches the panel"
        );

        sb.set_open(RightView::Preview, false);
        assert!(!sb.open, "unchecking the active view closes the panel");
    }

    /// The active-row stripe must sit on the rail edge the panel is attached to.
    #[test]
    fn accent_stripe_follows_the_rail_side() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 10.0), Vec2::new(48.0, ICON_BUTTON));

        let left = accent_stripe(rect, RailSide::Left);
        assert_eq!(left.left(), rect.left(), "left rail marks its left edge");
        assert_eq!(left.width(), ACCENT_WIDTH);

        let right = accent_stripe(rect, RailSide::Right);
        assert_eq!(
            right.right(),
            rect.right(),
            "right rail marks its right edge"
        );
        assert_eq!(right.width(), ACCENT_WIDTH);
        assert_eq!(left.height(), right.height(), "stripe spans the full row");
    }

    /// Pinned rows sink to the bottom without colliding with the stacked ones.
    #[test]
    fn pinned_rows_sink_below_the_stack() {
        let rail_h = 400.0;
        let stacked: f32 = SidebarView::all().iter().filter(|v| !v.pinned()).count() as f32;
        let pinned: f32 = SidebarView::all().iter().filter(|v| v.pinned()).count() as f32;
        let pinned_y = (rail_h - ICON_BUTTON * pinned).max(stacked * ICON_BUTTON);

        assert!(
            pinned_y >= stacked * ICON_BUTTON,
            "pinned must clear the stack"
        );
        assert_eq!(
            pinned_y + ICON_BUTTON,
            rail_h,
            "a single pinned row hugs the rail's end"
        );
        // The right rail pins nothing, so it is a plain top-down stack.
        assert!(RightView::all().iter().all(|v| !v.pinned()));
    }
}
