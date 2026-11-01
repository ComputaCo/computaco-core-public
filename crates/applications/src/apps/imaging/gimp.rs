//! GIMP 2.10 in single-window mode with its default dark theme: the menu bar, the image
//! tab, the toolbox with FG/BG colours and Tool Options on the left, rulers around the
//! canvas, the Layers / Undo dock on the right and the status bar.
use super::gimp_menus::{Entry, MENUS as REAL};
use super::view::{self, Item, Skin};
use super::{Panel, Studio, Tool};
use crate::desktop_scene::{shared::Align, Painter};
use cw_raster::mask::SelectMode;
use cw_scene::{Color, Rect};
use std::collections::BTreeMap;

fn skin() -> Skin {
    Skin {
        bg: Color::rgb(69, 69, 69),
        bar: Color::rgb(60, 60, 60),
        panel: Color::rgb(56, 56, 56),
        text: Color::rgb(221, 221, 221),
        muted: Color::rgb(160, 160, 160),
        accent: Color::rgb(116, 153, 204),
        on_accent: Color::WHITE,
        selected: Color::rgb(94, 94, 94),
        line: Color(0, 0, 0, 90),
        backdrop: Color::rgb(51, 51, 51),
        radius: 2,
        font: 12,
    }
}

const MENU_IDS: [&str; 11] = [
    "file", "edit", "select", "view", "image", "layer", "colors", "tools", "filters", "windows",
    "help",
];

const MENUS: [(&str, &str); 11] = [
    ("file", "File"),
    ("edit", "Edit"),
    ("select", "Select"),
    ("view", "View"),
    ("image", "Image"),
    ("layer", "Layer"),
    ("colors", "Colors"),
    ("tools", "Tools"),
    ("filters", "Filters"),
    ("windows", "Windows"),
    ("help", "Help"),
];

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Move => "Move",
        Tool::RectSelect => "Rectangle Select",
        Tool::EllipseSelect => "Ellipse Select",
        Tool::Lasso => "Free Select",
        Tool::MagicWand => "Fuzzy Select",
        Tool::Crop => "Crop",
        Tool::Text => "Text",
        Tool::Fill => "Bucket Fill",
        Tool::Brush => "Paintbrush",
        Tool::Pencil => "Pencil",
        Tool::Airbrush => "Airbrush",
        Tool::Eraser => "Eraser",
        Tool::Picker => "Color Picker",
        Tool::Zoom => "Zoom",
        Tool::Pan => "Pan",
        Tool::Gradient => "Gradient",
        Tool::Clone => "Clone",
        Tool::Heal => "Heal",
        Tool::Paths => "Paths",
        _ => "Tool",
    }
}

