use chrono::Local;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Note {
    pub filename: String,
    pub content: String,
    pub modified_epoch: i64,
    pub dirty: bool,
}

impl Note {
    /// Returns up to `max_lines` of note content starting from line 1 onward.
    pub fn preview_lines(&self, max_lines: usize) -> Vec<String> {
        self.content
            .lines()
            .take(max_lines)
            .map(|l| l.trim_end().to_string())
            .collect()
    }

    /// Returns a compact single-line excerpt for search result jump pills.
    pub fn compact_excerpt(&self, max_chars: usize) -> String {
        let first = self
            .content
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("Empty note");
        if first.chars().count() <= max_chars {
            first.to_string()
        } else {
            let mut s: String = first.chars().take(max_chars.saturating_sub(1)).collect();
            s.push('…');
            s
        }
    }
}

pub fn word_and_char_count(content: &str) -> (usize, usize, usize) {
    let lines = if content.is_empty() {
        0
    } else {
        content.split('\n').count()
    };
    let words = content.split_whitespace().count();
    let chars = content.chars().count();
    (lines, words, chars)
}

pub fn format_relative_time(epoch_secs: i64) -> String {
    let now = current_epoch_secs();
    let diff = (now - epoch_secs).max(0);
    if diff < 60 {
        "Just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86_400 {
        format!("{}h ago", diff / 3600)
    } else {
        let days = diff / 86_400;
        if days == 1 {
            "Yesterday".to_string()
        } else if days < 30 {
            format!("{days}d ago")
        } else {
            Local::now().format("%b %d").to_string()
        }
    }
}

fn current_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn file_modified_epoch(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or_else(current_epoch_secs, |d| d.as_secs() as i64)
}

pub struct NoteStore {
    pub notes_dir: PathBuf,
    pub notes: Vec<Note>,
    pub active_idx: usize,
    pub last_deleted: Option<(usize, Note)>,
}

