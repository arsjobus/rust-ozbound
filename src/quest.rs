/// Minimal domain model for the starter collection quest.
/// Kept independent of Bevy so its rules are easy to test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoonpetalQuest {
    required: u8,
    collected: u8,
}

impl Default for MoonpetalQuest {
    fn default() -> Self {
        Self {
            required: 3,
            collected: 0,
        }
    }
}

impl MoonpetalQuest {
    pub fn required(&self) -> u8 {
        self.required
    }

    pub fn collected(&self) -> u8 {
        self.collected
    }

    pub fn is_complete(&self) -> bool {
        self.collected >= self.required
    }

    /// Adds one item if the objective is not already complete.
    /// Returns true only when the state changed.
    pub fn collect_one(&mut self) -> bool {
        if self.is_complete() {
            return false;
        }
        self.collected += 1;
        true
    }

    pub fn progress_text(&self) -> String {
        format!("Moonpetals: {}/{}", self.collected, self.required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_zero_progress() {
        let quest = MoonpetalQuest::default();
        assert_eq!(quest.collected(), 0);
        assert_eq!(quest.required(), 3);
        assert!(!quest.is_complete());
    }

    #[test]
    fn collecting_advances_progress_and_completes() {
        let mut quest = MoonpetalQuest::default();
        assert!(quest.collect_one());
        assert!(quest.collect_one());
        assert!(!quest.is_complete());
        assert!(quest.collect_one());
        assert!(quest.is_complete());
        assert_eq!(quest.progress_text(), "Moonpetals: 3/3");
    }

    #[test]
    fn completed_quest_cannot_over_collect() {
        let mut quest = MoonpetalQuest::default();
        for _ in 0..3 {
            assert!(quest.collect_one());
        }
        assert!(!quest.collect_one());
        assert_eq!(quest.collected(), 3);
    }
}