pub fn render(st: &Studio, p: &mut Painter, env: &crate::AppEnv<'_>) {
    let s = skin();
    let (w, h) = (env.width, env.height);
    p.scene.background = s.bg;
    let t = |c: &str| st.target(c);

    // Menu bar: a menu bar of menus, as GTK announces GIMP's.
    p.box_(Rect::new(0, 0, w, 27), s.bar, 0);
    p.region(Rect::new(0, 0, w, 27), &t("noop:menubar"), "");
    announce(p, "menubar", None);
    let mut x = 2;
    for (id, label) in MENUS {
        // GTK2's menu titles are set wider than this renderer's text (File 36, Select 56, Windows
        // 74 px on the real application).
        let bw = p.measure(label, 12, false) * 5 / 4 + 12;
        let r = Rect::new(x, 0, bw, 27);
        let open =
            matches!(&st.panel, Some(Panel::Menu { id: m }) if m.split('/').next() == Some(id));
        p.button(
            r,
            if open { s.selected } else { Color::TRANSPARENT },
            2,
            &t(&format!("menu:{id}")),
            label,
        );
        announce(p, "menu", open.then_some(true));
        p.label(x, 7, bw, label, 12, s.text, false, Align::Center);
        x += bw as i32;
    }
    p.hline(0, 27, w, s.line);

    // GIMP 2.10's single-window layout: the docks run the window's height, and the image
    // window's status bar lies under the image only (measured on the real application).
    let left = 188u32.min(w / 4);
    let right = 203u32.min(w / 4);
    let status_h = 30;
    let top = 27;
    let body_h = h.saturating_sub(top as u32);
    let cbody = body_h.saturating_sub(status_h);

    // Toolbox: the sixteen buttons GIMP groups its tools in, six to a row, then the colours.
    let mark_toolbox = p.scene.nodes.len();
    p.box_(Rect::new(0, top, left, body_h), s.panel, 0);
    p.vline(left as i32, top, body_h, s.line);
    let slots: [Option<Tool>; 16] = [
        Some(Tool::Move),
        Some(Tool::RectSelect),
        Some(Tool::Lasso),
        Some(Tool::MagicWand),
        Some(Tool::Crop),
        None,
        None,
        Some(Tool::Fill),
        Some(Tool::Brush),
        Some(Tool::Eraser),
        Some(Tool::Clone),
        None,
        Some(Tool::Paths),
        Some(Tool::Text),
        Some(Tool::Picker),
        Some(Tool::Zoom),
    ];
    let available = st.product.tools();
    for (i, slot) in slots.iter().enumerate() {
        let r = Rect::new((i as i32 % 6) * 31, top + 25 + (i as i32 / 6) * 28, 31, 28);
        match slot {
            Some(tool) if available.contains(tool) => view::tool(
                p,
                &s,
                r,
                tool.symbol(),
                tool_name(*tool),
                &t(&format!("tool:{}", tool.id())),
                st.tool == *tool,
            ),
            _ => view::tool(p, &s, r, "gear", "", &t(&format!("noop:slot:{i}")), false),
        }
    }
    // FG/BG colour squares with swap and reset, as in the toolbox.
    let sw_y = top + 115;
    let fg = Rect::new(78, sw_y, 24, 24);
    let bg = Rect::new(90, sw_y + 12, 24, 24);
    view::swatch(p, &s, bg, st.secondary, &t("dialog:color"), false, false);
    p.z += 1;
    view::swatch(p, &s, fg, st.primary, &t("dialog:color"), false, false);
    p.z -= 1;
    view::tool(
        p,
        &s,
        Rect::new(120, sw_y, 16, 16),
        "sort",
        "Swap colors",
        &t("swap-colors"),
        false,
    );
    view::tool(
        p,
        &s,
        Rect::new(120, sw_y + 20, 16, 16),
        "reload",
        "Default colors",
        &t("reset-colors"),
        false,
    );
    quiet(p, st, mark_toolbox, &[]);
    container(
        p,
        Rect::new(0, top + 157, left, body_h.saturating_sub(157)),
        &t("noop:tablist:options"),
        "tablist",
    );
    let mark_options = p.scene.nodes.len();
    // Tool Options.
    let oy = top + 157;
    p.hline(0, oy, left, s.line);
    p.strong(8, oy + 6, left - 16, "Tool Options", 12, s.text);
    p.label(
        8,
        oy + 26,
        left - 16,
        tool_name(st.tool),
        12,
        s.accent,
        true,
        Align::Left,
    );
    let ow = left - 16;
    let mut y = oy + 48;
    if st.tool.paints() {
        let mode = st.brush().blend;
        p.label(
            8,
            y,
            ow,
            &format!("Mode: {}", mode.label()),
            11,
            s.muted,
            false,
            Align::Left,
        );
        y += 20;
        view::slider(
            p,
            &s,
            Rect::new(8, y, ow, 34),
            st,
            "Opacity",
            "opacity",
            true,
        );
        y += 40;
        view::slider(p, &s, Rect::new(8, y, ow, 34), st, "Size", "size", true);
        y += 40;
        if st.tool != Tool::Pencil {
            view::slider(
                p,
                &s,
                Rect::new(8, y, ow, 34),
                st,
                "Hardness",
                "hardness",
                true,
            );
        }
    } else if matches!(
        st.tool,
        Tool::RectSelect | Tool::EllipseSelect | Tool::Lasso | Tool::MagicWand
    ) {
        p.label(8, y, ow, "Mode:", 11, s.muted, false, Align::Left);
        y += 18;
        for (i, mode) in SelectMode::ALL.iter().enumerate() {
            let label = match mode {
                SelectMode::Replace => "Replace",
                SelectMode::Add => "Add",
                SelectMode::Subtract => "Subtract",
                SelectMode::Intersect => "Intersect",
            };
            view::chip(
                p,
                &s,
                Rect::new(
                    8 + (i as i32 % 2) * (ow as i32 / 2),
                    y + (i as i32 / 2) * 26,
                    ow / 2 - 4,
                    22,
                ),
                label,
                &t(&format!("mode:{}", mode.id())),
                st.select_mode == *mode,
            );
        }
        y += 58;
        if st.tool == Tool::MagicWand {
            view::slider(
                p,
                &s,
                Rect::new(8, y, ow, 34),
                st,
                "Threshold",
                "tolerance",
                true,
            );
            y += 40;
            view::chip(
                p,
                &s,
                Rect::new(8, y, ow, 22),
                "Sample merged",
                &t("merged"),
                st.merged,
            );
        }
    } else if st.tool == Tool::Fill {
        view::slider(
            p,
            &s,
            Rect::new(8, y, ow, 34),
            st,
            "Threshold",
            "tolerance",
            true,
        );
        y += 40;
        view::chip(
            p,
            &s,
            Rect::new(8, y, ow, 22),
            "Sample merged",
            &t("merged"),
            st.merged,
        );
    } else if st.tool == Tool::Text {
        view::slider(
            p,
            &s,
            Rect::new(8, y, ow, 34),
            st,
            "Size",
            "font-size",
            true,
        );
        y += 40;
        view::chip(p, &s, Rect::new(8, y, ow, 22), "Bold", &t("bold"), st.bold);
        y += 28;
        p.paragraph(
            8,
            y,
            ow,
            "Click the image and type; Enter commits.",
            11,
            s.muted,
        );
    } else if st.tool == Tool::Crop {
        match st.crop {
            Some(r) => {
                p.label(
                    8,
                    y,
                    ow,
                    &format!("{} × {} at {}, {}", r.w, r.h, r.x, r.y),
                    11,
                    s.text,
                    false,
                    Align::Left,
                );
                view::button(
                    p,
                    &s,
                    Rect::new(8, y + 22, ow, 26),
                    "Crop",
                    &t("crop-apply"),
                    true,
                );
            }
            None => {
                p.paragraph(8, y, ow, "Drag a rectangle over the image.", 11, s.muted);
            }
        }
    } else if st.tool == Tool::Picker {
        view::chip(
            p,
            &s,
            Rect::new(8, y, ow, 22),
            "Sample merged",
            &t("merged"),
            st.merged,
        );
    } else if matches!(st.tool, Tool::Clone | Tool::Heal) {
        for (label, param, y_off) in [
            ("Opacity", "opacity", 0),
            ("Size", "size", 40),
            ("Hardness", "hardness", 80),
        ] {
            view::slider(
                p,
                &s,
                Rect::new(8, y + y_off, ow, 34),
                st,
                label,
                param,
                true,
            );
        }
        y += 122;
        view::chip(
            p,
            &s,
            Rect::new(8, y, ow, 22),
            "Sample merged",
            &t("merged"),
            st.merged,
        );
        y += 30;
        p.label(8, y, ow, "Alignment:", 11, s.muted, false, Align::Left);
        y += 18;
        for (i, (label, aligned)) in [("None", false), ("Aligned", true)].iter().enumerate() {
            view::chip(
                p,
                &s,
                Rect::new(8 + i as i32 * (ow as i32 / 2), y, ow / 2 - 4, 22),
                label,
                &t(if *aligned {
                    "aligned:on"
                } else {
                    "aligned:off"
                }),
                st.retouch.aligned == *aligned,
            );
        }
        y += 32;
        let note = match st.retouch.source {
            Some((sx, sy)) => format!("Source: {sx}, {sy}. Ctrl-click to set another."),
            None => "Ctrl-click the image to set the source.".to_owned(),
        };
        p.paragraph(8, y, ow, &note, 11, s.muted);
    } else if st.tool == Tool::Gradient {
        view::slider(
            p,
            &s,
            Rect::new(8, y, ow, 34),
            st,
            "Opacity",
            "opacity",
            true,
        );
        y += 42;
        for (i, (label, command, on)) in [
            (
                "FG to BG (RGB)",
                "gradient-colors:fg-bg",
                !st.gradient.transparent,
            ),
            (
                "FG to Transparent",
                "gradient-colors:fg-transparent",
                st.gradient.transparent,
            ),
            ("Reverse", "gradient-reverse", st.gradient.reverse),
        ]
        .iter()
        .enumerate()
        {
            view::chip(
                p,
                &s,
                Rect::new(8, y + i as i32 * 26, ow, 22),
                label,
                &t(command),
                *on,
            );
        }
        y += 84;
        p.label(8, y, ow, "Shape:", 11, s.muted, false, Align::Left);
        y += 18;
        use cw_raster::gradient::{GradientShape as G, Repeat as R};
        for (i, (label, shape)) in [
            ("Linear", G::Linear),
            ("Bi-linear", G::BiLinear),
            ("Radial", G::Radial),
            ("Square", G::Square),
            ("Conical (sym)", G::ConicalSymmetric),
            ("Conical (asym)", G::ConicalAsymmetric),
        ]
        .iter()
        .enumerate()
        {
            view::chip(
                p,
                &s,
                Rect::new(
                    8 + (i as i32 % 2) * (ow as i32 / 2),
                    y + (i as i32 / 2) * 26,
                    ow / 2 - 4,
                    22,
                ),
                label,
                &t(&format!("gradient-shape:{}", shape.id())),
                st.gradient.shape == *shape,
            );
        }
        y += 84;
        p.label(8, y, ow, "Repeat:", 11, s.muted, false, Align::Left);
        y += 18;
        for (i, (label, repeat)) in [
            ("None (extend)", R::None),
            ("Sawtooth wave", R::Sawtooth),
            ("Triangular wave", R::Triangular),
            ("Truncate", R::Truncate),
        ]
        .iter()
        .enumerate()
        {
            view::chip(
                p,
                &s,
                Rect::new(
                    8 + (i as i32 % 2) * (ow as i32 / 2),
                    y + (i as i32 / 2) * 26,
                    ow / 2 - 4,
                    22,
                ),
                label,
                &t(&format!("gradient-repeat:{}", repeat.id())),
                st.gradient.repeat == *repeat,
            );
        }
    } else if st.tool == Tool::Paths {
        p.paragraph(
            8,
            y,
            ow,
            "Design: click to add anchors, drag to pull handles; Ctrl-click the first anchor to close.",
            11,
            s.muted,
        );
        y += 60;
        path_buttons(st, p, &s, 8, y, ow);
    }

    quiet(p, st, mark_options, &[]);
    // The tool options dock is a tab list: its tab, and what the active tab holds.
    p.button(
        Rect::new(70, top + 162, 18, 18),
        Color::TRANSPARENT,
        0,
        &t("noop:tab:options"),
        "",
    );
    announce(p, "tab", None);

    // Image tab, rulers, canvas.
    let cx = left as i32 + 1;
    let cw = w.saturating_sub(left + right + 2);
    container(
        p,
        Rect::new(cx, top, cw, body_h),
        &t("noop:tablist:image"),
        "tablist",
    );
    let mark_center = p.scene.nodes.len();
    let tab_h: i32 = if st.doc.is_some() { 30 } else { 0 };
    if st.doc.is_some() {
        p.box_(Rect::new(cx, top, cw, tab_h as u32), s.bar, 0);
        let tab = Rect::new(cx + 4, top + 3, 150.min(cw.saturating_sub(8)), 26);
        p.box_(tab, s.bg, 2);
        p.label(
            tab.x + 8,
            tab.y + 6,
            tab.width - 16,
            &st.document_name(),
            12,
            s.text,
            false,
            Align::Left,
        );
    }
    let ruler: i32 = 18;
    let area = Rect::new(
        cx + ruler,
        top + tab_h + ruler,
        cw.saturating_sub(ruler as u32),
        cbody.saturating_sub((tab_h + ruler) as u32),
    );
    if st.doc.is_some() {
        p.box_(Rect::new(cx, top + tab_h, cw, ruler as u32), s.panel, 0);
        p.box_(
            Rect::new(
                cx,
                top + tab_h,
                ruler as u32,
                cbody.saturating_sub(tab_h as u32),
            ),
            s.panel,
            0,
        );
        // Ruler ticks every 100 image pixels.
        let z = st.effective_zoom(area.width, area.height).max(1) as i32;
        let (ox, oy) = st.origin(area.width, area.height);
        let step = (100 * z / 100).max(8);
        let mut k = 0;
        while ox + k * step < area.width as i32 {
            let tx = area.x + ox + k * step;
            if tx >= area.x {
                p.vline(tx, top + tab_h + 8, 10, s.muted);
                p.left(
                    tx + 2,
                    top + tab_h + 1,
                    40,
                    &(k * 100).to_string(),
                    9,
                    s.muted,
                );
            }
            k += 1;
        }
        let mut k = 0;
        while oy + k * step < area.height as i32 {
            let ty = area.y + oy + k * step;
            if ty >= area.y {
                p.hline(cx + 8, ty, 10, s.muted);
            }
            k += 1;
        }
        view::canvas(p, &s, area, st, false, env.pointer);
        view::scrollbars(p, &s, area, st);
    } else {
        let empty = Rect::new(cx, top, cw, cbody);
        p.box_(empty, s.backdrop, 0);
        p.symbol(
            "image",
            cx + cw as i32 / 2 - 40,
            top + cbody as i32 / 2 - 70,
            80,
            Color(255, 255, 255, 40),
        );
        p.center(
            cx,
            top + cbody as i32 / 2 + 20,
            cw,
            "No image open",
            14,
            s.muted,
        );
        view::button(
            p,
            &s,
            Rect::new(
                cx + cw as i32 / 2 - 110,
                top + cbody as i32 / 2 + 48,
                104,
                28,
            ),
            "New…",
            &t("dialog:new-image"),
            false,
        );
        view::button(
            p,
            &s,
            Rect::new(cx + cw as i32 / 2 + 6, top + cbody as i32 / 2 + 48, 104, 28),
            "Open…",
            &t("open"),
            true,
        );
    }

    quiet(p, st, mark_center, &[]);
    // The image window is a tab list holding the image, its scroll bars and its status bar.

    // Right dock: Layers or Undo History.
    let rx = w as i32 - right as i32;
    p.box_(Rect::new(rx, top, right, body_h), s.panel, 0);
    p.vline(rx, top, body_h, s.line);
    let mark_brushes = p.scene.nodes.len();
    brushes_dock(p, &s, st, rx, top, right);
    quiet(p, st, mark_brushes, &["2. Hardness 050 (51 × 51)"]);
    // The layers dock lies under the brushes dock, a tab list of its own.
    let layers_top = top + 315;
    let (top, body_h) = (layers_top, body_h.saturating_sub(315));
    let mark_layers = p.scene.nodes.len();
    container(
        p,
        Rect::new(rx, top, right, body_h),
        &t("noop:tablist:layers"),
        "tablist",
    );
    let undo_tab = st.tab == "undo";
    let paths_tab = st.tab == "paths";
    let mut tx = rx + 4;
    for (id, label, live) in [
        ("layers", "Layers", true),
        ("channels", "Channels", false),
        ("paths", "Paths", true),
        ("undo", "Undo", true),
    ] {
        let bw = p.measure(label, 11, false) + 14;
        let r = Rect::new(tx, top + 4, bw, 22);
        let on = match id {
            "undo" => undo_tab,
            "paths" => paths_tab,
            _ => !undo_tab && !paths_tab,
        };
        if live {
            view::chip(p, &s, r, label, &t(&format!("tab:{id}")), on);
        } else {
            p.box_(r, Color::TRANSPARENT, 2);
            p.disabled("Channels are not edited in this GIMP");
            p.label(
                r.x,
                r.y + 4,
                r.width,
                label,
                11,
                s.muted,
                false,
                Align::Center,
            );
        }
        tx += bw as i32 + 2;
    }
    let dy = top + 32;
    if let Some(doc) = &st.doc {
        if paths_tab {
            let mut y = dy + 4;
            match &st.path_edit {
                Some(path) => {
                    p.box_(Rect::new(rx + 4, y, right - 8, 26), s.selected, 2);
                    p.symbol("edit", rx + 10, y + 5, 16, s.text);
                    p.label(
                        rx + 32,
                        y + 6,
                        right - 40,
                        &format!(
                            "Unnamed ({} anchors{})",
                            path.anchors.len(),
                            if path.closed { ", closed" } else { "" }
                        ),
                        12,
                        s.text,
                        false,
                        Align::Left,
                    );
                    y += 34;
                }
                None => {
                    p.paragraph(
                        rx + 8,
                        y,
                        right - 16,
                        "No paths. Draw one with the Paths tool.",
                        11,
                        s.muted,
                    );
                    y += 40;
                }
            }
            path_buttons(st, p, &s, rx + 8, y, right - 16);
        } else if undo_tab {
            let mut y = dy + 4;
            for (i, step) in std::iter::once("[ Base Image ]")
                .chain(doc.undo_steps())
                .enumerate()
            {
                if y + 22 > top + body_h as i32 {
                    break;
                }
                let current = i == doc.undo_steps().len();
                if current {
                    p.box_(Rect::new(rx + 4, y, right - 8, 22), s.selected, 2);
                }
                p.label(
                    rx + 12,
                    y + 4,
                    right - 24,
                    step,
                    12,
                    s.text,
                    current,
                    Align::Left,
                );
                y += 22;
            }
        } else {
            let mode = doc.active_layer().blend;
            // GTK2's layers dock: the mode and paint-mode combo boxes, the opacity spin
            // button, the lock row, the layer list and the row of buttons under it.
            view::chip(
                p,
                &s,
                Rect::new(rx + 3, top + 27, 150, 28),
                &format!("{} ▾", gimp_mode(mode)),
                &t("menu:blend"),
                false,
            );
            p.button(
                Rect::new(rx + 154, top + 27, 46, 28),
                Color::TRANSPARENT,
                2,
                &t("noop:paintmode"),
                "Default",
            );
            announce(p, "combobox", None);
            view::slider(
                p,
                &s,
                Rect::new(rx + 3, top + 56, right - 6, 18),
                st,
                "",
                "layer-opacity",
                true,
            );
            p.label(
                rx + 4,
                top + 79,
                30,
                "Lock:",
                11,
                s.muted,
                false,
                Align::Left,
            );
            for (i, x) in [34, 61, 88].iter().enumerate() {
                p.button(
                    Rect::new(rx + x, top + 75, 26, 26),
                    Color::TRANSPARENT,
                    2,
                    &t(&format!("noop:lock:{i}")),
                    "",
                );
            }
            view::layer_rows(p, &s, Rect::new(rx + 4, top + 102, right - 8, 100), st, 40);
            let by = top + 218;
            let count = doc.layers().len();
            let active = doc.active();
            let mut bx = rx + 3;
            for (symbol, command, label, live, why) in [
                ("new-tab", "layer:new", "Create a new layer", true, ""),
                (
                    "chevron-up",
                    "layer:up",
                    "Raise this layer",
                    active + 1 < count,
                    "The layer is already at the top",
                ),
                (
                    "chevron-down",
                    "layer:down",
                    "Lower this layer",
                    active > 0,
                    "The layer is already at the bottom",
                ),
                ("copy", "layer:duplicate", "Duplicate this layer", true, ""),
                (
                    "layers",
                    "layer:merge",
                    "Merge this layer with the one below",
                    active > 0,
                    "There is no layer below",
                ),
                (
                    "trash",
                    "layer:delete",
                    "Delete this layer",
                    count > 1,
                    "An image keeps at least one layer",
                ),
            ] {
                let r = Rect::new(bx, by, 24, 26);
                if live {
                    view::tool(p, &s, r, symbol, label, &t(command), false);
                } else {
                    view::inert_tool(p, &s, r, symbol, why);
                }
                bx += 24;
            }
            for i in 0..2 {
                p.button(
                    Rect::new(bx, by, if i == 1 { 29 } else { 24 }, 26),
                    Color::TRANSPARENT,
                    2,
                    &t(&format!("noop:layerbtn:{i}")),
                    "",
                );
                bx += 24;
            }
        }
    }

    quiet(p, st, mark_layers, &["Lock:"]);
    // Status bar: under the image only, a bar holding the unit and zoom choices and the message.
    let top = 27;
    let sy = top + cbody as i32;
    let sx = cx;
    p.box_(Rect::new(sx, sy, cw, status_h), s.bar, 0);
    p.hline(sx, sy, cw, s.line);
    container(
        p,
        Rect::new(sx, sy, cw, status_h),
        &t("noop:statusbar"),
        "statusbar",
    );
    let mark_status = p.scene.nodes.len();
    if st.doc.is_some() {
        // The pointer's image coordinates, blank while it is off the canvas.
        let coords = st
            .pointer_pixel(area, env.pointer)
            .map(|(x, y)| format!("{x}, {y}"))
            .unwrap_or_default();
        p.label(sx + 4, sy + 8, 70, &coords, 11, s.text, false, Align::Right);
        p.button(
            Rect::new(sx + 100, sy + 1, 53, 28),
            Color::TRANSPARENT,
            2,
            &t("noop:units"),
            "pixels",
        );
        announce(p, "combobox", None);
        p.left(sx + 106, sy + 8, 40, "px", 11, s.text);
        let z = st.effective_zoom(area.width, area.height);
        view::chip(
            p,
            &s,
            Rect::new(sx + 154, sy + 1, 59, 28),
            &format!("{z}% ▾"),
            &t("menu:view/zoom"),
            false,
        );
        let msg = st
            .status
            .clone()
            .unwrap_or_else(|| format!("{} ({})", st.document_name(), view::size_text(st)));
        p.left(sx + 220, sy + 8, cw.saturating_sub(230), &msg, 11, s.text);
    } else {
        view::status(p, &s, sx + 8, sy + 8, cw.saturating_sub(16), st);
    }
    quiet(p, st, mark_status, &[]);

    let home_name = env
        .files
        .home
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    menus(st, p, &s);
    if matches!(&st.panel, Some(Panel::Dialog { id, .. }) if id == "jpeg") {
        if let Some(sheet) = &st.sheet {
            super::gimp_dialogs::chooser(p, &s, st, sheet, home_name, w, h);
        }
        super::gimp_dialogs::jpeg(p, &s, st, w, h);
    } else if matches!(&st.panel, Some(Panel::Dialog { id, .. }) if id == "resize") {
        super::gimp_dialogs::scale(p, &s, st, w, h);
    } else {
        view::dialog(p, &s, st, w, h);
        if let Some(sheet) = &st.panel {
            super::gimp_dialogs::chooser(p, &s, st, sheet, home_name, w, h);
        }
    }
}

