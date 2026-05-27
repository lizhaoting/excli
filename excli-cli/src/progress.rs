use std::time::Instant;

/// Tracks progress for long-running operations.
pub struct ProgressTracker {
    start_time: Instant,
    total: u64,
    current: u64,
    label: String,
    use_emoji: bool,
    quiet: bool,
}

impl ProgressTracker {
    pub fn new(label: impl Into<String>, total: u64, use_emoji: bool, quiet: bool) -> Self {
        ProgressTracker {
            start_time: Instant::now(),
            total,
            current: 0,
            label: label.into(),
            use_emoji,
            quiet,
        }
    }

    pub fn set_progress(&mut self, current: u64) {
        self.current = current;
        if !self.quiet {
            self.render();
        }
    }

    pub fn increment(&mut self, amount: u64) {
        self.current += amount;
        if !self.quiet {
            self.render();
        }
    }

    pub fn finish(&mut self, message: &str) {
        if self.quiet {
            return;
        }
        let elapsed = self.start_time.elapsed();
        if self.use_emoji {
            println!(
                "✅ {} — {} ({:.1}s)",
                self.label,
                message,
                elapsed.as_secs_f64()
            );
        } else {
            println!(
                "[SUCCESS] {} — {} ({:.1}s)",
                self.label,
                message,
                elapsed.as_secs_f64()
            );
        }
    }

    fn render(&self) {
        if self.total == 0 {
            return;
        }
        let pct = (self.current as f64 / self.total as f64 * 100.0) as u32;
        let bar_width = 30;
        let filled = (bar_width as f64 * pct as f64 / 100.0) as usize;
        let empty = bar_width - filled;
        let bar = format!(
            "{}{}",
            "█".repeat(filled),
            "░".repeat(empty)
        );

        let elapsed = self.start_time.elapsed();
        let rate = if elapsed.as_secs() > 0 {
            self.current / elapsed.as_secs()
        } else {
            0
        };

        let eta = if rate > 0 && self.total > self.current {
            (self.total - self.current) / rate
        } else {
            0
        };

        print!(
            "\r{} {} | {}% | {}/{} | {:.1}s | ETA {}s",
            self.label, bar, pct, self.current, self.total, elapsed.as_secs_f64(), eta
        );
    }
}

/// Simple spinner for indeterminate operations.
pub struct Spinner {
    frames: Vec<&'static str>,
    current: usize,
}

impl Spinner {
    pub fn new() -> Self {
        Spinner {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current: 0,
        }
    }

    pub fn tick(&mut self) -> &str {
        let frame = self.frames[self.current];
        self.current = (self.current + 1) % self.frames.len();
        frame
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_tracker() {
        let mut tracker = ProgressTracker::new("Testing", 100, false, false);
        tracker.set_progress(50);
        tracker.increment(25);
        tracker.finish("completed");
    }

    #[test]
    fn test_spinner() {
        let mut spinner = Spinner::new();
        let f1 = spinner.tick().to_string();
        let f2 = spinner.tick().to_string();
        assert_ne!(f1, f2);
    }
}
