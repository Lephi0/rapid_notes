use egui::{
    text::{LayoutJob, TextFormat},
    Color32, CornerRadius, FontFamily, FontId, Frame, Margin, Stroke, Ui,
};
use egui_glass_ui::{
    GlassPalette, EDITOR_BOLD_FONT_FAMILY, EDITOR_FONT_FAMILY, EDITOR_ITALIC_FONT_FAMILY,
};

fn editor_regular_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

fn editor_bold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(EDITOR_BOLD_FONT_FAMILY.into()))
}

fn editor_italic_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(EDITOR_ITALIC_FONT_FAMILY.into()))
}

fn append_span(job: &mut LayoutJob, text: &str, format: TextFormat) {
    if !text.is_empty() {
        job.append(text, 0.0, format);
    }
}

fn plain_format(font_id: FontId, color: Color32) -> TextFormat {
    TextFormat {
        font_id,
        color,
        ..Default::default()
    }
}

/// Builds an `egui::text::LayoutJob` with syntax styling while preserving 100% 1-to-1 byte parity
/// with `text` so `egui::TextEdit` cursor positions, selections, and undo buffers remain exact.
pub fn build_markdown_layout_job(
    text: &str,
    wrap_width: f32,
    font_size: f32,
    palette: &GlassPalette,
    highlight_enabled: bool,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;

    if !highlight_enabled {
        job.append(
            text,
            0.0,
            plain_format(editor_regular_font(font_size), palette.primary_text),
        );
        return job;
    }

    let mut in_code_fence = false;

    for raw_line in text.split_inclusive('\n') {
        let (line_body, newline) = if let Some(stripped) = raw_line.strip_suffix("\r\n") {
            (stripped, "\r\n")
        } else if let Some(stripped) = raw_line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (raw_line, "")
        };

        let trimmed = line_body.trim_start();
        let leading_len = line_body.len() - trimmed.len();
        let leading_ws = &line_body[..leading_len];

        // Code fence toggle (```)
        if trimmed.starts_with("```") {
            append_span(
                &mut job,
                line_body,
                TextFormat {
                    font_id: editor_regular_font((font_size * 0.92).max(9.0)),
                    color: palette.syntax_muted,
                    background: palette.code_bg,
                    ..Default::default()
                },
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            in_code_fence = !in_code_fence;
            continue;
        }

        if in_code_fence {
            append_span(
                &mut job,
                line_body,
                TextFormat {
                    font_id: editor_regular_font((font_size * 0.94).max(9.0)),
                    color: palette.secondary_text,
                    background: palette.code_bg,
                    ..Default::default()
                },
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            continue;
        }

        // Headings (#, ##, ###, ...)
        if let Some(prefix_len) = heading_prefix_len(trimmed) {
            let hashes = trimmed[..prefix_len].trim_end().len();
            let scale = match hashes {
                1 => 1.22,
                2 => 1.14,
                _ => 1.07,
            };
            let h_size = (font_size * scale).round();
            append_span(
                &mut job,
                leading_ws,
                plain_format(editor_bold_font(h_size), palette.primary_text),
            );
            append_span(
                &mut job,
                &trimmed[..prefix_len],
                plain_format(editor_regular_font(h_size), palette.syntax_muted),
            );
            append_inline_markdown(
                &mut job,
                &trimmed[prefix_len..],
                h_size,
                palette,
                true,
                palette.primary_text,
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_bold_font(h_size), palette.primary_text),
            );
            continue;
        }

        // Horizontal rule (---, ***, ___)
        if is_horizontal_rule(trimmed) {
            append_span(
                &mut job,
                line_body,
                plain_format(editor_regular_font(font_size), palette.syntax_muted),
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            continue;
        }

        // Blockquote (> ...)
        if trimmed.starts_with('>') {
            let prefix_len = if trimmed.starts_with("> ") { 2 } else { 1 };
            append_span(
                &mut job,
                leading_ws,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            append_span(
                &mut job,
                &trimmed[..prefix_len],
                plain_format(editor_bold_font(font_size), palette.accent_fill),
            );
            append_span(
                &mut job,
                &trimmed[prefix_len..],
                plain_format(editor_italic_font(font_size), palette.secondary_text),
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            continue;
        }

        // Task list item (- [ ] / - [x])
        if let Some((marker_len, checked)) = checklist_prefix_len(trimmed) {
            append_span(
                &mut job,
                leading_ws,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            let marker_color = if checked {
                palette.success_text
            } else {
                palette.accent_text
            };
            append_span(
                &mut job,
                &trimmed[..marker_len],
                plain_format(editor_bold_font(font_size), marker_color),
            );
            let text_color = if checked {
                palette.muted_text
            } else {
                palette.primary_text
            };
            append_inline_markdown(
                &mut job,
                &trimmed[marker_len..],
                font_size,
                palette,
                false,
                text_color,
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            continue;
        }

        // Bullet or numbered list
        if let Some(marker_len) = list_prefix_len(trimmed) {
            append_span(
                &mut job,
                leading_ws,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            append_span(
                &mut job,
                &trimmed[..marker_len],
                plain_format(editor_bold_font(font_size), palette.accent_text),
            );
            append_inline_markdown(
                &mut job,
                &trimmed[marker_len..],
                font_size,
                palette,
                false,
                palette.primary_text,
            );
            append_span(
                &mut job,
                newline,
                plain_format(editor_regular_font(font_size), palette.primary_text),
            );
            continue;
        }

        // Standard paragraph line
        append_inline_markdown(
            &mut job,
            line_body,
            font_size,
            palette,
            false,
            palette.primary_text,
        );
        append_span(
            &mut job,
            newline,
            plain_format(editor_regular_font(font_size), palette.primary_text),
        );
    }

    job
}

fn heading_prefix_len(trimmed: &str) -> Option<usize> {
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        Some(hashes + 1)
    } else {
        None
    }
}

fn is_horizontal_rule(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.as_bytes()[0];
    matches!(first, b'-' | b'*' | b'_')
        && trimmed
            .bytes()
            .filter(|&b| b != b' ')
            .all(|b| b == first)
        && trimmed.bytes().filter(|&b| b == first).count() >= 3
}

fn checklist_prefix_len(trimmed: &str) -> Option<(usize, bool)> {
    for prefix in ["- [ ] ", "* [ ] "] {
        if trimmed.starts_with(prefix) {
            return Some((prefix.len(), false));
        }
    }
    for prefix in ["- [x] ", "- [X] ", "* [x] ", "* [X] "] {
        if trimmed.starts_with(prefix) {
            return Some((prefix.len(), true));
        }
    }
    None
}

fn list_prefix_len(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return Some(2);
    }
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        return Some(digits + 2);
    }
    None
}

fn append_inline_markdown(
    job: &mut LayoutJob,
    line: &str,
    font_size: f32,
    palette: &GlassPalette,
    force_bold: bool,
    base_color: Color32,
) {
    let base_font = if force_bold {
        editor_bold_font(font_size)
    } else {
        editor_regular_font(font_size)
    };
    let mut i = 0;

    while i < line.len() {
        let rest = &line[i..];

        // Inline code: `...`
        if rest.starts_with('`') {
            if let Some(end_rel) = rest[1..].find('`') {
                let code_end = i + 1 + end_rel + 1;
                append_span(
                    job,
                    &line[i..i + 1],
                    plain_format(editor_regular_font(font_size), palette.syntax_muted),
                );
                append_span(
                    job,
                    &line[i + 1..code_end - 1],
                    TextFormat {
                        font_id: editor_regular_font((font_size * 0.94).max(9.0)),
                        color: palette.primary_text,
                        background: palette.code_bg,
                        ..Default::default()
                    },
                );
                append_span(
                    job,
                    &line[code_end - 1..code_end],
                    plain_format(editor_regular_font(font_size), palette.syntax_muted),
                );
                i = code_end;
                continue;
            }
        }

        // Bold: **...**
        if rest.starts_with("**") && rest.len() > 4 {
            if let Some(end_rel) = rest[2..].find("**") {
                if end_rel > 0 {
                    let end = i + 2 + end_rel + 2;
                    append_span(
                        job,
                        &line[i..i + 2],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    append_span(
                        job,
                        &line[i + 2..end - 2],
                        plain_format(editor_bold_font(font_size), base_color),
                    );
                    append_span(
                        job,
                        &line[end - 2..end],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    i = end;
                    continue;
                }
            }
        }

        // Strikethrough: ~~...~~
        if rest.starts_with("~~") && rest.len() > 4 {
            if let Some(end_rel) = rest[2..].find("~~") {
                if end_rel > 0 {
                    let end = i + 2 + end_rel + 2;
                    append_span(
                        job,
                        &line[i..i + 2],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    append_span(
                        job,
                        &line[i + 2..end - 2],
                        TextFormat {
                            font_id: base_font.clone(),
                            color: palette.muted_text,
                            strikethrough: Stroke::new(1.0_f32, palette.muted_text),
                            ..Default::default()
                        },
                    );
                    append_span(
                        job,
                        &line[end - 2..end],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    i = end;
                    continue;
                }
            }
        }

        // Italic: *...*
        if rest.starts_with('*') && !rest.starts_with("**") && rest.len() > 2 {
            if let Some(end_rel) = rest[1..].find('*') {
                if end_rel > 0 {
                    let end = i + 1 + end_rel + 1;
                    append_span(
                        job,
                        &line[i..i + 1],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    append_span(
                        job,
                        &line[i + 1..end - 1],
                        plain_format(editor_italic_font(font_size), base_color),
                    );
                    append_span(
                        job,
                        &line[end - 1..end],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    i = end;
                    continue;
                }
            }
        }

        // Markdown link: [label](url)
        if rest.starts_with('[') {
            if let Some(close_bracket) = rest.find("](") {
                if let Some(close_paren_rel) = rest[close_bracket + 2..].find(')') {
                    let label_start = i + 1;
                    let label_end = i + close_bracket;
                    let url_start = label_end + 2;
                    let url_end = url_start + close_paren_rel;
                    append_span(
                        job,
                        &line[i..label_start],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    append_span(
                        job,
                        &line[label_start..label_end],
                        TextFormat {
                            font_id: base_font.clone(),
                            color: palette.link_text,
                            underline: Stroke::new(1.0_f32, palette.link_text),
                            ..Default::default()
                        },
                    );
                    append_span(
                        job,
                        &line[label_end..url_end + 1],
                        plain_format(editor_regular_font(font_size), palette.syntax_muted),
                    );
                    i = url_end + 1;
                    continue;
                }
            }
        }

        // Bare URL: https://... or http://...
        if rest.starts_with("https://") || rest.starts_with("http://") {
            let url_len = rest
                .find(|c: char| c.is_whitespace() || matches!(c, ')' | ']' | '>'))
                .unwrap_or(rest.len());
            append_span(
                job,
                &line[i..i + url_len],
                TextFormat {
                    font_id: base_font.clone(),
                    color: palette.link_text,
                    underline: Stroke::new(1.0_f32, palette.link_text),
                    ..Default::default()
                },
            );
            i += url_len;
            continue;
        }

        // Advance cleanly up to next syntax trigger character
        let next_trigger = rest[1..]
            .char_indices()
            .find(|&(_, c)| matches!(c, '`' | '*' | '~' | '[' | 'h'))
            .map_or(line.len(), |(offset, _)| i + 1 + offset);

        append_span(
            job,
            &line[i..next_trigger],
            plain_format(base_font.clone(), base_color),
        );
        i = next_trigger;
    }
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(b, _)| b)
}

fn byte_to_char_idx(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx.min(s.len())].chars().count()
}

/// Smart Enter continuation for lists and checklists.
/// Returns `Some(new_cursor_char_idx)` if handled (caller should NOT insert plain Enter again).
pub fn handle_smart_enter(content: &mut String, cursor_char_idx: usize) -> Option<usize> {
    let cursor_byte = char_to_byte_idx(content, cursor_char_idx);
    let line_start = content[..cursor_byte].rfind('\n').map_or(0, |p| p + 1);
    let before_cursor = &content[line_start..cursor_byte];
    let after_on_line = content[cursor_byte..]
        .find('\n')
        .map_or(&content[cursor_byte..], |p| &content[cursor_byte..cursor_byte + p]);

    let leading_spaces: String = before_cursor
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let trimmed = &before_cursor[leading_spaces.len()..];

    let (prefix_len, next_prefix) = if trimmed.starts_with("- [ ] ")
        || trimmed.starts_with("- [x] ")
        || trimmed.starts_with("- [X] ")
    {
        (6usize, format!("{leading_spaces}- [ ] "))
    } else if trimmed.starts_with("- ") {
        (2usize, format!("{leading_spaces}- "))
    } else if trimmed.starts_with("* ") {
        (2usize, format!("{leading_spaces}* "))
    } else {
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && trimmed[digits..].starts_with(". ") {
            let num: usize = trimmed[..digits].parse().unwrap_or(1);
            (digits + 2, format!("{leading_spaces}{}. ", num + 1))
        } else {
            return None;
        }
    };

    // If pressing Enter on an empty list item (no content after prefix), exit the list cleanly
    if trimmed.len() == prefix_len && after_on_line.trim().is_empty() {
        let line_end = line_start + before_cursor.len() + after_on_line.len();
        content.replace_range(line_start..line_end, "");
        return Some(byte_to_char_idx(content, line_start));
    }

    let insertion = format!("\n{next_prefix}");
    content.insert_str(cursor_byte, &insertion);
    let new_byte = cursor_byte + insertion.len();
    Some(byte_to_char_idx(content, new_byte))
}

/// Toggles `- [ ] ` <-> `- [x] ` on the cursor's current line, or adds `- [ ] ` if plain text.
pub fn toggle_checklist_at_cursor(content: &mut String, cursor_char_idx: usize) -> usize {
    let cursor_byte = char_to_byte_idx(content, cursor_char_idx);
    let line_start = content[..cursor_byte].rfind('\n').map_or(0, |p| p + 1);
    let line_end = content[cursor_byte..]
        .find('\n')
        .map_or(content.len(), |p| cursor_byte + p);
    let line = content[line_start..line_end].to_string();
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let rest = &line[indent_len..];

    let (replacement, byte_delta): (String, isize) =
        if let Some(after) = rest.strip_prefix("- [ ] ") {
            (format!("{indent}- [x] {after}"), 0)
        } else if let Some(after) = rest
            .strip_prefix("- [x] ")
            .or_else(|| rest.strip_prefix("- [X] "))
        {
            (format!("{indent}- [ ] {after}"), 0)
        } else if let Some(after) = rest.strip_prefix("- ") {
            (format!("{indent}- [ ] {after}"), 4)
        } else if let Some(after) = rest.strip_prefix("* ") {
            (format!("{indent}- [ ] {after}"), 4)
        } else {
            (format!("{indent}- [ ] {rest}"), 6)
        };

    content.replace_range(line_start..line_end, &replacement);
    let new_byte = ((cursor_byte as isize + byte_delta).max(line_start as isize)) as usize;
    byte_to_char_idx(content, new_byte.min(content.len()))
}

/// Wraps the selected range `[start_char, end_char]` (or inserts empty delimiters at cursor)
/// with `prefix` and `suffix`, returning the new selection `(start_char, end_char)`.
pub fn wrap_selection(
    content: &mut String,
    start_char: usize,
    end_char: usize,
    prefix: &str,
    suffix: &str,
) -> (usize, usize) {
    let (a, b) = if start_char <= end_char {
        (start_char, end_char)
    } else {
        (end_char, start_char)
    };
    let start_byte = char_to_byte_idx(content, a);
    let end_byte = char_to_byte_idx(content, b);

    if a == b {
        let both = format!("{prefix}{suffix}");
        content.insert_str(start_byte, &both);
        let inner = a + prefix.chars().count();
        (inner, inner)
    } else {
        let selected = content[start_byte..end_byte].to_string();
        let replaced = format!("{prefix}{selected}{suffix}");
        content.replace_range(start_byte..end_byte, &replaced);
        let p_chars = prefix.chars().count();
        (a + p_chars, b + p_chars)
    }
}

/// Renders an interactive Markdown preview with clickable checklist checkboxes.
/// Returns `true` if any checkbox was toggled in `content`.
pub fn render_markdown_preview(
    ui: &mut Ui,
    content: &mut String,
    font_size: f32,
    palette: &GlassPalette,
) -> bool {
    let lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let mut toggle_line: Option<(usize, bool)> = None;
    let mut in_code_fence = false;
    let mut code_buffer = String::new();

    ui.spacing_mut().item_spacing.y = 4.0;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code_fence {
                render_code_block(ui, &code_buffer, font_size, palette);
                code_buffer.clear();
                in_code_fence = false;
            } else {
                in_code_fence = true;
            }
            continue;
        }
        if in_code_fence {
            if !code_buffer.is_empty() {
                code_buffer.push('\n');
            }
            code_buffer.push_str(line);
            continue;
        }

        if trimmed.is_empty() {
            ui.add_space(font_size * 0.45);
            continue;
        }

        if is_horizontal_rule(trimmed) {
            let (rect, _) =
                ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), 8.0), egui::Sense::hover());
            let y = rect.center().y;
            ui.painter().line_segment(
                [egui::Pos2::new(rect.min.x, y), egui::Pos2::new(rect.max.x, y)],
                Stroke::new(1.0_f32, palette.divider),
            );
            continue;
        }

        if let Some(prefix_len) = heading_prefix_len(trimmed) {
            let hashes = trimmed[..prefix_len].trim_end().len();
            let scale = match hashes {
                1 => 1.22,
                2 => 1.14,
                _ => 1.07,
            };
            let h_size = (font_size * scale).round();
            render_rich_line(
                ui,
                &trimmed[prefix_len..],
                h_size,
                palette,
                true,
                palette.primary_text,
            );
            continue;
        }

        if let Some((prefix_len, checked)) = checklist_prefix_len(trimmed) {
            let body = &trimmed[prefix_len..];
            ui.horizontal(|ui| {
                let mut val = checked;
                if ui.checkbox(&mut val, "").changed() {
                    toggle_line = Some((idx, val));
                }
                let color = if checked {
                    palette.muted_text
                } else {
                    palette.primary_text
                };
                render_rich_line(ui, body, font_size, palette, false, color);
            });
            continue;
        }

        if trimmed.starts_with('>') {
            let body = trimmed.trim_start_matches('>').trim_start();
            Frame::NONE
                .fill(palette.card_fill)
                .inner_margin(Margin::symmetric(10, 4))
                .stroke(Stroke::new(1.0_f32, palette.card_stroke))
                .corner_radius(CornerRadius::same(6))
                .show(ui, |ui| {
                    render_rich_line(ui, body, font_size, palette, false, palette.secondary_text);
                });
            continue;
        }

        if let Some(marker_len) = list_prefix_len(trimmed) {
            let marker = trimmed[..marker_len].trim();
            let body = &trimmed[marker_len..];
            ui.horizontal_top(|ui| {
                ui.label(
                    egui::RichText::new(if marker == "-" || marker == "*" {
                        "•".to_string()
                    } else {
                        marker.to_string()
                    })
                    .font(editor_bold_font(font_size))
                    .color(palette.accent_text),
                );
                render_rich_line(ui, body, font_size, palette, false, palette.primary_text);
            });
            continue;
        }

        render_rich_line(ui, trimmed, font_size, palette, false, palette.primary_text);
    }

    if in_code_fence && !code_buffer.is_empty() {
        render_code_block(ui, &code_buffer, font_size, palette);
    }

    if let Some((line_idx, checked)) = toggle_line {
        let mut updated = lines;
        if let Some(target) = updated.get_mut(line_idx) {
            if checked {
                *target = target.replacen("- [ ] ", "- [x] ", 1).replacen("* [ ] ", "* [x] ", 1);
            } else {
                *target = target
                    .replacen("- [x] ", "- [ ] ", 1)
                    .replacen("- [X] ", "- [ ] ", 1)
                    .replacen("* [x] ", "* [ ] ", 1)
                    .replacen("* [X] ", "* [ ] ", 1);
            }
            *content = updated.join("\n");
            return true;
        }
    }

    false
}

fn render_code_block(ui: &mut Ui, code: &str, font_size: f32, palette: &GlassPalette) {
    Frame::NONE
        .fill(palette.code_bg)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .stroke(Stroke::new(1.0_f32, palette.card_stroke))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(code)
                    .font(editor_regular_font((font_size * 0.92).max(9.0)))
                    .color(palette.secondary_text),
            );
        });
}