/// A container GTK2 announces (a tab list, a status bar): a region under the controls it holds,
/// so that it is in the tree but a click reaches what is on it.
fn container(p: &mut Painter, r: Rect, id: &str, role: &str) {
    p.region(r, id, "");
    announce(p, role, None);
}

/// Take the names GTK2 does not announce off the controls painted since `from`: its tool
/// buttons, sliders, tabs and docks are unnamed in the accessibility tree, and the text beside
/// them is laid out, not labelled. A slider is a spin button, a tab a tab, a row of a list a
/// cell. Interaction ids are untouched; the painted texts in `named` stay labels.
fn quiet(p: &mut Painter, st: &Studio, from: usize, named: &[&str]) {
    for n in &mut p.scene.nodes[from..] {
        let text = n.painted_text().map(str::to_owned);
        let action = n.interaction.clone().unwrap_or_default();
        match &mut n.semantic {
            Some(sem) if sem.role == "button" && action.contains(":canvas") => {
                // The drawing area is an unnamed panel in GTK2's tree, which the comparison
                // splices out; it stays a control that takes the pointer.
                sem.role = "panel".into();
                sem.label.clear();
            }
            Some(sem)
                if sem.role == "button"
                    && (action.contains(":hscroll") || action.contains(":vscroll")) =>
            {
                sem.role = "scrollbar".into();
                sem.label.clear();
            }
            Some(sem)
                if sem.role == "button"
                    && (action.ends_with("swap-colors")
                        || action.ends_with("reset-colors")
                        || action.ends_with("dialog:color")) =>
            {
                // The colour area is unnamed panels in GTK2's tree.
                sem.role = "panel".into();
                sem.label.clear();
            }
            Some(sem) if sem.role == "button" => {
                let label = std::mem::take(&mut sem.label);
                match label.as_str() {
                    l if l.ends_with(" slider") => {
                        sem.role = "spinbutton".into();
                        // `...:slider:<param>:<width>`: its value is the parameter's.
                        if let Some(param) = action
                            .split(":slider:")
                            .nth(1)
                            .and_then(|r| r.split(':').next())
                        {
                            sem.value = Some(st.param(param).to_string());
                        }
                    }
                    // The layers dock's tab icons are not in GTK2's tree.
                    _ if action.contains(":tab:") => sem.role = "presentation".into(),
                    "Normal ▾" => {
                        sem.role = "combobox".into();
                        sem.label = "Normal".into();
                    }
                    l if l.ends_with("% ▾") => {
                        sem.role = "combobox".into();
                        sem.label = l.trim_end_matches(" ▾").replace('%', "\u{2009}%");
                    }
                    "pixels" => sem.label = label,
                    _ => {}
                }
            }
            Some(_) => {}
            None => {
                if let Some(text) = text {
                    n.semantic = Some(if named.contains(&text.as_str()) {
                        cw_scene::Semantic {
                            role: "label".into(),
                            label: text,
                            value: None,
                            disabled: false,
                            focusable: false,
                        }
                    } else {
                        cw_scene::Semantic {
                            role: "presentation".into(),
                            label: String::new(),
                            value: None,
                            disabled: false,
                            focusable: false,
                        }
                    });
                }
            }
        }
    }
}