impl NoteStore {
    pub fn load(
        notes_dir: PathBuf,
        saved_order: &[String],
        active_file: Option<&str>,
    ) -> Self {
        let _ = fs::create_dir_all(&notes_dir);

        let mut loaded = Vec::new();
        if let Ok(entries) = fs::read_dir(&notes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        let content = fs::read_to_string(&path).unwrap_or_default();
                        let modified_epoch = file_modified_epoch(&path);
                        loaded.push(Note {
                            filename: filename.to_string(),
                            content: content.replace("\r\n", "\n"),
                            modified_epoch,
                            dirty: false,
                        });
                    }
                }
            }
        }

        // Order according to saved_order first, then remaining notes sorted by modified time desc
        let mut ordered = Vec::with_capacity(loaded.len());
        let mut used = HashSet::new();
        for name in saved_order {
            if let Some(pos) = loaded.iter().position(|n| n.filename == *name) {
                if used.insert(pos) {
                    ordered.push(loaded[pos].clone());
                }
            }
        }
        let mut rest: Vec<Note> = loaded
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| !used.contains(idx))
            .map(|(_, n)| n)
            .collect();
        rest.sort_by(|a, b| {
            b.modified_epoch
                .cmp(&a.modified_epoch)
                .then_with(|| a.filename.cmp(&b.filename))
        });
        ordered.extend(rest);

        if ordered.is_empty() {
            let welcome = Self::default_welcome_note();
            let _ = fs::write(notes_dir.join(&welcome.filename), &welcome.content);
            ordered.push(welcome);
        }

        let active_idx = active_file
            .and_then(|target| ordered.iter().position(|n| n.filename == target))
            .unwrap_or(0)
            .min(ordered.len().saturating_sub(1));

        Self {
            notes_dir,
            notes: ordered,
            active_idx,
            last_deleted: None,
        }
    }

    fn default_welcome_note() -> Note {
        let content = "\
Fast, translucent Windows scratchpad — every note is a local .md file.

## Quick Shortcuts
- **Win+Shift+A**: Summon anywhere (auto-hides to tray when out of focus)
- **Alt + Scroll**: Flip horizontally between notes
- **Alt+A**: Half-screen At-a-Glance grid overlay
- **Ctrl+N**: New note   |   **Ctrl+,**: Settings

## Checklist & Inline Math
- [ ] Click any [ ] checkbox to toggle it directly (or press Ctrl+L)
- [ ] Hover top edge for quick icons (New, Grid, Copy, Pin, Settings, Hide)
- [ ] Hover bottom edge for horizontal note navigation
- [x] Courier New default font & Markdown editing

items = 3
12.50 * items =
20% of 150 =
"
        .to_string();

        Note {
            filename: "note-1.md".to_string(),
            content,
            modified_epoch: current_epoch_secs(),
            dirty: false,
        }
    }

    pub fn active_note(&self) -> Option<&Note> {
        self.notes.get(self.active_idx)
    }

    pub fn active_note_mut(&mut self) -> Option<&mut Note> {
        self.notes.get_mut(self.active_idx)
    }

    pub fn note_order(&self) -> Vec<String> {
        self.notes.iter().map(|n| n.filename.clone()).collect()
    }

    pub fn active_filename(&self) -> Option<String> {
        self.active_note().map(|n| n.filename.clone())
    }

    fn is_filename_available(&self, candidate: &str) -> bool {
        !self
            .notes
            .iter()
            .any(|n| n.filename.eq_ignore_ascii_case(candidate))
            && !self.notes_dir.join(candidate).exists()
    }

    fn next_note_filename(&self) -> String {
        for n in 1..=99999 {
            let cand = format!("note-{n}.md");
            if self.is_filename_available(&cand) {
                return cand;
            }
        }
        format!("note-{}.md", current_epoch_secs())
    }

    pub fn save_note(&mut self, idx: usize) -> Result<(), String> {
        if idx >= self.notes.len() {
            return Ok(());
        }
        let _ = fs::create_dir_all(&self.notes_dir);

        let path = self.notes_dir.join(&self.notes[idx].filename);
        fs::write(&path, &self.notes[idx].content).map_err(|e| e.to_string())?;
        self.notes[idx].modified_epoch = current_epoch_secs();
        self.notes[idx].dirty = false;
        Ok(())
    }

    pub fn save_all_dirty(&mut self) {
        for idx in 0..self.notes.len() {
            if self.notes[idx].dirty {
                let _ = self.save_note(idx);
            }
        }
    }

    pub fn create_note(&mut self) -> usize {
        self.save_all_dirty();
        let idx = self.notes.len();
        let filename = self.next_note_filename();
        let note = Note {
            filename,
            content: String::new(),
            modified_epoch: current_epoch_secs(),
            dirty: false,
        };
        let _ = fs::create_dir_all(&self.notes_dir);
        let _ = fs::write(self.notes_dir.join(&note.filename), "");
        self.notes.push(note);
        self.active_idx = idx;
        idx
    }

    pub fn delete_note(&mut self, idx: usize) -> bool {
        if idx >= self.notes.len() {
            return false;
        }
        let removed = self.notes.remove(idx);
        let _ = fs::remove_file(self.notes_dir.join(&removed.filename));
        self.last_deleted = Some((idx, removed));

        if self.notes.is_empty() {
            self.create_note();
        } else if self.active_idx >= self.notes.len() {
            self.active_idx = self.notes.len() - 1;
        } else if idx < self.active_idx {
            self.active_idx = self.active_idx.saturating_sub(1);
        }

        true
    }

    pub fn undo_delete(&mut self) -> bool {
        let Some((orig_idx, mut note)) = self.last_deleted.take() else {
            return false;
        };
        let _ = fs::create_dir_all(&self.notes_dir);
        if !self.is_filename_available(&note.filename) {
            note.filename = self.next_note_filename();
        }
        note.dirty = false;
        let _ = fs::write(self.notes_dir.join(&note.filename), &note.content);
        let insert_idx = orig_idx.min(self.notes.len());
        self.notes.insert(insert_idx, note);
        self.active_idx = insert_idx;
        true
    }

    pub fn switch_to(&mut self, idx: usize, auto_delete_empty: bool) -> bool {
        if idx >= self.notes.len() || idx == self.active_idx {
            return false;
        }
        let prev = self.active_idx;
        self.save_all_dirty();

        if auto_delete_empty && self.notes.len() > 1 {
            if let Some(prev_note) = self.notes.get(prev) {
                if prev_note.content.trim().is_empty() {
                    let target_filename = self.notes[idx].filename.clone();
                    let removed = self.notes.remove(prev);
                    let _ = fs::remove_file(self.notes_dir.join(&removed.filename));
                    self.active_idx = self
                        .notes
                        .iter()
                        .position(|n| n.filename == target_filename)
                        .unwrap_or(0);
                    return true;
                }
            }
        }

        self.active_idx = idx;
        true
    }

    pub fn step_note(&mut self, delta: isize, auto_delete_empty: bool) -> Option<usize> {
        let len = self.notes.len();
        if len <= 1 || delta == 0 {
            return None;
        }
        let target = ((self.active_idx as isize) + delta).clamp(0, (len - 1) as isize) as usize;
        if target == self.active_idx {
            return None;
        }
        if self.switch_to(target, auto_delete_empty) {
            Some(self.active_idx)
        } else {
            None
        }
    }

    pub fn reload_dir(&mut self, new_dir: PathBuf, saved_order: &[String]) {
        self.save_all_dirty();
        let active_name = self.active_filename();
        *self = Self::load(new_dir, saved_order, active_name.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_notes_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rapid_notes_test_{name}_{}_{}",
            std::process::id(),
            current_epoch_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_preview_lines_starts_from_line_one() {
        let note = Note {
            filename: "note-1.md".to_string(),
            content: "First line\nSecond line\nThird line\nFourth".to_string(),
            modified_epoch: 0,
            dirty: false,
        };
        assert_eq!(
            note.preview_lines(3),
            vec!["First line", "Second line", "Third line"]
        );
        assert_eq!(note.compact_excerpt(20), "First line");
    }

    #[test]
    fn test_note_store_stable_filenames_crud_and_undo() {
        let dir = temp_notes_dir("stable_crud");
        let mut store = NoteStore::load(dir.clone(), &[], None);
        assert_eq!(store.notes.len(), 1);
        assert_eq!(store.notes[0].filename, "note-1.md");

        let idx1 = store.create_note();
        assert_eq!(store.notes[idx1].filename, "note-2.md");
        store.notes[idx1].content = "First row is just text\n- Glass UI".to_string();
        store.notes[idx1].dirty = true;
        store.save_note(idx1).unwrap();
        // Filename stays stable regardless of note content
        assert_eq!(store.notes[idx1].filename, "note-2.md");
        assert!(dir.join("note-2.md").exists());

        let idx2 = store.create_note();
        assert_eq!(store.notes[idx2].filename, "note-3.md");

        assert!(store.delete_note(idx2));
        assert!(!dir.join("note-3.md").exists());
        assert!(store.undo_delete());
        assert!(dir.join("note-3.md").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_word_and_char_count() {
        assert_eq!(word_and_char_count(""), (0, 0, 0));
        assert_eq!(word_and_char_count("Hello world\nTwo"), (2, 3, 15));
    }
}