fn render_rich_line(
    ui: &mut Ui,
    line: &str,
    font_size: f32,
    palette: &GlassPalette,
    force_bold: bool,
    base_color: Color32,
) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width().max(80.0);
    append_inline_markdown(&mut job, line, font_size, palette, force_bold, base_color);
    ui.label(job);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_job_preserves_exact_bytes() {
        let sample = "# Title\n\n## Section **bold** *italic* `code`\n- [ ] Task one\n- [x] Done\n> Quote [link](https://example.com)\n```\ncode block\n```\n";
        let palette = GlassPalette::for_theme(false);
        let job = build_markdown_layout_job(sample, 420.0, 15.0, &palette, true);
        assert_eq!(job.text, sample);

        let mut covered = 0;
        for sec in &job.sections {
            assert_eq!(sec.byte_range.start, covered);
            assert!(sec.byte_range.end >= sec.byte_range.start);
            covered = sec.byte_range.end;
        }
        assert_eq!(covered, sample.len());

        // Plain first row uses regular editor font size (no special title scaling)
        let plain = build_markdown_layout_job("Just plain text\nSecond", 420.0, 15.0, &palette, true);
        assert_eq!(plain.sections[0].format.font_id.size, 15.0);
    }

    #[test]
    fn test_smart_enter_and_checklist_toggle() {
        let mut s = "- [ ] First".to_string();
        let len0 = s.chars().count();
        let pos = handle_smart_enter(&mut s, len0).unwrap();
        assert_eq!(s, "- [ ] First\n- [ ] ");
        assert_eq!(pos, s.chars().count());

        // Pressing Enter on empty bullet exits list
        let pos2 = handle_smart_enter(&mut s, pos).unwrap();
        assert_eq!(s, "- [ ] First\n");
        assert_eq!(pos2, s.chars().count());

        let mut item = "Buy coffee".to_string();
        let c1 = toggle_checklist_at_cursor(&mut item, 3);
        assert_eq!(item, "- [ ] Buy coffee");
        assert_eq!(c1, 9);
        toggle_checklist_at_cursor(&mut item, c1);
        assert_eq!(item, "- [x] Buy coffee");
    }

    #[test]
    fn test_wrap_selection() {
        let mut s = "hello world".to_string();
        let (a, b) = wrap_selection(&mut s, 6, 11, "**", "**");
        assert_eq!(s, "hello **world**");
        assert_eq!((a, b), (8, 13));
    }
}