/// The brushes dock above the layers: a filter field, the brush's name, the brushes, and the
/// buttons under them. Nothing in it is wired to a command (this GIMP paints with its own
/// brush settings), so each control is an inert node of the kind GTK announces.
fn brushes_dock(p: &mut Painter, s: &Skin, st: &Studio, rx: i32, top: i32, width: u32) {
    let ctl = |p: &mut Painter, r: Rect, role: &str, id: &str| {
        p.border(r, Color::TRANSPARENT, 2, s.line);
        p.button(
            r,
            Color::TRANSPARENT,
            2,
            &st.target(&format!("noop:brushes:{id}")),
            "",
        );
        announce(p, role, None);
    };
    container(
        p,
        Rect::new(rx, top, width, 313),
        &st.target("noop:tablist:brushes"),
        "tablist",
    );
    for (i, x) in [4, 28, 52, 76].iter().enumerate() {
        ctl(
            p,
            Rect::new(rx + x, top + 4, 18, 18),
            "tab",
            &format!("tab{i}"),
        );
        if i == 0 {
            // The first tab is the brushes tab, named by the brush shown.
            if let Some(sem) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
                sem.label = "2. Hardness 050 (51 × 51)".into();
            }
        }
    }
    ctl(
        p,
        Rect::new(rx + 3, top + 27, width - 6, 22),
        "textbox",
        "filter",
    );
    p.label(
        rx + 6,
        top + 51,
        width - 12,
        "2. Hardness 050 (51 × 51)",
        11,
        s.text,
        false,
        Align::Left,
    );
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(cw_scene::Semantic {
            role: "label".into(),
            label: "2. Hardness 050 (51 × 51)".into(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
    let grid = Rect::new(rx + 4, top + 64, width - 22, 181);
    p.border(grid, s.bg, 0, s.line);
    p.region(
        Rect::new(grid.x + grid.width as i32 + 1, grid.y, 15, 181),
        &st.target("noop:brushes:scroll"),
        "",
    );
    announce(p, "scrollbar", None);
    ctl(
        p,
        Rect::new(rx + 3, top + 245, width - 6, 22),
        "textbox",
        "tags",
    );
    ctl(
        p,
        Rect::new(rx + 3, top + 267, width - 6, 18),
        "spinbutton",
        "spacing",
    );
    if let Some(sem) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        sem.value = Some("10".into());
    }
    for i in 0..6 {
        let r = Rect::new(rx + 3 + i * 32, top + 285, if i == 5 { 37 } else { 32 }, 26);
        ctl(p, r, "button", &format!("b{i}"));
        // The fourth and sixth are insensitive until a brush is chosen.
        if i == 3 || i == 5 {
            if let Some(sem) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
                sem.disabled = true;
            }
        }
    }
}

/// The Paths dialog's actions, live when there is a path to act on.
fn path_buttons(st: &Studio, p: &mut Painter, s: &Skin, x: i32, mut y: i32, w: u32) {
    let ready = st.path_edit.as_ref().is_some_and(|p| p.anchors.len() >= 2);
    for (label, command) in [
        ("Selection from Path", "path:select"),
        ("Fill Path", "path:fill"),
        ("Stroke Path…", "path:stroke"),
        ("Close Path", "path:close"),
        ("Delete Path", "path:delete"),
    ] {
        let r = Rect::new(x, y, w, 24);
        let live = match command {
            "path:delete" => st.path_edit.is_some(),
            "path:close" => ready && st.path_edit.as_ref().is_some_and(|p| !p.closed),
            _ => ready,
        };
        if live {
            view::button(p, s, r, label, &st.target(command), false);
        } else {
            view::inert(
                p,
                s,
                r,
                label,
                if command == "path:close" && ready {
                    "The path is already closed"
                } else {
                    "Draw a path with the Paths tool first"
                },
            );
        }
        y += 28;
    }
}

fn gimp_mode(mode: cw_raster::BlendMode) -> &'static str {
    match mode {
        cw_raster::BlendMode::Add => "Addition",
        cw_raster::BlendMode::Darken => "Darken only",
        cw_raster::BlendMode::Lighten => "Lighten only",
        other => other.label(),
    }
}

fn menus(st: &Studio, p: &mut Painter, s: &Skin) {
    let Some(Panel::Menu { id: open_path }) = &st.panel else {
        return;
    };
    let root = open_path.split('/').next().unwrap_or("");
    let doc = st.doc.as_ref();
    let open = doc.is_some();
    let has_sel = doc.and_then(|d| d.selection()).is_some();
    let undo = doc
        .and_then(|d| d.undo_label())
        .map(|l| format!("Undo {l}"));
    let redo = doc
        .and_then(|d| d.redo_label())
        .map(|l| format!("Redo {l}"));
    let overwrite = st
        .overwrite_target()
        .map(|p| format!("Overwrite {}", p.rsplit('/').next().unwrap_or("")));
    let has_path = st.path_edit.as_ref().is_some_and(|p| p.anchors.len() >= 2);
    let no_path = "There is no path; draw one with the Paths tool";
    let no = "No image is open";
    let adj =
        |label: &'static str, id: &str| Item::new(label, &format!("dialog:{id}")).when(open, no);
    let act =
        |label: &'static str, id: &str| Item::new(label, &format!("action:{id}")).when(open, no);
    let items_of = |id: &str| -> Vec<Item<'_>> {
        match id {
            "file" => vec![
                Item::new("New…", "dialog:new-image").key("Ctrl+N"),
                Item::new("Open…", "open").key("Ctrl+O"),
                Item::separator(),
                Item::new("Save", "save").key("Ctrl+S").when(open, no),
                Item::new("Save As…", "save-as")
                    .key("Shift+Ctrl+S")
                    .when(open, no),
                Item::separator(),
                match &overwrite {
                    Some(label) => Item::new(label, "overwrite"),
                    None => Item::new("Overwrite", "overwrite")
                        .when(false, "The image did not come from a PNG, JPEG or BMP file"),
                },
                Item::new("Export As…", "export")
                    .key("Shift+Ctrl+E")
                    .when(open, no),
            ],
            "edit" => vec![
                match &undo {
                    Some(label) => Item::new(label, "undo").key("Ctrl+Z"),
                    None => Item::new("Undo", "undo").when(false, "Nothing to undo"),
                },
                match &redo {
                    Some(label) => Item::new(label, "redo").key("Ctrl+Y"),
                    None => Item::new("Redo", "redo").when(false, "Nothing to redo"),
                },
                Item::separator(),
                Item::new("Cut", "cut")
                    .key("Ctrl+X")
                    .when(has_sel, "Nothing is selected"),
                Item::new("Copy", "copy").key("Ctrl+C").when(open, no),
                Item::new("Paste as New Layer", "paste")
                    .key("Ctrl+V")
                    .when(open, no),
                Item::new("Clear", "delete")
                    .key("Delete")
                    .when(has_sel, "Nothing is selected"),
                Item::separator(),
                Item::new("Fill Path", "path:fill").when(has_path, no_path),
                Item::new("Stroke Path…", "path:stroke").when(has_path, no_path),
            ],
            "select" => vec![
                Item::new("All", "select-all").key("Ctrl+A").when(open, no),
                Item::new("None", "select-none")
                    .key("Shift+Ctrl+A")
                    .when(has_sel, "Nothing is selected"),
                Item::new("Invert", "select-invert")
                    .key("Ctrl+I")
                    .when(open, no),
                Item::new("From Path", "path:select").when(has_path, no_path),
            ],
            "view" => vec![
                Item::new("Zoom In", "zoom:in").key("+").when(open, no),
                Item::new("Zoom Out", "zoom:out").key("-").when(open, no),
                Item::new("Fit Image in Window", "zoom:fit")
                    .key("Shift+Ctrl+J")
                    .checked(st.zoom == 0),
                Item::new("1:1 (100%)", "zoom:100")
                    .key("1")
                    .checked(st.zoom == 100),
                Item::new("2:1 (200%)", "zoom:200").checked(st.zoom == 200),
            ],
            "image" => vec![
                Item::new("Flip Horizontally", "flip:h").when(open, no),
                Item::new("Flip Vertically", "flip:v").when(open, no),
                Item::new("Rotate 90° clockwise", "rotate:cw").when(open, no),
                Item::new("Rotate 90° counter-clockwise", "rotate:ccw").when(open, no),
                Item::new("Rotate 180°", "rotate:180").when(open, no),
                adj("Arbitrary Rotation…", "rotate"),
                Item::separator(),
                Item::new("Crop to Selection", "crop-selection")
                    .when(has_sel, "Nothing is selected"),
                adj("Scale Image…", "resize"),
                Item::new("Flatten Image", "flatten").when(
                    doc.is_some_and(|d| d.layers().len() > 1),
                    "The image has one layer",
                ),
            ],
            "layer" => {
                let count = doc.map_or(0, |d| d.layers().len());
                let active = doc.map_or(0, |d| d.active());
                vec![
                    Item::new("New Layer…", "layer:new")
                        .key("Shift+Ctrl+N")
                        .when(open, no),
                    Item::new("Duplicate Layer", "layer:duplicate").when(open, no),
                    Item::new("Merge Down", "layer:merge")
                        .when(active > 0, "There is no layer below"),
                    Item::new("Delete Layer", "layer:delete")
                        .when(count > 1, "An image keeps at least one layer"),
                    Item::separator(),
                    Item::new("Raise Layer", "layer:up")
                        .when(active + 1 < count, "The layer is already at the top"),
                    Item::new("Lower Layer", "layer:down")
                        .when(active > 0, "The layer is already at the bottom"),
                ]
            }
            "colors" => vec![
                adj("Color Balance…", "color-balance"),
                adj("Color Temperature…", "temperature"),
                adj("Hue-Saturation…", "hue-saturation"),
                adj("Saturation…", "saturation"),
                adj("Exposure…", "exposure"),
                adj("Shadows-Highlights…", "shadows-highlights"),
                adj("Brightness-Contrast…", "brightness-contrast"),
                adj("Levels…", "levels"),
                adj("Curves…", "curves"),
                Item::separator(),
                act("Invert", "invert"),
                adj("Desaturate…", "desaturate"),
                adj("Auto ▸ Stretch Contrast…", "stretch-contrast"),
                Item::separator(),
                adj("Threshold…", "threshold"),
                adj("Posterize…", "posterize"),
            ],
            "tools" => st
                .product
                .tools()
                .iter()
                .map(|tool| {
                    Item::new(tool_name(*tool), &format!("tool:{}", tool.id()))
                        .checked(st.tool == *tool)
                })
                .collect(),
            "filters" => vec![
                adj("Blur ▸ Gaussian Blur…", "gaussian-blur"),
                adj("Blur ▸ Median Blur…", "median"),
                adj("Blur ▸ Pixelize…", "pixelate"),
                adj("Enhance ▸ Noise Reduction…", "noise-reduction"),
                adj("Enhance ▸ Sharpen (Unsharp Mask)…", "unsharp-mask"),
                act("Enhance ▸ Sharpen", "sharpen"),
                act("Edge-Detect ▸ Edge…", "edge-detect"),
                act("Distorts ▸ Emboss…", "emboss"),
                adj("Light and Shadow ▸ Vignette…", "vignette"),
            ],
            "blend" => cw_raster::BlendMode::ALL
                .iter()
                .map(|m| {
                    Item::new(gimp_mode(*m), &format!("layer:blend:{}", m.id()))
                        .checked(doc.is_some_and(|d| d.active_layer().blend == *m))
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    if root == "blend" {
        let items = items_of("blend");
        view::menu(p, s, st, p.scene.width as i32 - 240, 90, 260, &items);
        return;
    }
    let Some(entries) = REAL
        .iter()
        .zip(MENU_IDS)
        .find(|(_, id)| *id == root)
        .map(|((_, entries), _)| *entries)
    else {
        return;
    };
    // What the simulated GIMP does of each menu, by the row's name: a row it has no command
    // for is listed insensitive.
    let mut bound: BTreeMap<(String, String), Item<'_>> = BTreeMap::new();
    for rid in MENU_IDS {
        for item in items_of(rid) {
            if item.command.is_some() {
                let leaf = item.label.rsplit(" ▸ ").next().unwrap_or(item.label);
                bound.insert((rid.to_owned(), key(leaf)), item);
            }
        }
    }
    let mut x = 2;
    for (mid, label) in MENUS {
        if mid == root {
            break;
        }
        x += (p.measure(label, 12, false) * 5 / 4 + 12) as i32;
    }
    let z = p.z;
    p.z += 500;
    drop_down(
        p,
        s,
        st,
        &Drop {
            root,
            bound: &bound,
            open: open_path,
        },
        entries,
        root,
        x.max(0),
        27,
    );
    p.z = z;
}

/// Row height of GTK2's menus in GIMP's theme, and a separator's.
const ROW: u32 = 25;
const SEPARATOR: u32 = 16;

struct Drop<'a, 'b> {
    root: &'a str,
    bound: &'a BTreeMap<(String, String), Item<'b>>,
    /// The open menu path (`image/transform`).
    open: &'a str,
}

/// A row's name as the simulator's commands are keyed: without its ellipsis.
fn key(label: &str) -> String {
    label.trim_end_matches('…').trim().to_owned()
}

/// The path segment naming a submenu row.
fn slug(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Give the node just painted the role a toolkit gives the control it draws.
fn announce(p: &mut Painter, role: &str, selected: Option<bool>) {
    if let Some(n) = p.scene.nodes.last_mut() {
        if let Some(sem) = &mut n.semantic {
            sem.role = role.into();
        }
        if selected.is_some() {
            n.state = Some(cw_scene::NodeState {
                selected,
                ..Default::default()
            });
        }
    }
}

/// One drop-down of the menu bar at `(x, y)`, and, when a row of it is open, its submenu
/// beside that row. `trail` is the path of this drop-down (`image`, `image/transform`).
#[allow(clippy::too_many_arguments)]
fn drop_down(
    p: &mut Painter,
    s: &Skin,
    st: &Studio,
    d: &Drop<'_, '_>,
    entries: &[Entry],
    trail: &str,
    x: i32,
    y: i32,
) {
    let label_w = entries
        .iter()
        .map(|e| match e {
            Entry::Item(l) => p.measure(l, s.font, false) + 90,
            Entry::Sub(l, _) => p.measure(l, s.font, false) + 90,
            Entry::Sep => 0,
        })
        .max()
        .unwrap_or(0)
        .max(180);
    let height: u32 = entries
        .iter()
        .map(|e| {
            if matches!(e, Entry::Sep) {
                SEPARATOR
            } else {
                ROW
            }
        })
        .sum::<u32>()
        + 4;
    let r = Rect::new(x, y, label_w, height);
    p.drop_shadow(r, s.radius, 10, 60, 3);
    p.border(r, s.panel, s.radius, s.line);
    // The menu's own body absorbs clicks between rows, and is not a control.
    p.region(r, &st.target(&format!("noop:{}", p.scene.nodes.len())), "");
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(cw_scene::Semantic {
            role: "presentation".into(),
            label: String::new(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
    let mut cy = y + 2;
    let mut child: Option<(i32, i32, &[Entry], String)> = None;
    let menu = d.root;
    for e in entries {
        match e {
            Entry::Sep => {
                p.hline(x + 8, cy + SEPARATOR as i32 / 2, label_w - 16, s.line);
                cy += SEPARATOR as i32;
            }
            Entry::Sub(label, sub) => {
                let path = format!("{trail}/{}", slug(label));
                let on = d.open == path || d.open.starts_with(&format!("{path}/"));
                let rr = Rect::new(x + 2, cy, label_w - 4, ROW);
                p.button(
                    rr,
                    if on { s.selected } else { Color::TRANSPARENT },
                    0,
                    &st.target(&format!("menu:{path}")),
                    label,
                );
                announce(p, "menu", Some(on));
                p.label(
                    rr.x + 24,
                    rr.y + 5,
                    label_w - 60,
                    label,
                    s.font,
                    s.text,
                    false,
                    Align::Left,
                );
                p.symbol(
                    "chevron-right",
                    rr.x + rr.width as i32 - 22,
                    rr.y + 5,
                    14,
                    s.text,
                );
                if on {
                    child = Some((x + label_w as i32, cy - 2, *sub, path));
                }
                cy += ROW as i32;
            }
            Entry::Item(label) => {
                let rr = Rect::new(x + 2, cy, label_w - 4, ROW);
                let name = (*label).to_owned();
                match d.bound.get(&(menu.to_owned(), key(label))) {
                    Some(item) if item.why.is_empty() => {
                        let text = if matches!(*label, "Undo" | "Redo" | "Overwrite") {
                            item.label
                        } else {
                            label
                        };
                        p.button(
                            rr,
                            Color::TRANSPARENT,
                            0,
                            &st.target(item.command.as_deref().unwrap_or("noop")),
                            text,
                        );
                        announce(p, "menuitem", None);
                        if item.checked {
                            p.symbol("check", rr.x + 4, rr.y + 5, 14, s.text);
                        }
                        p.label(
                            rr.x + 24,
                            rr.y + 5,
                            label_w - 120,
                            text,
                            s.font,
                            s.text,
                            false,
                            Align::Left,
                        );
                        if !item.shortcut.is_empty() {
                            p.label(
                                rr.x + label_w as i32 - 110,
                                rr.y + 5,
                                90,
                                item.shortcut,
                                s.font.saturating_sub(1),
                                s.muted,
                                false,
                                Align::Right,
                            );
                        }
                    }
                    other => {
                        let why = other.map_or("This GIMP does not do that", |i| i.why);
                        p.box_(rr, Color::TRANSPARENT, 0);
                        p.disabled(why);
                        if let Some(n) = p.scene.nodes.last_mut() {
                            if let Some(sem) = &mut n.semantic {
                                sem.role = "menuitem".into();
                                sem.label = name.clone();
                            }
                        }
                        p.label(
                            rr.x + 24,
                            rr.y + 5,
                            label_w - 60,
                            &name,
                            s.font,
                            s.muted,
                            false,
                            Align::Left,
                        );
                    }
                }
                cy += ROW as i32;
            }
        }
    }
    if let Some((cx, cyy, sub, path)) = child {
        p.z += 1;
        drop_down(p, s, st, d, sub, &path, cx, cyy);
        p.z -= 1;
    }
}
